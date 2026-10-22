use std::time::Duration;

use crate::{Argument, Bus, Value};

use super::{Battery, BatteryState, PowerDevice, WarningLevel};

pub(super) const UPOWER: &str = "org.freedesktop.UPower";
pub(super) const ROOT: &str = "/org/freedesktop/UPower";
pub(super) const DEVICE: &str = "org.freedesktop.UPower.Device";

const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

// none when upower isn't running, and then the battery is read from the kernel
pub(super) fn fetch() -> Option<Battery> {
    let bus = Bus::system();

    // the display device is upower's own pick of what a bar shows, all batteries combined
    let display = bus.call(UPOWER, ROOT, UPOWER, "GetDisplayDevice", &[]);

    let Value::Text(display) = display else {
        return None;
    };

    let device = properties(&display);

    let state = BatteryState::from_upower(device.get("State").number() as u32);
    let on_ac = !bus.property(UPOWER, ROOT, UPOWER, "OnBattery").bool();

    let mut battery = Battery {
        upower: true,
        present: device.get("IsPresent").bool(),

        percent: device.get("Percentage").number().round() as u8,

        charging: state == BatteryState::Charging,
        full: state == BatteryState::Full || (on_ac && state == BatteryState::PendingCharge),

        state,
        icon: String::from(device.get("IconName").text()),
        time_to_empty: seconds(device.get("TimeToEmpty")),
        time_to_full: seconds(device.get("TimeToFull")),
        warning: WarningLevel::from_upower(device.get("WarningLevel").number() as u32),
        on_ac,

        ..Battery::default()
    };

    let paths = bus.call(UPOWER, ROOT, UPOWER, "EnumerateDevices", &[]);

    for path in paths.list() {
        let device = properties(path.text());

        // upower's own line between the machine's batteries and everything else
        if device.get("PowerSupply").bool() || !device.get("IsPresent").bool() {
            continue;
        }

        battery
            .peripherals
            .push(PowerDevice::from_properties(&device));
    }

    battery
        .peripherals
        .sort_by_key(|device| device.model.to_lowercase());

    Some(battery)
}

// every property of one device in a single call
fn properties(path: &str) -> Value {
    let interface = [Argument::from(DEVICE)];

    Bus::system().call(UPOWER, path, PROPERTIES, "GetAll", &interface)
}

// upower says 0 when it doesn't know yet
fn seconds(value: &Value) -> Option<Duration> {
    match value.number() {
        seconds if seconds > 0.0 => Some(Duration::from_secs(seconds as u64)),
        _ => None,
    }
}
