use crate::Value;

// whether the radio is on, and what it's on its way to
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PowerState {
    #[default]
    Off,
    On,
    TurningOn,
    TurningOff,
    // held off by rfkill, airplane mode or a hardware switch
    Blocked,
}

// this computer's bluetooth radio
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothAdapter {
    // the d-bus object, like /org/bluez/hci0
    pub(crate) path: String,

    // what other devices see this computer as
    pub(crate) name: String,

    pub(crate) address: String,

    pub(crate) power: PowerState,

    // searching for devices nearby
    pub(crate) discovering: bool,

    // visible to devices searching, so a phone can pair with it
    pub(crate) discoverable: bool,
}

impl BluetoothAdapter {
    // properties is what bluez lists for org.bluez.Adapter1
    pub(crate) fn from_properties(path: &str, properties: &Value) -> Self {
        let powered = properties.get("Powered").bool();

        // PowerState came with bluez 5.64, Powered alone says enough without it
        let power = match properties.get("PowerState").text() {
            "on" => PowerState::On,
            "off-enabling" => PowerState::TurningOn,
            "on-disabling" => PowerState::TurningOff,
            "off-blocked" => PowerState::Blocked,
            "off" => PowerState::Off,
            _ if powered => PowerState::On,
            _ => PowerState::Off,
        };

        let name = match properties.get("Alias").text() {
            "" => properties.get("Name").text(),
            alias => alias,
        };

        Self {
            path: String::from(path),
            name: String::from(name),
            address: String::from(properties.get("Address").text()),
            power,
            discovering: properties.get("Discovering").bool(),
            discoverable: properties.get("Discoverable").bool(),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn power(&self) -> PowerState {
        self.power
    }

    pub fn powered(&self) -> bool {
        self.power == PowerState::On
    }

    pub fn discovering(&self) -> bool {
        self.discovering
    }

    pub fn discoverable(&self) -> bool {
        self.discoverable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(entries: &[(&str, Value)]) -> BluetoothAdapter {
        let properties = Value::Map(
            entries
                .iter()
                .map(|(key, value)| (key.to_string(), value.clone()))
                .collect(),
        );

        BluetoothAdapter::from_properties("/org/bluez/hci0", &properties)
    }

    #[test]
    fn power_state_reads_rfkill_and_the_way_in_between() {
        let blocked = adapter(&[("PowerState", Value::Text("off-blocked".into()))]);
        assert_eq!(blocked.power(), PowerState::Blocked);
        assert!(!blocked.powered());

        let rising = adapter(&[("PowerState", Value::Text("off-enabling".into()))]);
        assert_eq!(rising.power(), PowerState::TurningOn);
    }

    #[test]
    fn an_older_bluez_only_says_powered() {
        assert_eq!(
            adapter(&[("Powered", Value::Bool(true))]).power(),
            PowerState::On
        );
        assert_eq!(adapter(&[]).power(), PowerState::Off);
    }
}
