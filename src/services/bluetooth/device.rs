use crate::Value;

// a phone, headset or other device bluez knows about
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothDevice {
    // the d-bus object, used to connect, pair and forget
    pub(crate) path: String,

    pub(crate) name: String,

    // it has a name of its own, or one given here, not just its address
    pub(crate) named: bool,

    pub(crate) address: String,

    // a freedesktop icon name like "audio-headset"
    pub(crate) icon: String,

    pub(crate) paired: bool,

    pub(crate) connected: bool,

    // reconnects without asking, which pairing from amane turns on
    pub(crate) trusted: bool,

    // refused by bluez, it can't connect until unblocked
    pub(crate) blocked: bool,

    // 0 to 100, only for devices that report it
    pub(crate) battery: Option<u8>,

    /*
     * 1 to 4 bars, only while it's being heard in a search; whole bars
     * so the list isn't redrawn on every small change of signal
     */
    pub(crate) signal: Option<u8>,
}

impl BluetoothDevice {
    // interfaces is what bluez lists for one object: interface name -> properties
    pub(crate) fn from_interfaces(path: &str, interfaces: &Value) -> Self {
        let device = interfaces.get("org.bluez.Device1");
        let battery = interfaces.get("org.bluez.Battery1");

        let address = device.get("Address").text();

        // unnamed devices show their address, like other bluetooth menus do
        let name = match device.get("Alias").text() {
            "" => address,
            alias => alias,
        };

        // without a name bluez makes the alias from the address, with dashes
        let named = !device.get("Name").text().is_empty()
            || (name != address && name != address.replace(':', "-"));

        let battery = match battery.get("Percentage") {
            Value::Nothing => None,
            percent => Some(percent.number().clamp(0.0, 100.0) as u8),
        };

        let signal = match device.get("RSSI") {
            Value::Nothing => None,
            rssi => Some(bars(rssi.number())),
        };

        Self {
            path: String::from(path),
            name: String::from(name),
            named,
            address: String::from(address),
            icon: String::from(device.get("Icon").text()),
            paired: device.get("Paired").bool() || device.get("Bonded").bool(),
            connected: device.get("Connected").bool(),
            trusted: device.get("Trusted").bool(),
            blocked: device.get("Blocked").bool(),
            battery,
            signal,
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    // whether it has a name of its own, not just its address
    pub fn named(&self) -> bool {
        self.named
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn icon(&self) -> &str {
        &self.icon
    }

    pub fn paired(&self) -> bool {
        self.paired
    }

    pub fn connected(&self) -> bool {
        self.connected
    }

    pub fn trusted(&self) -> bool {
        self.trusted
    }

    pub fn blocked(&self) -> bool {
        self.blocked
    }

    pub fn battery(&self) -> Option<u8> {
        self.battery
    }

    pub fn signal(&self) -> Option<u8> {
        self.signal
    }
}

// signal strength in dBm as bars, the way phones draw it
fn bars(rssi: f64) -> u8 {
    match rssi {
        r if r >= -60.0 => 4,
        r if r >= -70.0 => 3,
        r if r >= -80.0 => 2,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(entries: &[(&str, &str)]) -> BluetoothDevice {
        let properties = Value::Map(
            entries
                .iter()
                .map(|(key, value)| (key.to_string(), Value::Text(value.to_string())))
                .collect(),
        );
        let interfaces = Value::Map([("org.bluez.Device1".to_string(), properties)].into());

        BluetoothDevice::from_interfaces("/org/bluez/hci0/dev_x", &interfaces)
    }

    #[test]
    fn an_alias_made_from_the_address_is_no_name() {
        let unnamed = device(&[
            ("Address", "04:4B:ED:9D:B5:65"),
            ("Alias", "04-4B-ED-9D-B5-65"),
        ]);
        assert!(!unnamed.named());

        let named = device(&[
            ("Address", "04:4B:ED:9D:B5:65"),
            ("Alias", "Pixel 8"),
            ("Name", "Pixel 8"),
        ]);
        assert!(named.named());

        // renamed here, with no name of its own
        let renamed = device(&[("Address", "04:4B:ED:9D:B5:65"), ("Alias", "Desk lamp")]);
        assert!(renamed.named());
    }

    #[test]
    fn signal_is_whole_bars() {
        assert_eq!(bars(-40.0), 4);
        assert_eq!(bars(-65.0), 3);
        assert_eq!(bars(-79.0), 2);
        assert_eq!(bars(-95.0), 1);
    }
}
