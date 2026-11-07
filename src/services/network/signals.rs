#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo)]

//! What a NetworkManager signal asks of [`Network`](super::Network). Most of them say something
//! that changes what shows and need everything read again. A few come every few seconds and
//! don't: access points' signal strength wavering, a scan's timestamp, a device's traffic
//! counters. Those are answered without the 60-odd calls a full read takes.

use crate::{ErrorKind, ServiceError, Value};

const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const ACCESS_POINT: &str = "org.freedesktop.NetworkManager.AccessPoint";
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const STATISTICS: &str = "org.freedesktop.NetworkManager.Device.Statistics";

// changes that don't alter what shows on their own
const AP_NOISE: [&str; 2] = ["Strength", "LastSeen"];
const WIRELESS_NOISE: [&str; 1] = ["LastScan"];

/// What one signal calls for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Change {
    /// Read everything again, the saved profiles' settings too when `settings`.
    Full { settings: bool },
    /// The joined network's strength moved: no read needed.
    Strength(u8),
    /// Strength or scan noise from a network not joined: worth a read only while a window
    /// lists the networks.
    Listed,
    /// Nothing that shows.
    Nothing,
}

/// Sorts one signal. `joined` is the object path of the access point joined now, if any.
pub(super) fn classify(
    path: &str,
    interface: &str,
    name: &str,
    arguments: &[Value],
    settings_path: &str,
    joined: &str,
) -> Change {
    if path.starts_with(settings_path) {
        return Change::Full { settings: true };
    }

    // the standard signal says whose properties; NetworkManager's older one on each
    // interface carries the changes alone
    let (owner, changed) = match (interface, name) {
        (PROPERTIES, "PropertiesChanged") => {
            (arguments.first().map_or("", Value::text), arguments.get(1))
        }
        (_, "PropertiesChanged") => (interface, arguments.first()),
        _ => return Change::Full { settings: false },
    };
    let Some(Value::Map(changed)) = changed else {
        return Change::Full { settings: false };
    };

    if owner == STATISTICS {
        return Change::Nothing;
    }

    let only = |noise: &[&str]| changed.keys().all(|key| noise.contains(&key.as_str()));

    if owner == ACCESS_POINT && only(&AP_NOISE) {
        if !joined.is_empty() && path == joined {
            return match changed.get("Strength") {
                Some(Value::Number(strength)) => Change::Strength(strength.clamp(0.0, 100.0) as u8),
                _ => Change::Nothing,
            };
        }
        return Change::Listed;
    }

    if owner == WIRELESS && only(&WIRELESS_NOISE) {
        return Change::Listed;
    }

    Change::Full { settings: false }
}

const DEVICE: &str = "org.freedesktop.NetworkManager.Device";

// NetworkManager's device state for a failed activation, and the reasons worth telling apart
const FAILED: f64 = 120.0;
const NO_SECRETS: f64 = 7.0;
const SUPPLICANT_DISCONNECT: f64 = 8.0;
const SUPPLICANT_TIMEOUT: f64 = 11.0;
const SSID_NOT_FOUND: f64 = 53.0;

/// A device's `StateChanged(new, old, reason)` into the failed state, as a failure: a wrong
/// password reads as `AuthenticationFailed`.
pub(super) fn device_failure(
    interface: &str,
    name: &str,
    arguments: &[Value],
) -> Option<ServiceError> {
    if interface != DEVICE || name != "StateChanged" {
        return None;
    }
    let number = |at: usize| match arguments.get(at) {
        Some(Value::Number(number)) => Some(*number),
        _ => None,
    };
    if number(0)? != FAILED {
        return None;
    }
    let reason = number(2).unwrap_or(0.0);
    let kind = match reason {
        NO_SECRETS | SUPPLICANT_DISCONNECT => ErrorKind::AuthenticationFailed,
        SUPPLICANT_TIMEOUT => ErrorKind::TimedOut,
        SSID_NOT_FOUND => ErrorKind::Unreachable,
        _ => ErrorKind::Failed,
    };
    Some(ServiceError::new(
        kind,
        format!("NetworkManager device state reason {reason}"),
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    const SETTINGS: &str = "/org/freedesktop/NetworkManager/Settings";
    const JOINED: &str = "/org/freedesktop/NetworkManager/AccessPoint/7";

    fn changes(entries: &[(&str, Value)]) -> Value {
        Value::Map(
            entries
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    fn standard(path: &str, owner: &str, changed: Value) -> Change {
        classify(
            path,
            PROPERTIES,
            "PropertiesChanged",
            &[
                Value::Text(owner.to_owned()),
                changed,
                Value::List(Vec::new()),
            ],
            SETTINGS,
            JOINED,
        )
    }

    #[test]
    fn the_joined_networks_strength_needs_no_read() {
        let strength = changes(&[("Strength", Value::Number(64.0))]);
        assert_eq!(
            standard(JOINED, ACCESS_POINT, strength.clone()),
            Change::Strength(64)
        );
        assert_eq!(
            standard(
                "/org/freedesktop/NetworkManager/AccessPoint/9",
                ACCESS_POINT,
                strength
            ),
            Change::Listed
        );

        // the older signal, as NetworkManager also sends it
        let legacy = classify(
            JOINED,
            ACCESS_POINT,
            "PropertiesChanged",
            &[changes(&[
                ("Strength", Value::Number(30.0)),
                ("LastSeen", Value::Number(1.0)),
            ])],
            SETTINGS,
            JOINED,
        );
        assert_eq!(legacy, Change::Strength(30));
    }

    #[test]
    fn a_failed_join_says_why() {
        let changed = |new: f64, reason: f64| {
            device_failure(
                DEVICE,
                "StateChanged",
                &[
                    Value::Number(new),
                    Value::Number(70.0),
                    Value::Number(reason),
                ],
            )
            .map(|failure| failure.kind)
        };
        assert_eq!(
            changed(FAILED, NO_SECRETS),
            Some(ErrorKind::AuthenticationFailed)
        );
        assert_eq!(
            changed(FAILED, SSID_NOT_FOUND),
            Some(ErrorKind::Unreachable)
        );
        assert_eq!(changed(FAILED, 1.0), Some(ErrorKind::Failed));
        assert_eq!(changed(100.0, 0.0), None, "activated is no failure");
        assert_eq!(device_failure(ACCESS_POINT, "StateChanged", &[]), None);
    }

    #[test]
    fn noise_is_told_apart_from_real_changes() {
        let device = "/org/freedesktop/NetworkManager/Devices/3";
        assert_eq!(
            standard(
                device,
                WIRELESS,
                changes(&[("LastScan", Value::Number(5.0))])
            ),
            Change::Listed
        );
        assert_eq!(
            standard(
                device,
                STATISTICS,
                changes(&[("TxBytes", Value::Number(5.0))])
            ),
            Change::Nothing
        );
        assert_eq!(
            standard(
                device,
                WIRELESS,
                changes(&[("ActiveAccessPoint", Value::Text(JOINED.into()))])
            ),
            Change::Full { settings: false }
        );
        assert_eq!(
            standard(
                JOINED,
                ACCESS_POINT,
                changes(&[("Ssid", Value::List(Vec::new()))])
            ),
            Change::Full { settings: false }
        );
        assert_eq!(
            classify(
                device,
                "org.freedesktop.NetworkManager.Device",
                "StateChanged",
                &[],
                SETTINGS,
                JOINED
            ),
            Change::Full { settings: false }
        );
        assert_eq!(
            classify(
                &format!("{SETTINGS}/4"),
                "x",
                "Updated",
                &[],
                SETTINGS,
                JOINED
            ),
            Change::Full { settings: true }
        );
    }
}
