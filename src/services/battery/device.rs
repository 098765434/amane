use crate::Value;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BatteryState {
    #[default]
    Unknown,

    Charging,

    Discharging,

    Empty,

    Full,

    // plugged in, but waiting to charge, like a battery kept below 80% to last longer
    PendingCharge,

    PendingDischarge,
}

// ordered, so warning() >= WarningLevel::Low covers low, critical and action
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum WarningLevel {
    #[default]
    None,

    // running on a ups, only for ups devices
    Discharging,

    Low,

    Critical,

    // so low the system is about to hibernate or power off
    Action,
}

// what kind of thing a peripheral is, other kinds keep upower's number
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Mouse,

    Keyboard,

    Phone,

    MediaPlayer,

    Tablet,

    GamingInput,

    Headset,

    Other(u32),
}

// a mouse, keyboard, headset or other device with its own battery
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerDevice {
    // the name the device reports, like "MX Master 3"
    pub(crate) model: String,

    pub(crate) kind: DeviceKind,

    // 0 to 100
    pub(crate) percent: u8,

    pub(crate) state: BatteryState,
}

impl BatteryState {
    pub(crate) fn from_upower(number: u32) -> Self {
        match number {
            1 => Self::Charging,
            2 => Self::Discharging,
            3 => Self::Empty,
            4 => Self::Full,
            5 => Self::PendingCharge,
            6 => Self::PendingDischarge,
            _ => Self::Unknown,
        }
    }
}

impl WarningLevel {
    pub(crate) fn from_upower(number: u32) -> Self {
        match number {
            2 => Self::Discharging,
            3 => Self::Low,
            4 => Self::Critical,
            5 => Self::Action,
            _ => Self::None,
        }
    }
}

impl DeviceKind {
    pub(crate) fn from_upower(number: u32) -> Self {
        match number {
            5 => Self::Mouse,
            6 => Self::Keyboard,
            8 => Self::Phone,
            9 => Self::MediaPlayer,
            10 => Self::Tablet,
            12 => Self::GamingInput,
            14 => Self::Headset,
            other => Self::Other(other),
        }
    }
}

impl PowerDevice {
    // properties is what upower answers for one device
    pub(crate) fn from_properties(properties: &Value) -> Self {
        Self {
            model: String::from(properties.get("Model").text()),
            kind: DeviceKind::from_upower(properties.get("Type").number() as u32),
            percent: properties.get("Percentage").number().round() as u8,
            state: BatteryState::from_upower(properties.get("State").number() as u32),
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn kind(&self) -> DeviceKind {
        self.kind
    }

    pub fn percent(&self) -> u8 {
        self.percent
    }

    pub fn state(&self) -> BatteryState {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warnings_are_ordered() {
        assert!(WarningLevel::Critical >= WarningLevel::Low);
        assert!(WarningLevel::Discharging < WarningLevel::Low);
    }

    #[test]
    fn unknown_numbers_read_as_unknown() {
        assert_eq!(BatteryState::from_upower(0), BatteryState::Unknown);
        assert_eq!(DeviceKind::from_upower(99), DeviceKind::Other(99));
    }
}
