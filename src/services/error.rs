#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo, missing_docs)]

//! One way for the fork's services to say something failed.
//!
//! A [`ServiceError`] is a typed [`ErrorKind`], the text the other side gave (untranslated,
//! for logs and as a last resort in the UI), and when it happened. Services keep the last
//! failure per thing they act on (a device, a network, a request) until that thing is tried
//! again; they never clear it on a timer. How long a failure shows, and in what words, is the
//! UI's choice: it has [`ServiceError::at`] for the first and [`ErrorKind`] for the second.
//!
//! Blocking calls return `Result<_, ServiceError>`, converted from [`io::Error`],
//! [`BusError`] and [`zbus::Error`].

use std::fmt;
use std::io;
use std::time::Instant;

use crate::BusError;

/// What kind of failure, for the UI to word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The other end couldn't be reached: out of range, switched off, the host down.
    Unreachable,
    /// A password, PIN or key was wrong.
    AuthenticationFailed,
    /// The other end said no.
    Rejected,
    /// Stopped on purpose, by this side or the other.
    Canceled,
    /// No answer in time.
    TimedOut,
    /// Not ready yet, like a radio still powering up.
    NotReady,
    /// Held off, like a radio by rfkill.
    Blocked,
    /// Asked for something the other end can't do.
    NotSupported,
    /// What was asked about isn't there (any more).
    Gone,
    /// Not allowed for this user.
    PermissionDenied,
    /// The service itself isn't running or can't be reached.
    Unavailable,
    /// Anything else; the message says what.
    Failed,
}

/// A failure, what the other side said about it, and when it happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceError {
    /// What kind of failure it was.
    pub kind: ErrorKind,
    /// The other side's own words, untranslated; may be empty.
    pub message: String,
    /// When it happened.
    pub at: Instant,
}

impl ServiceError {
    /// A failure happening now.
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            at: Instant::now(),
        }
    }
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.message.is_empty() {
            write!(f, "{:?}", self.kind)
        } else {
            f.write_str(&self.message)
        }
    }
}

impl std::error::Error for ServiceError {}

impl From<io::Error> for ServiceError {
    fn from(error: io::Error) -> Self {
        let kind = match error.kind() {
            io::ErrorKind::NotFound => ErrorKind::Gone,
            io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => ErrorKind::TimedOut,
            io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::NotConnected
            | io::ErrorKind::BrokenPipe => ErrorKind::Unavailable,
            io::ErrorKind::Unsupported => ErrorKind::NotSupported,
            io::ErrorKind::Interrupted => ErrorKind::Canceled,
            _ => ErrorKind::Failed,
        };
        Self::new(kind, error.to_string())
    }
}

impl From<BusError> for ServiceError {
    fn from(error: BusError) -> Self {
        Self::from(&error)
    }
}

impl From<&BusError> for ServiceError {
    fn from(error: &BusError) -> Self {
        Self::new(kind_of_bus(error.name()), error.message())
    }
}

impl From<zbus::Error> for ServiceError {
    fn from(error: zbus::Error) -> Self {
        Self::from(BusError::from(error))
    }
}

/// The kind for a D-Bus error name, by the standard names; a service with names of its own
/// (BlueZ, NetworkManager) maps those first and falls back to this.
pub fn kind_of_bus(name: &str) -> ErrorKind {
    let short = name.rsplit('.').next().unwrap_or(name);
    match short {
        "NoReply" | "Timeout" | "TimedOut" => ErrorKind::TimedOut,
        "ServiceUnknown" | "NameHasNoOwner" | "Disconnected" | "NoServer" | "Spawn"
        | "SpawnFailed" => ErrorKind::Unavailable,
        "AccessDenied"
        | "AuthFailed"
        | "InteractiveAuthorizationRequired"
        | "NotAuthorized"
        | "PermissionDenied" => ErrorKind::PermissionDenied,
        "UnknownObject" | "UnknownMethod" | "UnknownInterface" | "UnknownProperty"
        | "FileNotFound" | "DoesNotExist" => ErrorKind::Gone,
        "NotSupported" => ErrorKind::NotSupported,
        "Cancelled" | "Canceled" => ErrorKind::Canceled,
        _ => ErrorKind::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_bus_names_get_their_kind() {
        let kind = |name| ServiceError::from(BusError::new(name, "")).kind;
        assert_eq!(
            kind("org.freedesktop.DBus.Error.NoReply"),
            ErrorKind::TimedOut
        );
        assert_eq!(
            kind("org.freedesktop.DBus.Error.ServiceUnknown"),
            ErrorKind::Unavailable
        );
        assert_eq!(
            kind("org.freedesktop.DBus.Error.AccessDenied"),
            ErrorKind::PermissionDenied
        );
        assert_eq!(
            kind("org.freedesktop.DBus.Error.UnknownObject"),
            ErrorKind::Gone
        );
        assert_eq!(kind("org.example.Error.Weird"), ErrorKind::Failed);
    }

    #[test]
    fn io_errors_get_their_kind_and_keep_the_text() {
        let error = ServiceError::from(io::Error::new(io::ErrorKind::PermissionDenied, "no"));
        assert_eq!(error.kind, ErrorKind::PermissionDenied);
        assert_eq!(error.to_string(), "no");
        let error = ServiceError::from(io::Error::from(io::ErrorKind::NotFound));
        assert_eq!(error.kind, ErrorKind::Gone);
    }

    #[test]
    fn shows_the_kind_without_a_message() {
        assert_eq!(
            ServiceError::new(ErrorKind::TimedOut, "").to_string(),
            "TimedOut"
        );
    }
}
