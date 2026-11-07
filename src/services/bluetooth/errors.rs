#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo, missing_docs)]

//! BlueZ's errors as [`ServiceError`]s: its own names first, then the reason it puts in the
//! text of a plain `org.bluez.Error.Failed`. How a failure reads is the UI's choice.

use crate::{BusError, ErrorKind, ServiceError};

/*
 * none when the call failed only because what was asked for is already
 * so, like connecting a device that is connected; that counts as done
 */
pub(crate) fn from_bus(error: &BusError) -> Option<ServiceError> {
    let name = error.name().trim_start_matches("org.bluez.Error.");
    let message = error.message();

    let error = match name {
        "AlreadyConnected" | "AlreadyExists" => return None,

        "AuthenticationFailed" => ErrorKind::AuthenticationFailed,
        "AuthenticationRejected" | "Rejected" => ErrorKind::Rejected,
        "AuthenticationCanceled" | "Canceled" => ErrorKind::Canceled,
        "AuthenticationTimeout" => ErrorKind::TimedOut,
        "ConnectionAttemptFailed" | "PageTimeout" | "HostDown" => ErrorKind::Unreachable,
        "NotReady" => ErrorKind::NotReady,
        "Blocked" => ErrorKind::Blocked,
        "NotSupported" | "NotAvailable" => ErrorKind::NotSupported,
        "DoesNotExist" => ErrorKind::Gone,

        "org.freedesktop.DBus.Error.NoReply" | "org.freedesktop.DBus.Error.Timeout" => {
            ErrorKind::TimedOut
        }
        "org.freedesktop.DBus.Error.UnknownObject" | "org.freedesktop.DBus.Error.UnknownMethod" => {
            ErrorKind::Gone
        }

        // bluez puts the reason for most failed connects in the message of a plain Failed
        _ => from_message(message),
    };

    Some(ServiceError::new(error, message))
}

// the reasons bluez gives in the text of org.bluez.Error.Failed
fn from_message(message: &str) -> ErrorKind {
    let lower = message.to_lowercase();
    let has = |part: &str| lower.contains(part);

    if has("rfkill") || has("blocked") {
        ErrorKind::Blocked
    } else if has("profile-unavailable") || has("not supported") {
        ErrorKind::NotSupported
    } else if has("page-timeout")
        || has("connection-refused")
        || has("connection-aborted")
        || has("le-connection-abort")
        || has("host is down")
        || has("create-socket")
        || has("connection-unknown")
        || has("software caused connection abort")
    {
        ErrorKind::Unreachable
    } else if has("timeout") || has("timed out") {
        ErrorKind::TimedOut
    } else if has("authentication") {
        ErrorKind::AuthenticationFailed
    } else if has("not ready") || has("not powered") {
        ErrorKind::NotReady
    } else {
        ErrorKind::Failed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(name: &str, message: &str) -> Option<ErrorKind> {
        from_bus(&BusError::new(name, message)).map(|error| error.kind)
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
            Some(ErrorKind::AuthenticationFailed)
        );
        assert_eq!(
            error("org.bluez.Error.NotReady", "Resource Not Ready"),
            Some(ErrorKind::NotReady)
        );
        assert_eq!(
            error(
                "org.freedesktop.DBus.Error.NoReply",
                "Did not receive a reply"
            ),
            Some(ErrorKind::TimedOut)
        );
    }

    #[test]
    fn keeps_what_bluez_said() {
        let failed = from_bus(&BusError::new("org.bluez.Error.Failed", "something new"));
        assert_eq!(
            failed.map(|error| error.message),
            Some(String::from("something new"))
        );
    }

    #[test]
    fn a_plain_failed_is_read_from_its_message() {
        assert_eq!(
            error("org.bluez.Error.Failed", "br-connection-page-timeout"),
            Some(ErrorKind::Unreachable)
        );
        assert_eq!(
            error("org.bluez.Error.Failed", "le-connection-abort-by-local"),
            Some(ErrorKind::Unreachable)
        );
        assert_eq!(
            error(
                "org.bluez.Error.Failed",
                "br-connection-profile-unavailable"
            ),
            Some(ErrorKind::NotSupported)
        );
        assert_eq!(
            error("org.bluez.Error.Failed", "Blocked through rfkill"),
            Some(ErrorKind::Blocked)
        );
        assert_eq!(
            error("org.bluez.Error.Failed", "something new"),
            Some(ErrorKind::Failed)
        );
    }
}
