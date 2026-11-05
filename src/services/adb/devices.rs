use std::net::{IpAddr, SocketAddr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdbState {
    // ready to use
    Device,

    // the phone hasn't allowed this computer's key yet
    Unauthorized,

    Offline,

    // anything else adb says, like "authorizing" or "no permissions"
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbDevice {
    // what adb calls it: a USB serial number, or address:port over the network
    pub serial: String,
    pub state: AdbState,

    // "SM_S921B"; adb puts underscores for spaces
    pub model: Option<String>,
    pub product: Option<String>,
}

impl AdbDevice {
    pub fn ready(&self) -> bool {
        self.state == AdbState::Device
    }

    // the network address of one connected over Wireless debugging, none over USB
    pub fn address(&self) -> Option<IpAddr> {
        self.serial
            .parse::<SocketAddr>()
            .ok()
            .map(|socket| socket.ip())
    }

    // the model as people write it
    pub fn model_name(&self) -> Option<String> {
        self.model.as_ref().map(|model| model.replace('_', " "))
    }
}

/*
 * one list from `adb track-devices -l` (or `adb devices -l` without its
 * heading): a line per device, its serial, its state, then key:value pairs
 */
pub fn parse(list: &str) -> Vec<AdbDevice> {
    list.lines().filter_map(device).collect()
}

fn device(line: &str) -> Option<AdbDevice> {
    let mut words = line.split_whitespace();

    let serial = words.next()?;

    // the heading `adb devices` prints
    if serial == "List" {
        return None;
    }

    let rest: Vec<&str> = words.collect();

    // a state can be several words, like "no permissions (…)", until the first key:value
    let state_words: Vec<&str> = rest
        .iter()
        .take_while(|word| !is_pair(word))
        .copied()
        .collect();

    let state = match state_words.join(" ").as_str() {
        "device" => AdbState::Device,
        "unauthorized" => AdbState::Unauthorized,
        "offline" => AdbState::Offline,
        other => AdbState::Other(other.to_string()),
    };

    let value = |key: &str| {
        rest.iter()
            .find_map(|word| word.strip_prefix(key)?.strip_prefix(':'))
            .map(String::from)
    };

    Some(AdbDevice {
        serial: serial.to_string(),
        state,
        model: value("model"),
        product: value("product"),
    })
}

// a key:value pair, as opposed to a word of the state (which may hold a colon in brackets)
fn is_pair(word: &str) -> bool {
    word.split_once(':').is_some_and(|(key, _)| {
        ["usb", "product", "model", "device", "transport_id"].contains(&key)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_usb_phone_and_a_wireless_one() {
        let list = "\
R5CX12345AB            device usb:1-2 product:e1qxeea model:SM_S921B device:e1q transport_id:1
192.168.1.101:37123    device product:e1qxeea model:SM_S921B device:e1q transport_id:2
";

        let devices = parse(list);

        assert_eq!(devices.len(), 2);
        assert!(devices[0].ready());
        assert_eq!(devices[0].address(), None);
        assert_eq!(devices[0].model_name().as_deref(), Some("SM S921B"));
        assert_eq!(
            devices[1].address(),
            Some("192.168.1.101".parse().expect("an address"))
        );
    }

    #[test]
    fn reads_states_that_arent_ready() {
        let devices = parse(
            "List of devices attached\n\
             emulator-5554 unauthorized transport_id:3\n\
             R5CX no permissions (missing udev rules? user is in the plugdev group); see [http://developer.android.com/tools/device.html] usb:1-1 transport_id:4\n",
        );

        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].state, AdbState::Unauthorized);
        assert!(
            matches!(&devices[1].state, AdbState::Other(state) if state.starts_with("no permissions"))
        );
    }

    #[test]
    fn an_empty_list_has_no_devices() {
        assert!(parse("").is_empty());
    }
}
