mod agent;
mod helper;

use crate::Service;

pub use agent::{AuthPrompt, AuthRequest};

/*
 * polkit's authentication prompts, for a shell that answers them with
 * serve_agent(): what polkitd asks, what the helper wants typed, and how
 * the last try went
 */
#[derive(Default)]
pub struct Polkit {
    // the question polkitd is asking, waiting for an answer; only with serve_agent()
    request: Option<AuthRequest>,

    // whether polkitd took the registration
    registered: bool,
}

// only written by the agent's own threads
impl Service for Polkit {
    fn new() -> Self {
        Self::default()
    }

    fn listen() {}
}

impl Polkit {
    pub fn auth_request(&self) -> Option<&AuthRequest> {
        self.request.as_ref()
    }

    pub fn is_registered(&self) -> bool {
        self.registered
    }
}
