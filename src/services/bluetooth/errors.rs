use crate::BusError;

// why a connect, pair or other bluetooth action didn't happen
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothError {
    // off, out of range, or connected to something else
    Unreachable,

    // the codes didn't match, or the device dropped the pairing
    AuthenticationFailed,

    // refused on the other device, or here
    Rejected,

    Canceled,

    // nobody answered in time
    TimedOut,

    // the radio is off
    NotReady,

    // rfkill or a hardware switch holds the radio off
    Blocked,

    // nothing on the device amane can connect to, like a phone without audio
    NotSupported,

    // the device went away while being asked
    Gone,

    // anything else, with what bluez said
    Failed(String),
}

impl BluetoothError {
    /*
     * none when the call failed only because what was asked for is already
     * so, like connecting a device that is connected; that counts as done
     */
    pub(crate) fn from_bus(error: &BusError) -> Option<Self> {
        let name = error.name().trim_start_matches("org.bluez.Error.");
        let message = error.message();

        let error = match name {
            "AlreadyConnected" | "AlreadyExists" => return None,

            "AuthenticationFailed" => Self::AuthenticationFailed,
            "AuthenticationRejected" | "Rejected" => Self::Rejected,
            "AuthenticationCanceled" | "Canceled" => Self::Canceled,
            "AuthenticationTimeout" => Self::TimedOut,
            "ConnectionAttemptFailed" | "PageTimeout" | "HostDown" => Self::Unreachable,
            "NotReady" => Self::NotReady,
            "Blocked" => Self::Blocked,
            "NotSupported" | "NotAvailable" => Self::NotSupported,
            "DoesNotExist" => Self::Gone,

            "org.freedesktop.DBus.Error.NoReply" | "org.freedesktop.DBus.Error.Timeout" => {
                Self::TimedOut
            }
            "org.freedesktop.DBus.Error.UnknownObject"
            | "org.freedesktop.DBus.Error.UnknownMethod" => Self::Gone,

            // bluez puts the reason for most failed connects in the message of a plain Failed
            _ => from_message(message),
        };

        Some(error)
    }

    // a few words for a row in a device list
    pub fn label(&self) -> &str {
        match self {
            Self::Unreachable => "Not reachable",
            Self::AuthenticationFailed => "Pairing failed",
            Self::Rejected => "Declined",
            Self::Canceled => "Canceled",
            Self::TimedOut => "Timed out",
            Self::NotReady => "Bluetooth is off",
            Self::Blocked => "Blocked",
            Self::NotSupported => "Not supported",
            Self::Gone => "Not found",
            Self::Failed(_) => "Couldn't connect",
        }
    }

    // a sentence for where there's room
    pub fn description(&self) -> &str {
        match self {
            Self::Unreachable => "Couldn't connect. Make sure it's on and nearby.",
            Self::AuthenticationFailed => "Pairing failed. The codes may not have matched.",
            Self::Rejected => "The pairing was declined.",
            Self::Canceled => "The pairing was canceled.",
            Self::TimedOut => "The device didn't answer in time.",
            Self::NotReady => "Bluetooth is off.",
            Self::Blocked => "Bluetooth is blocked by airplane mode or a hardware switch.",
            Self::NotSupported => "This device has nothing this computer can connect to.",
            Self::Gone => "The device is no longer nearby.",
            Self::Failed(message) if !message.is_empty() => message,
            Self::Failed(_) => "Something went wrong.",
        }
    }
}

// the reasons bluez gives in the text of org.bluez.Error.Failed
fn from_message(message: &str) -> BluetoothError {
    let lower = message.to_lowercase();
    let has = |part: &str| lower.contains(part);

    if has("rfkill") || has("blocked") {
        BluetoothError::Blocked
    } else if has("profile-unavailable") || has("not supported") {
        BluetoothError::NotSupported
    } else if has("page-timeout")
        || has("connection-refused")
        || has("connection-aborted")
        || has("le-connection-abort")
        || has("host is down")
        || has("create-socket")
        || has("connection-unknown")
        || has("software caused connection abort")
    {
        BluetoothError::Unreachable
    } else if has("timeout") || has("timed out") {
        BluetoothError::TimedOut
    } else if has("authentication") {
        BluetoothError::AuthenticationFailed
    } else if has("not ready") || has("not powered") {
        BluetoothError::NotReady
    } else {
        BluetoothError::Failed(message.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(name: &str, message: &str) -> Option<BluetoothError> {
        BluetoothError::from_bus(&BusError::new(name, message))
    }

    #[test]
    fn already_done_is_not_an_error() {
        assert_eq!(error("org.bluez.Error.AlreadyConnected", ""), None);
        assert_eq!(
            error("org.bluez.Error.AlreadyExists", "Already Paired"),
            None
        );
    }

    #[test]
    fn named_errors_map_directly() {
        assert_eq!(
            error("org.bluez.Error.AuthenticationFailed", ""),
            Some(BluetoothError::AuthenticationFailed)
        );
        assert_eq!(
            error("org.bluez.Error.NotReady", "Resource Not Ready"),
            Some(BluetoothError::NotReady)
        );
        assert_eq!(
            error(
                "org.freedesktop.DBus.Error.NoReply",
                "Did not receive a reply"
            ),
            Some(BluetoothError::TimedOut)
        );
    }

    #[test]
    fn a_plain_failed_is_read_from_its_message() {
        assert_eq!(
            error("org.bluez.Error.Failed", "br-connection-page-timeout"),
            Some(BluetoothError::Unreachable)
        );
        assert_eq!(
            error("org.bluez.Error.Failed", "le-connection-abort-by-local"),
            Some(BluetoothError::Unreachable)
        );
        assert_eq!(
            error(
                "org.bluez.Error.Failed",
                "br-connection-profile-unavailable"
            ),
            Some(BluetoothError::NotSupported)
        );
        assert_eq!(
            error("org.bluez.Error.Failed", "Blocked through rfkill"),
            Some(BluetoothError::Blocked)
        );
        assert_eq!(
            error("org.bluez.Error.Failed", "something new"),
            Some(BluetoothError::Failed("something new".into()))
        );
    }

    #[test]
    fn every_label_fits_a_row() {
        let all = [
            BluetoothError::Unreachable,
            BluetoothError::AuthenticationFailed,
            BluetoothError::Rejected,
            BluetoothError::Canceled,
            BluetoothError::TimedOut,
            BluetoothError::NotReady,
            BluetoothError::Blocked,
            BluetoothError::NotSupported,
            BluetoothError::Gone,
            BluetoothError::Failed(String::new()),
        ];

        for error in all {
            assert!(error.label().chars().count() <= 16, "{}", error.label());
        }
    }
}
