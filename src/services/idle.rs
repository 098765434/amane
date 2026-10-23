mod screensaver;

use std::sync::PoisonError;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::store;
use crate::Service;
use crate::changes;

// set when the levels change, taken by the event loop when it wakes
static REQUESTED: AtomicBool = AtomicBool::new(false);

// the backend only asks the compositor for idle timers once a shell uses Idle
static WANTED: AtomicBool = AtomicBool::new(false);

// dim after two minutes, lock after five, until the shell picks its own
const DEFAULT_LEVELS: [Duration; 2] = [Duration::from_secs(120), Duration::from_secs(300)];

pub struct Idle {
    levels: Vec<Duration>,

    // the levels the user has been away longer than, cleared by any input
    crossed: Vec<Duration>,

    // programs that asked through org.freedesktop.ScreenSaver not to idle
    inhibitors: Vec<Inhibitor>,

    // the spec keeps 0 for "no inhibit", so cookies start at 1
    next_cookie: u32,

    running: bool,
}

struct Inhibitor {
    cookie: u32,

    // the unique name like :1.42, so a program that quits without UnInhibit is let go
    sender: String,

    application: String,
    reason: String,
}

/*
 * the compositor says when the user went idle and came back, and amane
 * serves org.freedesktop.ScreenSaver so video players can hold it off
 */
impl Service for Idle {
    fn new() -> Self {
        WANTED.store(true, Ordering::Relaxed);
        REQUESTED.store(true, Ordering::Relaxed);

        // the event loop may already be waiting, it makes the timers once it wakes
        changes::mark_all();

        Self {
            levels: DEFAULT_LEVELS.to_vec(),
            crossed: Vec::new(),
            inhibitors: Vec::new(),
            next_cookie: 1,
            running: false,
        }
    }

    fn listen() {
        screensaver::run();
    }
}

impl Idle {
    // away longer than at least one level
    pub fn is_idle(&self) -> bool {
        !self.crossed.is_empty()
    }

    pub fn crossed(&self, level: Duration) -> bool {
        self.crossed.contains(&level)
    }

    pub fn levels(&self) -> &[Duration] {
        &self.levels
    }

    // a program like a video player asked not to idle, so check this before dimming or locking
    pub fn inhibited(&self) -> bool {
        !self.inhibitors.is_empty()
    }

    // the application name and reason of each inhibit
    pub fn inhibitors(&self) -> impl Iterator<Item = (&str, &str)> {
        self.inhibitors
            .iter()
            .map(|inhibitor| (inhibitor.application.as_str(), inhibitor.reason.as_str()))
    }

    // false when a desktop's own screensaver service already owns the name
    pub fn running(&self) -> bool {
        self.running
    }

    // the times without input to be told about, replacing the defaults
    pub fn set_levels(levels: &[Duration]) {
        {
            let mut idle = Self::write();

            idle.levels = levels.to_vec();
            idle.crossed.clear();
        }

        REQUESTED.store(true, Ordering::Relaxed);

        changes::mark_all();
    }

    pub(crate) fn wanted() -> bool {
        WANTED.load(Ordering::Relaxed)
    }

    pub(crate) fn take_request() -> bool {
        REQUESTED.swap(false, Ordering::Relaxed)
    }

    // for the backend, which is not a view, so this read subscribes no window
    pub(crate) fn current_levels() -> Vec<Duration> {
        store::find::<Self>()
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .levels
            .clone()
    }

    pub(crate) fn idled(level: Duration) {
        let mut idle = Self::write();

        if !idle.cross(level) {
            idle.quiet();
        }
    }

    pub(crate) fn resumed(level: Duration) {
        let mut idle = Self::write();

        if !idle.uncross(level) {
            idle.quiet();
        }
    }

    // timers made again start from zero, so none is crossed yet
    pub(crate) fn reset_crossed() {
        let mut idle = Self::write();

        if idle.crossed.is_empty() {
            idle.quiet();
        }

        idle.crossed.clear();
    }

    // each returns whether anything changed

    fn cross(&mut self, level: Duration) -> bool {
        if self.crossed.contains(&level) {
            return false;
        }

        self.crossed.push(level);

        true
    }

    fn uncross(&mut self, level: Duration) -> bool {
        let before = self.crossed.len();

        self.crossed.retain(|crossed| *crossed != level);

        self.crossed.len() != before
    }

    fn inhibit(&mut self, sender: &str, application: &str, reason: &str) -> u32 {
        let cookie = self.next_cookie;

        // wraps past u32::MAX without ever handing out 0
        self.next_cookie = self.next_cookie.checked_add(1).unwrap_or(1);

        self.inhibitors.push(Inhibitor {
            cookie,
            sender: String::from(sender),
            application: String::from(application),
            reason: String::from(reason),
        });

        cookie
    }

    fn uninhibit(&mut self, cookie: u32) -> bool {
        let before = self.inhibitors.len();

        self.inhibitors
            .retain(|inhibitor| inhibitor.cookie != cookie);

        self.inhibitors.len() != before
    }

    fn release_sender(&mut self, sender: &str) -> bool {
        let before = self.inhibitors.len();

        self.inhibitors
            .retain(|inhibitor| inhibitor.sender != sender);

        self.inhibitors.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle() -> Idle {
        Idle {
            levels: DEFAULT_LEVELS.to_vec(),
            crossed: Vec::new(),
            inhibitors: Vec::new(),
            next_cookie: 1,
            running: false,
        }
    }

    #[test]
    fn cookies_never_are_zero() {
        let mut idle = idle();

        idle.next_cookie = u32::MAX;

        assert_eq!(idle.inhibit(":1.1", "mpv", "video"), u32::MAX);
        assert_eq!(idle.inhibit(":1.1", "mpv", "video"), 1);
    }

    #[test]
    fn uninhibit_of_an_unknown_cookie_changes_nothing() {
        let mut idle = idle();

        let cookie = idle.inhibit(":1.1", "firefox", "video");

        assert!(!idle.uninhibit(cookie + 1));
        assert!(idle.inhibited());

        assert!(idle.uninhibit(cookie));
        assert!(!idle.inhibited());
    }

    #[test]
    fn a_program_that_left_is_let_go() {
        let mut idle = idle();

        idle.inhibit(":1.1", "firefox", "video");
        idle.inhibit(":1.1", "firefox", "audio");
        idle.inhibit(":1.2", "mpv", "video");

        assert!(idle.release_sender(":1.1"));

        let left: Vec<_> = idle.inhibitors().collect();

        assert_eq!(left, [("mpv", "video")]);
    }

    #[test]
    fn crossing_twice_changes_once() {
        let mut idle = idle();

        let level = Duration::from_secs(120);

        assert!(idle.cross(level));
        assert!(!idle.cross(level));
        assert!(idle.crossed(level));

        assert!(idle.uncross(level));
        assert!(!idle.uncross(level));
        assert!(!idle.is_idle());
    }
}
