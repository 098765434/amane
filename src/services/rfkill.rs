#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo)]

//! The kernel's radio switches (rfkill): which radios there are and whether software or a
//! hardware switch holds each off. Read from `/dev/rfkill`, which sends every switch when it is
//! opened and then each change, so nothing polls. The seat's user may read and write it; where
//! it can't be opened, `/sys/class/rfkill` is read every second while something watches.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write as _};
use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::services::IDLE_STOP;
use crate::services::store;
use crate::{ErrorKind, Service, ServiceError};

const DEVICE: &str = "/dev/rfkill";
const SYSFS: &str = "/sys/class/rfkill";

// struct rfkill_event: idx (u32), type, op, soft, hard; newer kernels add a byte after, which a
// read of 8 bytes leaves out
const EVENT_SIZE: usize = 8;

const OP_ADD: u8 = 0;
const OP_DEL: u8 = 1;
const OP_CHANGE: u8 = 2;
const OP_CHANGE_ALL: u8 = 3;

/// One radio switch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Radio {
    /// The kernel's index, as `/sys/class/rfkill/rfkill<index>`.
    pub index: u32,
    /// `wlan`, `bluetooth`, `wwan`, `nfc`, `gps`, `fm`, `uwb` or `wimax`, as sysfs names them.
    pub kind: &'static str,
    /// Held off by software, which [`Rfkill::set_blocked`] can lift.
    pub soft: bool,
    /// Held off by a hardware switch, which only the switch can lift.
    pub hard: bool,
}

/// Every radio switch the kernel has.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Rfkill {
    radios: Vec<Radio>,
}

impl Service for Rfkill {
    fn new() -> Self {
        Self {
            radios: from_sysfs(Path::new(SYSFS)),
        }
    }

    fn interval() -> Duration {
        Duration::from_secs(1)
    }

    fn idle_stop() -> Option<Duration> {
        Some(IDLE_STOP)
    }

    fn update(&mut self) -> bool {
        let radios = from_sysfs(Path::new(SYSFS));
        let changed = radios != self.radios;
        self.radios = radios;
        changed
    }

    fn listen() {
        let mut device = match File::open(DEVICE) {
            Ok(device) => device,
            Err(err) => {
                eprintln!("amane: {DEVICE}: {err}; reading {SYSFS} instead");
                return poll();
            }
        };

        let mut radios = Vec::new();
        let mut event = [0u8; EVENT_SIZE];
        loop {
            if let Err(err) = device.read_exact(&mut event) {
                eprintln!("amane: {DEVICE} stopped: {err}; reading {SYSFS} instead");
                return poll();
            }
            apply(&mut radios, event);

            let mut service = Self::write();
            if service.radios == radios {
                service.quiet();
                continue;
            }
            service.radios.clone_from(&radios);
        }
    }
}

impl Rfkill {
    /// Every radio, in the kernel's order.
    pub fn radios(&self) -> &[Radio] {
        &self.radios
    }

    /// Whether there's any radio to switch.
    pub fn present(&self) -> bool {
        !self.radios.is_empty()
    }

    /// Whether there's a radio of `kind`.
    pub fn has(&self, kind: &str) -> bool {
        self.of(kind).next().is_some()
    }

    /// Whether every radio of `kind` is held off, by software or a switch. False when there's
    /// none.
    pub fn blocked(&self, kind: &str) -> bool {
        self.has(kind) && self.of(kind).all(|radio| radio.soft || radio.hard)
    }

    /// Whether every radio of `kind` is held off by a hardware switch.
    pub fn hard_blocked(&self, kind: &str) -> bool {
        self.has(kind) && self.of(kind).all(|radio| radio.hard)
    }

    /// Airplane mode: every radio held off.
    pub fn airplane(&self) -> bool {
        self.present() && self.radios.iter().all(|radio| radio.soft || radio.hard)
    }

    /// Blocks or unblocks every radio of `kind` in software, or every radio for `None`, as
    /// `rfkill block|unblock <kind>` does. The change comes back through the service.
    pub fn set_blocked(kind: Option<&str>, blocked: bool) -> Result<(), ServiceError> {
        let kind = match kind {
            Some(kind) => type_of(kind).ok_or_else(|| {
                ServiceError::new(ErrorKind::NotSupported, format!("no radio kind {kind}"))
            })?,
            None => 0,
        };
        let event = [0, 0, 0, 0, kind, OP_CHANGE_ALL, u8::from(blocked), 0];
        OpenOptions::new()
            .write(true)
            .open(DEVICE)?
            .write_all(&event)?;
        Ok(())
    }

    fn of<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a Radio> + 'a {
        self.radios.iter().filter(move |radio| radio.kind == kind)
    }
}

fn poll() {
    loop {
        thread::sleep(Rfkill::interval());
        if let Some(idle) = Rfkill::idle_stop() {
            store::wait_while_unread::<Rfkill>(idle);
        }
        let mut service = Rfkill::write();
        if !service.update() {
            service.quiet();
        }
    }
}

const KINDS: [&str; 9] = [
    "all",
    "wlan",
    "bluetooth",
    "uwb",
    "wimax",
    "wwan",
    "gps",
    "fm",
    "nfc",
];

fn kind_of(kind: u8) -> &'static str {
    KINDS.get(usize::from(kind)).copied().unwrap_or("unknown")
}

fn type_of(kind: &str) -> Option<u8> {
    KINDS
        .iter()
        .position(|name| *name == kind)
        .and_then(|index| u8::try_from(index).ok())
}

/// One event from `/dev/rfkill` applied to the list.
fn apply(radios: &mut Vec<Radio>, event: [u8; EVENT_SIZE]) {
    let index = u32::from_ne_bytes([event[0], event[1], event[2], event[3]]);
    let radio = Radio {
        index,
        kind: kind_of(event[4]),
        soft: event[6] != 0,
        hard: event[7] != 0,
    };
    match event[5] {
        OP_ADD | OP_CHANGE => match radios.iter_mut().find(|known| known.index == index) {
            Some(known) => *known = radio,
            None => {
                radios.push(radio);
                radios.sort_by_key(|radio| radio.index);
            }
        },
        OP_DEL => radios.retain(|known| known.index != index),
        _ => {}
    }
}

/// Every switch under `root`, as sysfs lists them.
fn from_sysfs(root: &Path) -> Vec<Radio> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let read = |dir: &Path, name: &str| {
        fs::read_to_string(dir.join(name))
            .map(|text| text.trim().to_owned())
            .unwrap_or_default()
    };

    let mut radios: Vec<Radio> = entries
        .flatten()
        .filter_map(|entry| {
            let dir = entry.path();
            let index = entry
                .file_name()
                .to_str()?
                .strip_prefix("rfkill")?
                .parse()
                .ok()?;
            let kind = read(&dir, "type");
            Some(Radio {
                index,
                kind: KINDS
                    .iter()
                    .find(|name| **name == kind)
                    .copied()
                    .unwrap_or("unknown"),
                soft: read(&dir, "soft") == "1",
                hard: read(&dir, "hard") == "1",
            })
        })
        .collect();
    radios.sort_by_key(|radio| radio.index);
    radios
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(index: u32, kind: u8, op: u8, soft: u8, hard: u8) -> [u8; EVENT_SIZE] {
        let i = index.to_ne_bytes();
        [i[0], i[1], i[2], i[3], kind, op, soft, hard]
    }

    #[test]
    fn follows_the_device_events() {
        let mut radios = Vec::new();
        // what an open sends: every switch
        apply(&mut radios, event(1, 2, OP_ADD, 0, 0));
        apply(&mut radios, event(0, 1, OP_ADD, 0, 0));
        let mut service = Rfkill {
            radios: radios.clone(),
        };
        assert_eq!(service.radios()[0].kind, "wlan");
        assert!(!service.airplane() && !service.blocked("bluetooth"));

        apply(&mut radios, event(1, 2, OP_CHANGE, 1, 0));
        service.radios.clone_from(&radios);
        assert!(service.blocked("bluetooth") && !service.hard_blocked("bluetooth"));
        assert!(!service.airplane());

        apply(&mut radios, event(0, 1, OP_CHANGE, 0, 1));
        service.radios.clone_from(&radios);
        assert!(service.hard_blocked("wlan") && service.airplane());

        apply(&mut radios, event(1, 2, OP_DEL, 0, 0));
        service.radios.clone_from(&radios);
        assert!(!service.has("bluetooth"));
        assert!(!service.blocked("nfc"), "no radio is never blocked");
    }

    #[test]
    fn reads_sysfs_when_the_device_cant_be_opened() {
        let root = std::env::temp_dir().join(format!("amane-rfkill-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (name, kind, soft, hard) in [
            ("rfkill3", "bluetooth", "1", "0"),
            ("rfkill0", "wlan", "0", "0"),
        ] {
            let dir = root.join(name);
            fs::create_dir_all(&dir).expect("folder");
            fs::write(dir.join("type"), format!("{kind}\n")).expect("type");
            fs::write(dir.join("soft"), soft).expect("soft");
            fs::write(dir.join("hard"), hard).expect("hard");
        }

        let radios = from_sysfs(&root);
        assert_eq!(radios.len(), 2);
        assert_eq!((radios[0].index, radios[0].kind), (0, "wlan"));
        assert!(radios[1].soft && !radios[1].hard);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn names_the_kinds_as_sysfs_does() {
        assert_eq!(type_of("bluetooth"), Some(2));
        assert_eq!(kind_of(1), "wlan");
        assert_eq!(kind_of(200), "unknown");
        assert_eq!(type_of("tv"), None);
    }
}
