//! What polkitd asks, as the shell shows it; always built, so a UI compiles without the
//! agent itself (the `polkit-agent` feature).

use crate::ServiceError;

// what polkitd is asking the user to prove
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthRequest {
    pub(crate) id: u64,

    // like org.freedesktop.systemd1.manage-units
    pub(crate) action_id: String,

    // the action's own sentence, like "Authentication is required to start 'sshd.service'."
    pub(crate) message: String,

    // the action's icon name, often empty
    pub(crate) icon: String,

    // the user whose password proves it
    pub(crate) user: String,

    // what the helper asks for right now, once it does
    pub(crate) prompt: Option<AuthPrompt>,

    // lines the helper showed, like a fingerprint reader's
    pub(crate) info: Vec<String>,

    // why the last try failed
    pub(crate) error: Option<ServiceError>,

    // wrong answers so far
    pub(crate) tries: u8,

    // an answer is with the helper, so the window waits
    pub(crate) busy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthPrompt {
    pub(crate) text: String,

    // whether what's typed may show, which a password may not
    pub(crate) echo: bool,
}

impl AuthRequest {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn action_id(&self) -> &str {
        &self.action_id
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn icon(&self) -> &str {
        &self.icon
    }

    pub fn user(&self) -> &str {
        &self.user
    }

    pub fn prompt(&self) -> Option<&AuthPrompt> {
        self.prompt.as_ref()
    }

    pub fn info(&self) -> &[String] {
        &self.info
    }

    /// Why the last try failed: `AuthenticationFailed` for a wrong password, else `Failed` with
    /// the helper's own words. The UI words it.
    pub fn error(&self) -> Option<&ServiceError> {
        self.error.as_ref()
    }

    pub fn tries(&self) -> u8 {
        self.tries
    }

    pub fn busy(&self) -> bool {
        self.busy
    }
}

impl AuthPrompt {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn echo(&self) -> bool {
        self.echo
    }
}
