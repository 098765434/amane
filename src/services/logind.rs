#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo)]

//! The login session as logind sees it: whether it is the active one on its seat, its locked
//! and idle hints, requests to lock or unlock it (`loginctl lock-session`), and the computer
//! going to sleep. All from logind's signals on the system bus, so nothing polls. Also takes
//! inhibitor locks, which keep the computer from sleeping or idling while they're held.

use std::os::fd::OwnedFd;

use zbus::zvariant;

use crate::dbus::Bus;
use crate::{Argument, Service, Value};

const LOGIND: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";
const SESSION: &str = "org.freedesktop.login1.Session";
// the caller's session, or the user's graphical one for a program outside any session, like a
// systemd user service
const AUTO: &str = "/org/freedesktop/login1/session/auto";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

/// The session amane runs in, as logind sees it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Logind {
    path: Option<String>,
    active: bool,
    locked_hint: bool,
    idle_hint: bool,
    preparing_for_sleep: bool,
    lock_requests: u64,
    unlock_requests: u64,
    wakeups: u64,
}

impl Service for Logind {
    fn new() -> Self {
        let bus = Bus::system();
        let Some(path) = session_path(bus) else {
            return Self::default();
        };
        let mut logind = Self {
            path: Some(path),
            ..Self::default()
        };
        logind.read_properties(bus);
        logind
    }

    fn listen() {
        let bus = Bus::system();
        let Some(path) = Self::read().path.clone() else {
            return;
        };

        for signal in bus.signals_from(LOGIND) {
            let mut logind = Self::write();
            let before = logind.clone();

            match (signal.interface(), signal.name()) {
                (SESSION, "Lock") if signal.path() == path => logind.lock_requests += 1,
                (SESSION, "Unlock") if signal.path() == path => logind.unlock_requests += 1,
                (MANAGER, "PrepareForSleep") => {
                    let sleeping = signal.arguments().first().is_some_and(Value::bool);
                    if logind.preparing_for_sleep && !sleeping {
                        logind.wakeups += 1;
                    }
                    logind.preparing_for_sleep = sleeping;
                }
                (PROPERTIES, "PropertiesChanged") if signal.path() == path => {
                    logind.apply_changes(signal.arguments());
                }
                _ => {}
            }

            if *logind == before {
                logind.quiet();
            }
        }
    }
}

impl Logind {
    /// Whether logind knows the session; false outside a login session or without logind.
    pub fn present(&self) -> bool {
        self.path.is_some()
    }

    /// Whether the session is the one in front on its seat, not switched away from.
    pub fn active(&self) -> bool {
        self.active
    }

    /// Whether the screen locker said it locked the session.
    pub fn locked_hint(&self) -> bool {
        self.locked_hint
    }

    /// Whether the session said it is idle.
    pub fn idle_hint(&self) -> bool {
        self.idle_hint
    }

    /// Whether the computer is about to sleep: between logind's `PrepareForSleep(true)` and the
    /// wake-up.
    pub fn preparing_for_sleep(&self) -> bool {
        self.preparing_for_sleep
    }

    /// How many times the session was asked to lock (`loginctl lock-session`). A subscriber
    /// notices a new request by the count going up.
    pub fn lock_requests(&self) -> u64 {
        self.lock_requests
    }

    /// How many times the session was asked to unlock.
    pub fn unlock_requests(&self) -> u64 {
        self.unlock_requests
    }

    /// How many times the computer woke from sleep since amane started.
    pub fn wakeups(&self) -> u64 {
        self.wakeups
    }

    /// Takes an inhibitor lock, held until the returned descriptor is dropped.
    ///
    /// `what` is a colon-separated list of `sleep`, `shutdown`, `idle`,
    /// `handle-power-key`, `handle-lid-switch` and the like; `mode` is `block` or `delay`. `who`
    /// and `why` show in `systemd-inhibit --list`.
    pub fn inhibit(what: &str, who: &str, why: &str, mode: &str) -> zbus::Result<OwnedFd> {
        let Some(connection) = Bus::system().connection else {
            return Err(zbus::Error::Failure(String::from("no system bus")));
        };
        let reply = connection.call_method(
            Some(LOGIND),
            MANAGER_PATH,
            Some(MANAGER),
            "Inhibit",
            &(what, who, why, mode),
        )?;
        let fd: zvariant::OwnedFd = reply.body().deserialize()?;
        Ok(fd.into())
    }

    /// Sets the session's locked hint, for a screen locker.
    pub fn set_locked_hint(locked: bool) {
        let Some(path) = Self::read().path.clone() else {
            return;
        };
        crate::services::worker::run(move || {
            Bus::system().call(
                LOGIND,
                &path,
                SESSION,
                "SetLockedHint",
                &[Argument::from(locked)],
            );
        });
    }

    fn read_properties(&mut self, bus: Bus) {
        let Some(path) = &self.path else {
            return;
        };
        let all = bus.call(
            LOGIND,
            path,
            PROPERTIES,
            "GetAll",
            &[Argument::from(SESSION)],
        );
        self.active = all.get("Active").bool();
        self.locked_hint = all.get("LockedHint").bool();
        self.idle_hint = all.get("IdleHint").bool();
        self.preparing_for_sleep = bus
            .property(LOGIND, MANAGER_PATH, MANAGER, "PreparingForSleep")
            .bool();
    }

    // PropertiesChanged(interface, changed, invalidated)
    fn apply_changes(&mut self, arguments: &[Value]) {
        if arguments.first().map(Value::text) != Some(SESSION) {
            return;
        }
        let Some(changed) = arguments.get(1) else {
            return;
        };
        for (key, field) in [
            ("Active", &mut self.active),
            ("LockedHint", &mut self.locked_hint),
            ("IdleHint", &mut self.idle_hint),
        ] {
            if let Value::Bool(value) = changed.get(key) {
                *field = *value;
            }
        }
    }
}

// the real object path of the session `auto` names, since signals come from that one
fn session_path(bus: Bus) -> Option<String> {
    let id = bus.property(LOGIND, AUTO, SESSION, "Id");
    let id = id.text();
    if id.is_empty() {
        return None;
    }
    let path = bus.call(
        LOGIND,
        MANAGER_PATH,
        MANAGER,
        "GetSession",
        &[Argument::from(id)],
    );
    let path = path.text();
    (!path.is_empty()).then(|| path.to_owned())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn takes_the_session_properties_that_changed() {
        let mut logind = Logind {
            path: Some(String::from("/org/freedesktop/login1/session/_32")),
            active: true,
            ..Logind::default()
        };
        let changed = Value::Map(BTreeMap::from([
            (String::from("LockedHint"), Value::Bool(true)),
            (String::from("Active"), Value::Bool(false)),
        ]));

        logind.apply_changes(&[Value::Text(String::from("org.other")), changed.clone()]);
        assert!(
            logind.active() && !logind.locked_hint(),
            "another interface"
        );

        logind.apply_changes(&[Value::Text(String::from(SESSION)), changed]);
        assert!(!logind.active() && logind.locked_hint() && !logind.idle_hint());
    }
}
