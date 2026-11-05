/*
 * phones through kdeconnect: kdeconnectd runs on its own and amane draws
 * everything over its session-bus api, so no kde window ever shows
 *
 * every device and plugin is read again on any of the daemon's signals,
 * a handful of calls for the one or two phones anyone has
 */

mod actions;
mod config;
mod contacts;
mod device;
mod input;
mod sms;

use std::collections::BTreeMap;
use std::sync::{Mutex, Once, PoisonError};
use std::thread;
use std::time::Duration;

use crate::{Argument, Bus, Service};

pub use device::{PairState, PhoneBattery, PhoneDevice, PhoneNotification};
pub use input::{PhoneClick, PhoneKey, PhoneModifiers};
pub use sms::{PAGE as SMS_PAGE, PhoneMessages, SmsKind, SmsMessage, SmsThread};

pub(crate) const KDECONNECT: &str = "org.kde.kdeconnect";
pub(crate) const DAEMON_PATH: &str = "/modules/kdeconnect";
pub(crate) const DAEMON: &str = "org.kde.kdeconnect.daemon";
pub(crate) const DEVICE: &str = "org.kde.kdeconnect.device";
pub(crate) const BATTERY: &str = "org.kde.kdeconnect.device.battery";
pub(crate) const CLIPBOARD: &str = "org.kde.kdeconnect.device.clipboard";
pub(crate) const REMOTE_KEYBOARD: &str = "org.kde.kdeconnect.device.remotekeyboard";
pub(crate) const NOTIFICATIONS: &str = "org.kde.kdeconnect.device.notifications";
pub(crate) const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

// the daemon is asked for everything again this often, in case a signal was missed
const RESYNC_EVERY: Duration = Duration::from_secs(30);

// the daemon's unique name like :1.7, so only its signals cause a read
static OWNER: Mutex<String> = Mutex::new(String::new());

// one read at a time, so two reads never publish out of order
static READING: Mutex<()> = Mutex::new(());

#[derive(Default)]
pub struct KdeConnect {
    // kdeconnectd is on the bus; nothing else is filled in when it isn't
    running: bool,

    // paired devices first, then the ones that could pair, each by name
    devices: Vec<PhoneDevice>,

    // why each device's last action failed, by its id, for a little while
    failures: BTreeMap<String, String>,
}

impl Service for KdeConnect {
    fn new() -> Self {
        let (running, devices) = fetch();

        Self {
            running,
            devices,
            failures: BTreeMap::new(),
        }
    }

    fn listen() {
        let bus = Bus::session();

        static WATCH: Once = Once::new();

        WATCH.call_once(|| {
            // the daemon starting, quitting or restarting
            thread::spawn(move || {
                for _ in bus.owner_changes(KDECONNECT) {
                    resync();
                }
            });

            thread::spawn(|| {
                loop {
                    thread::sleep(RESYNC_EVERY);

                    // nothing to ask while it isn't running, and asking would start it
                    if KdeConnect::read().running {
                        resync();
                    }
                }
            });
        });

        // watched before reading, so nothing that happens in between is missed
        let signals = bus.signals_from(KDECONNECT);

        resync();

        for signal in signals {
            if signal.sender()
                != OWNER
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .as_str()
            {
                continue;
            }

            // the phone's media and messages change nothing listed
            if matters(signal.interface(), signal.name()) {
                resync();
            }
        }
    }
}

// the signals that change something a device's row or page shows
fn matters(interface: &str, name: &str) -> bool {
    match interface {
        DAEMON => !matches!(name, "announcedNameChanged" | "customDevicesChanged"),
        DEVICE | BATTERY | CLIPBOARD | REMOTE_KEYBOARD | NOTIFICATIONS => true,
        PROPERTIES => name == "PropertiesChanged",
        _ => false,
    }
}

impl KdeConnect {
    pub fn running(&self) -> bool {
        self.running
    }

    pub fn devices(&self) -> &[PhoneDevice] {
        &self.devices
    }

    pub fn device(&self, id: &str) -> Option<&PhoneDevice> {
        self.devices.iter().find(|device| device.id() == id)
    }

    pub fn paired(&self) -> impl Iterator<Item = &PhoneDevice> {
        self.devices.iter().filter(|device| device.paired())
    }

    // found on the network and not paired, so they can be asked
    pub fn available(&self) -> impl Iterator<Item = &PhoneDevice> {
        self.devices
            .iter()
            .filter(|device| !device.paired() && device.reachable())
    }

    // a phone asking to pair, waiting for an answer here
    pub fn pairing_request(&self) -> Option<&PhoneDevice> {
        self.devices
            .iter()
            .find(|device| device.pair_state() == PairState::RequestedByPeer)
    }

    /*
     * the phone the bar shows: the first connected one with a battery,
     * so a tablet left at home doesn't take the place of the phone at hand
     */
    pub fn primary(&self) -> Option<&PhoneDevice> {
        self.devices
            .iter()
            .find(|device| device.connected() && device.battery().is_some())
    }

    pub fn failure(&self, device: &str) -> Option<&str> {
        self.failures.get(device).map(String::as_str)
    }
}

// the daemon's unique name, empty while it isn't running
pub(crate) fn owner() -> String {
    OWNER.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

pub(crate) fn device_path(id: &str) -> String {
    format!("{DAEMON_PATH}/devices/{id}")
}

// everything the daemon has, nothing when it isn't running; never starts it
fn fetch() -> (bool, Vec<PhoneDevice>) {
    let bus = Bus::session();

    let owner = bus.call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetNameOwner",
        &[Argument::from(KDECONNECT)],
    );

    let owner = owner.text().to_string();

    let running = !owner.is_empty();

    *OWNER.lock().unwrap_or_else(PoisonError::into_inner) = owner;

    if !running {
        return (false, Vec::new());
    }

    // a raw call: the daemon's introspection lists methods twice, which proxies refuse
    let ids = bus.call(
        KDECONNECT,
        DAEMON_PATH,
        DAEMON,
        "devices",
        &[Argument::from(false), Argument::from(false)],
    );

    let mut devices: Vec<PhoneDevice> = ids
        .list()
        .iter()
        .filter_map(|id| PhoneDevice::fetch(&bus, id.text()))
        .collect();

    devices.sort_by(|a, b| {
        b.paired()
            .cmp(&a.paired())
            .then_with(|| a.name().to_lowercase().cmp(&b.name().to_lowercase()))
    });

    (true, devices)
}

// reads everything again and shows it, writing only when that changed
pub(crate) fn resync() {
    let _reading = READING.lock().unwrap_or_else(PoisonError::into_inner);

    let (running, devices) = fetch();

    let kdeconnect = KdeConnect::read();

    if kdeconnect.running == running && kdeconnect.devices == devices {
        return;
    }

    drop(kdeconnect);

    let mut kdeconnect = KdeConnect::write();

    kdeconnect
        .failures
        .retain(|id, _| devices.iter().any(|device| device.id() == id));

    kdeconnect.running = running;
    kdeconnect.devices = devices;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_device_signals_cause_a_read() {
        assert!(matters(DAEMON, "deviceAdded"));
        assert!(matters(DEVICE, "reachableChanged"));
        assert!(matters(BATTERY, "refreshed"));
        assert!(!matters(DAEMON, "announcedNameChanged"));
        // a device lists its notifications, for Open on Phone
        assert!(matters(NOTIFICATIONS, "notificationPosted"));
        assert!(!matters(
            "org.kde.kdeconnect.device.conversations",
            "conversationUpdated"
        ));
    }
}
