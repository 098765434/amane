#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo, missing_docs)]

//! Caps Lock and Num Lock. The niri fork reports them on its event stream (`lock-keys`), so
//! nothing polls; elsewhere the keyboard LEDs in `/sys/class/leds` are read every 250 ms while
//! something watches.

use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::Service;
use crate::compositor::{self, niri::client};
use crate::services::IDLE_STOP;

const LEDS: &str = "/sys/class/leds";

/// Whether Caps Lock and Num Lock are on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LockKeys {
    caps_lock: bool,
    num_lock: bool,
}

impl Service for LockKeys {
    fn new() -> Self {
        // the stream replays the state first, so niri needs no first read
        if client::features().has("lock-keys") {
            Self::default()
        } else {
            read_leds(Path::new(LEDS))
        }
    }

    fn interval() -> Duration {
        Duration::from_millis(250)
    }

    fn idle_stop() -> Option<Duration> {
        Some(IDLE_STOP)
    }

    fn update(&mut self) -> bool {
        let fresh = read_leds(Path::new(LEDS));
        let changed = fresh != *self;
        *self = fresh;
        changed
    }

    fn listen() {
        if !client::features().has("lock-keys") {
            Self::poll();
            return;
        }

        compositor::listen_lock_keys(|caps_lock, num_lock| {
            let fresh = Self {
                caps_lock,
                num_lock,
            };
            let mut keys = Self::write();
            if *keys == fresh {
                keys.quiet();
                return;
            }
            *keys = fresh;
        });
    }
}

impl LockKeys {
    /// Whether Caps Lock is on.
    pub fn caps_lock(&self) -> bool {
        self.caps_lock
    }

    /// Whether Num Lock is on.
    pub fn num_lock(&self) -> bool {
        self.num_lock
    }

    // the default polling loop, for a compositor that doesn't report them
    fn poll() {
        loop {
            std::thread::sleep(Self::interval());
            if let Some(idle) = Self::idle_stop() {
                crate::services::store::wait_while_unread::<Self>(idle);
            }
            let mut keys = Self::write();
            if !keys.update() {
                keys.quiet();
            }
        }
    }
}

// a lock key is on when any keyboard's LED for it is
fn read_leds(leds: &Path) -> LockKeys {
    let mut keys = LockKeys::default();
    let Ok(entries) = fs::read_dir(leds) else {
        return keys;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let on = || {
            fs::read_to_string(entry.path().join("brightness"))
                .is_ok_and(|value| value.trim() != "0")
        };
        if name.ends_with("::capslock") {
            keys.caps_lock |= on();
        } else if name.ends_with("::numlock") {
            keys.num_lock |= on();
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_leds_of_every_keyboard() {
        let leds = std::env::temp_dir().join(format!("amane-leds-{}", std::process::id()));
        let _ = fs::remove_dir_all(&leds);
        for (name, value) in [
            ("input3::capslock", "0"),
            ("input9::capslock", "1"),
            ("input3::numlock", "0"),
            ("input3::scrolllock", "1"),
            ("asus::kbd_backlight", "2"),
        ] {
            fs::create_dir_all(leds.join(name)).expect("folder");
            fs::write(leds.join(name).join("brightness"), format!("{value}\n")).expect("file");
        }

        let keys = read_leds(&leds);
        assert!(keys.caps_lock() && !keys.num_lock());
        assert_eq!(read_leds(&leds.join("missing")), LockKeys::default());

        let _ = fs::remove_dir_all(&leds);
    }
}
