#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo, missing_docs)]

/// Why a call failed: the error's d-bus name and the text the service sent with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusError {
    // like org.bluez.Error.Failed, or org.freedesktop.DBus.Error.NoReply
    pub(crate) name: String,

    // empty when the service sent no text
    pub(crate) message: String,
}

impl BusError {
    /// An error with its d-bus name, like org.bluez.Error.Failed, and the text sent with it.
    pub fn new(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            message: message.into(),
        }
    }

    /// The error's d-bus name, like org.bluez.Error.Failed or org.freedesktop.DBus.Error.NoReply.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The text the service sent with the error; empty when it sent none.
    pub fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn no_bus() -> Self {
        Self::new(
            "org.freedesktop.DBus.Error.Disconnected",
            "the bus could not be reached",
        )
    }
}

impl From<zbus::Error> for BusError {
    fn from(error: zbus::Error) -> Self {
        match error {
            zbus::Error::MethodError(name, message, _) => {
                Self::new(name.as_str(), message.unwrap_or_default())
            }
            zbus::Error::FDO(error) => {
                let name = zbus::DBusError::name(error.as_ref()).to_string();
                let message = zbus::DBusError::description(error.as_ref()).unwrap_or_default();

                Self::new(name, message)
            }
            // a broken connection or a reply that doesn't parse, there is no d-bus name for it
            other => Self::new("org.freedesktop.DBus.Error.Failed", other.to_string()),
        }
    }
}
