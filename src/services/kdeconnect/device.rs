use std::net::IpAddr;

use crate::{Argument, Bus, Value};

use super::{
    BATTERY, DEVICE, KDECONNECT, NOTIFICATIONS, PROPERTIES, REMOTE_KEYBOARD, config, device_path,
};

// the plugins whose sub-objects the device's page offers
pub(crate) const BATTERY_PLUGIN: &str = "kdeconnect_battery";
pub(crate) const RING_PLUGIN: &str = "kdeconnect_findmyphone";
pub(crate) const SHARE_PLUGIN: &str = "kdeconnect_share";
pub(crate) const CLIPBOARD_PLUGIN: &str = "kdeconnect_clipboard";
pub(crate) const SMS_PLUGIN: &str = "kdeconnect_sms";
pub(crate) const REMOTE_CONTROL_PLUGIN: &str = "kdeconnect_remotecontrol";
pub(crate) const REMOTE_KEYBOARD_PLUGIN: &str = "kdeconnect_remotekeyboard";
pub(crate) const RUN_COMMAND_PLUGIN: &str = "kdeconnect_runcommand";
pub(crate) const NOTIFICATIONS_PLUGIN: &str = "kdeconnect_notifications";

const NOTIFICATION: &str = "org.kde.kdeconnect.device.notifications.notification";

// one of the phone's notifications, as KDE Connect mirrors it
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhoneNotification {
    // KDE Connect's number for it, under the device's notifications object
    pub id: String,

    // the app's name as the phone shows it, like "Signal"
    pub app: String,

    // the app's package, like "org.thoughtcrime.securesms", when the phone says
    pub package: Option<String>,

    pub title: String,
    pub text: String,
}

// where a device and this computer stand on pairing, as kdeconnect numbers it
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairState {
    NotPaired,

    // asked from here, waiting for the phone to accept
    Requested,

    // the phone asked, waiting for an answer here
    RequestedByPeer,

    Paired,
}

impl PairState {
    fn from_code(code: f64) -> Self {
        match code as i32 {
            1 => Self::Requested,
            2 => Self::RequestedByPeer,
            3 => Self::Paired,
            _ => Self::NotPaired,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhoneBattery {
    // 0 to 100
    pub percent: u8,

    pub charging: bool,
}

// a phone, tablet or computer kdeconnect has found or paired with
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhoneDevice {
    // kdeconnect's own id, which every action takes
    pub(crate) id: String,

    pub(crate) name: String,

    // "phone" (older daemons say "smartphone"), "tablet", "desktop", "laptop" or "tv"
    pub(crate) kind: String,

    pub(crate) pair: PairState,

    // on the same network right now; a paired phone stays listed when it isn't
    pub(crate) reachable: bool,

    // the code both screens show while pairing, empty otherwise
    pub(crate) verification_key: String,

    // the plugins loaded for it, only while it's paired and reachable
    pub(crate) plugins: Vec<String>,

    pub(crate) battery: Option<PhoneBattery>,

    // whether this computer's clipboard goes to the phone by itself, only while the plugin is loaded
    pub(crate) clipboard_auto_sync: Option<bool>,

    // the phone can run this computer's commands (lock, suspend, ...)
    pub(crate) run_commands: bool,

    // a text field is focused on the phone, so typing here lands in it
    pub(crate) keyboard_ready: bool,

    // where it is on the network, while it's reachable over the LAN
    pub(crate) addresses: Vec<IpAddr>,

    // the phone's notifications, only while the plugin is loaded
    pub(crate) notifications: Vec<PhoneNotification>,
}

impl PhoneDevice {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn pair_state(&self) -> PairState {
        self.pair
    }

    pub fn paired(&self) -> bool {
        self.pair == PairState::Paired
    }

    pub fn reachable(&self) -> bool {
        self.reachable
    }

    // paired and on the network, so its plugins can be used
    pub fn connected(&self) -> bool {
        self.paired() && self.reachable
    }

    pub fn verification_key(&self) -> &str {
        &self.verification_key
    }

    pub fn battery(&self) -> Option<PhoneBattery> {
        self.battery
    }

    pub fn has_plugin(&self, plugin: &str) -> bool {
        self.plugins.iter().any(|loaded| loaded == plugin)
    }

    pub fn can_ring(&self) -> bool {
        self.has_plugin(RING_PLUGIN)
    }

    pub fn can_share(&self) -> bool {
        self.has_plugin(SHARE_PLUGIN)
    }

    pub fn can_message(&self) -> bool {
        self.has_plugin(SMS_PLUGIN)
    }

    // both halves, the pointer and the keyboard, are there
    pub fn can_remote_input(&self) -> bool {
        self.has_plugin(REMOTE_CONTROL_PLUGIN) && self.has_plugin(REMOTE_KEYBOARD_PLUGIN)
    }

    pub fn can_run_commands(&self) -> bool {
        self.has_plugin(RUN_COMMAND_PLUGIN)
    }

    pub fn clipboard_auto_sync(&self) -> Option<bool> {
        self.clipboard_auto_sync
    }

    pub fn run_commands(&self) -> bool {
        self.run_commands
    }

    pub fn keyboard_ready(&self) -> bool {
        self.keyboard_ready
    }

    // its network addresses, IPv4 first; empty when it isn't on this network
    pub fn addresses(&self) -> &[IpAddr] {
        &self.addresses
    }

    pub fn notifications(&self) -> &[PhoneNotification] {
        &self.notifications
    }

    // one device as kdeconnect has it now; none when it's gone since it was listed
    pub(crate) fn fetch(bus: &Bus, id: &str) -> Option<Self> {
        let path = device_path(id);

        let props = get_all(bus, &path, DEVICE);

        if props == Value::Nothing {
            return None;
        }

        let device = Self::from_properties(id, &props);

        Some(if device.connected() {
            device.with_plugins(bus, &path)
        } else if matches!(
            device.pair,
            PairState::Requested | PairState::RequestedByPeer
        ) && device.verification_key.is_empty()
        {
            // older daemons only answer the key as a method
            let key = bus.call(KDECONNECT, &path, DEVICE, "verificationKey", &[]);

            Self {
                verification_key: key.text().to_string(),
                ..device
            }
        } else {
            device
        })
    }

    // what the device object's own properties say, before asking its plugins
    pub(crate) fn from_properties(id: &str, props: &Value) -> Self {
        let paired = props.get("isPaired").bool();

        let pair = match props.get("pairState") {
            Value::Number(code) => PairState::from_code(*code),
            _ if paired => PairState::Paired,
            _ if props.get("isPairRequestedByPeer").bool() => PairState::RequestedByPeer,
            _ if props.get("isPairRequested").bool() => PairState::Requested,
            _ => PairState::NotPaired,
        };

        let name = match props.get("name").text() {
            "" => id,
            name => name,
        };

        Self {
            id: id.to_string(),
            name: name.to_string(),
            kind: props.get("type").text().to_string(),
            pair,
            reachable: props.get("isReachable").bool(),
            verification_key: props.get("verificationKey").text().to_string(),
            plugins: Vec::new(),
            battery: None,
            clipboard_auto_sync: None,
            run_commands: pair == PairState::Paired && config::run_commands_enabled(id),
            keyboard_ready: false,
            addresses: addresses(props.get("reachableAddresses")),
            notifications: Vec::new(),
        }
    }

    // the plugin state of a connected device, each from its own sub-object
    fn with_plugins(mut self, bus: &Bus, path: &str) -> Self {
        let plugins = bus.call(KDECONNECT, path, DEVICE, "loadedPlugins", &[]);

        self.plugins = plugins
            .list()
            .iter()
            .map(|plugin| plugin.text().to_string())
            .collect();

        self.plugins.sort();

        if self.has_plugin(BATTERY_PLUGIN) {
            let battery = get_all(bus, &format!("{path}/battery"), BATTERY);

            self.battery = battery_from(&battery);
        }

        if self.has_plugin(CLIPBOARD_PLUGIN) {
            // the file is what was asked for; the plugin's own property lags until it reloads
            self.clipboard_auto_sync = Some(config::clipboard_auto_sync(&self.id));
        }

        if self.has_plugin(REMOTE_KEYBOARD_PLUGIN) {
            let keyboard = get_all(bus, &format!("{path}/remotekeyboard"), REMOTE_KEYBOARD);

            self.keyboard_ready = keyboard.get("remoteState").bool();
        }

        if self.has_plugin(NOTIFICATIONS_PLUGIN) {
            self.notifications = notifications(bus, path);
        }

        self
    }
}

// the addresses KDE Connect reaches the phone at, IPv4 first
fn addresses(value: &Value) -> Vec<IpAddr> {
    let mut addresses: Vec<IpAddr> = value
        .list()
        .iter()
        .filter_map(|address| address.text().parse().ok())
        .collect();

    addresses.sort_by_key(IpAddr::is_ipv6);

    addresses
}

fn notifications(bus: &Bus, path: &str) -> Vec<PhoneNotification> {
    let path = format!("{path}/notifications");

    let ids = bus.call(KDECONNECT, &path, NOTIFICATIONS, "activeNotifications", &[]);

    ids.list()
        .iter()
        .map(|id| {
            let id = id.text();
            let props = get_all(bus, &format!("{path}/{id}"), NOTIFICATION);

            notification(id, &props)
        })
        .collect()
}

pub(crate) fn notification(id: &str, props: &Value) -> PhoneNotification {
    PhoneNotification {
        id: id.to_string(),
        app: props.get("appName").text().to_string(),
        package: package(props.get("internalId").text()),
        title: props.get("title").text().to_string(),
        text: props.get("text").text().to_string(),
    }
}

/*
 * Android's notification key, which KDE Connect passes on as the internal
 * id: "0|com.example.app|42|tag|10123", the user, the package, then the
 * app's own numbers
 */
fn package(internal_id: &str) -> Option<String> {
    let package = internal_id.split('|').nth(1)?;

    package.contains('.').then(|| package.to_string())
}

// the charge is -1 until the phone first sends it
pub(crate) fn battery_from(props: &Value) -> Option<PhoneBattery> {
    let charge = match props.get("charge") {
        Value::Number(charge) => *charge,
        _ => return None,
    };

    if charge < 0.0 {
        return None;
    }

    Some(PhoneBattery {
        percent: charge.clamp(0.0, 100.0) as u8,
        charging: props.get("isCharging").bool(),
    })
}

fn get_all(bus: &Bus, path: &str, interface: &str) -> Value {
    bus.call(
        KDECONNECT,
        path,
        PROPERTIES,
        "GetAll",
        &[Argument::from(interface)],
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn props(entries: &[(&str, Value)]) -> Value {
        Value::Map(
            entries
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    #[test]
    fn reads_the_phones_addresses_ipv4_first() {
        let device = PhoneDevice::from_properties(
            "abc",
            &props(&[(
                "reachableAddresses",
                Value::List(vec![
                    Value::Text("fe80::1".into()),
                    Value::Text("192.168.1.101".into()),
                    Value::Text("not an address".into()),
                ]),
            )]),
        );

        assert_eq!(
            device.addresses(),
            [
                "192.168.1.101".parse::<IpAddr>().expect("an address"),
                "fe80::1".parse().expect("an address")
            ]
        );
    }

    #[test]
    fn a_notification_names_its_app_package() {
        let notification = notification(
            "17",
            &props(&[
                ("appName", Value::Text("Nextcloud".into())),
                (
                    "internalId",
                    Value::Text("0|com.nextcloud.client|3044|null|10448".into()),
                ),
                ("title", Value::Text("Upload failed".into())),
            ]),
        );

        assert_eq!(notification.app, "Nextcloud");
        assert_eq!(
            notification.package.as_deref(),
            Some("com.nextcloud.client")
        );

        // KDE Connect's own notifications, like a missed call, have no package
        assert_eq!(package("missedCall"), None);
    }

    #[test]
    fn reads_a_paired_phone() {
        let device = PhoneDevice::from_properties(
            "abc",
            &props(&[
                ("name", Value::Text("Pixel".into())),
                ("type", Value::Text("smartphone".into())),
                ("isPaired", Value::Bool(true)),
                ("isReachable", Value::Bool(true)),
                ("pairState", Value::Number(3.0)),
            ]),
        );

        assert_eq!(device.name(), "Pixel");
        assert_eq!(device.kind(), "smartphone");
        assert!(device.paired());
        assert!(device.connected());
    }

    #[test]
    fn a_request_from_the_phone_waits_for_an_answer() {
        let device = PhoneDevice::from_properties(
            "abc",
            &props(&[
                ("isPaired", Value::Bool(false)),
                ("isReachable", Value::Bool(true)),
                ("pairState", Value::Number(2.0)),
                ("verificationKey", Value::Text("1A2B3C".into())),
            ]),
        );

        assert_eq!(device.pair_state(), PairState::RequestedByPeer);
        assert_eq!(device.verification_key(), "1A2B3C");
        assert!(!device.connected());
    }

    #[test]
    fn without_pair_state_the_flags_decide() {
        let device = PhoneDevice::from_properties(
            "abc",
            &props(&[("isPairRequestedByPeer", Value::Bool(true))]),
        );

        assert_eq!(device.pair_state(), PairState::RequestedByPeer);

        // an unnamed device shows its id
        assert_eq!(device.name(), "abc");
    }

    #[test]
    fn no_charge_yet_is_no_battery() {
        assert_eq!(
            battery_from(&props(&[("charge", Value::Number(-1.0))])),
            None
        );

        assert_eq!(
            battery_from(&props(&[
                ("charge", Value::Number(54.0)),
                ("isCharging", Value::Bool(true)),
            ])),
            Some(PhoneBattery {
                percent: 54,
                charging: true
            })
        );
    }
}
