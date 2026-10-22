use crate::services::worker;
use crate::{Argument, Bus, Service, Value};

const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

// where power-profiles-daemon answers: bus name, object path and interface
type Endpoint = (&'static str, &'static str, &'static str);

/*
 * older daemons only answer under their first name, newer ones also
 * under upower's, so both are tried instead of assuming one
 */
const ENDPOINTS: [Endpoint; 2] = [
    (
        "net.hadess.PowerProfiles",
        "/net/hadess/PowerProfiles",
        "net.hadess.PowerProfiles",
    ),
    (
        "org.freedesktop.UPower.PowerProfiles",
        "/org/freedesktop/UPower/PowerProfiles",
        "org.freedesktop.UPower.PowerProfiles",
    ),
];

#[derive(Default, PartialEq)]
pub struct PowerProfile {
    // none when power-profiles-daemon isn't installed
    endpoint: Option<Endpoint>,

    // "power-saver", "balanced" or "performance"
    active: String,

    // some machines have no "performance"
    available: Vec<String>,

    // why performance is held back, like "high-operating-temperature"
    degraded: Option<String>,
}

// the daemon announces every change with a signal, so nothing is polled
impl Service for PowerProfile {
    fn new() -> Self {
        for endpoint in ENDPOINTS {
            if let Some(profile) = fetch(endpoint) {
                return profile;
            }
        }

        Self::default()
    }

    fn listen() {
        let Some(endpoint) = Self::read().endpoint else {
            return;
        };

        let changes = Bus::system().signals(PROPERTIES, "PropertiesChanged");

        // a change between new() and watching would otherwise wait for the next one
        refresh(endpoint);

        for signal in changes {
            if signal.path() != endpoint.1 {
                continue;
            }

            refresh(endpoint);
        }
    }
}

impl PowerProfile {
    pub fn present(&self) -> bool {
        self.endpoint.is_some()
    }

    pub fn active(&self) -> &str {
        &self.active
    }

    pub fn available(&self) -> &[String] {
        &self.available
    }

    pub fn degraded(&self) -> Option<&str> {
        self.degraded.as_deref()
    }

    // one of available(); the daemon ignores anything else
    pub fn set(profile: &str) {
        let profile = String::from(profile);

        worker::run(move || {
            let Some(endpoint) = Self::read().endpoint else {
                return;
            };

            let (name, path, interface) = endpoint;

            let value = Argument::from(profile.as_str());

            Bus::system().set_property(name, path, interface, "ActiveProfile", value);

            refresh(endpoint);
        });
    }
}

// none when the daemon doesn't answer at this endpoint
fn fetch(endpoint: Endpoint) -> Option<PowerProfile> {
    let (name, path, interface) = endpoint;

    let arguments = [Argument::from(interface)];

    let properties = Bus::system().call(name, path, PROPERTIES, "GetAll", &arguments);

    if !matches!(properties, Value::Map(_)) {
        return None;
    }

    let available = properties
        .get("Profiles")
        .list()
        .iter()
        .map(|profile| String::from(profile.get("Profile").text()))
        .collect();

    let degraded = match properties.get("PerformanceDegraded").text() {
        "" => None,
        reason => Some(String::from(reason)),
    };

    Some(PowerProfile {
        endpoint: Some(endpoint),
        active: String::from(properties.get("ActiveProfile").text()),
        available,
        degraded,
    })
}

// the daemon is asked outside the lock, and nothing is written when nothing changed
fn refresh(endpoint: Endpoint) {
    let Some(fresh) = fetch(endpoint) else {
        return;
    };

    if *PowerProfile::read() == fresh {
        return;
    }

    *PowerProfile::write() = fresh;
}
