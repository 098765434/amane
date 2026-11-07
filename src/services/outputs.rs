#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo, missing_docs)]

//! niri's outputs as [`amane::niri::outputs`](crate::niri::outputs) answers them, kept up to
//! date: read again when the niri fork says they changed (`OutputsChanged`), so nothing polls.
//! On stock niri they are asked every five seconds, only while watched. Empty under another
//! compositor.

use std::time::Duration;

use crate::Service;
use crate::compositor::{self, niri::client};
use crate::services::{IDLE_STOP, store};

/// The connected outputs, by name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outputs {
    outputs: Vec<client::Output>,
}

impl Service for Outputs {
    fn new() -> Self {
        Self {
            outputs: client::outputs().unwrap_or_default(),
        }
    }

    fn interval() -> Duration {
        Duration::from_secs(5)
    }

    fn idle_stop() -> Option<Duration> {
        Some(IDLE_STOP)
    }

    fn listen() {
        if client::features().has("outputs-changed") {
            // a change between new() and listening would otherwise wait for the next one
            refresh();
            compositor::listen_outputs_changed(refresh);
            return;
        }
        if client::outputs().is_err() {
            // not niri: nothing to follow
            return;
        }
        loop {
            std::thread::sleep(Self::interval());
            store::wait_while_unread::<Self>(IDLE_STOP);
            refresh();
        }
    }
}

impl Outputs {
    /// Every connected output, by name.
    pub fn all(&self) -> &[client::Output] {
        &self.outputs
    }

    /// The output called `name`, like `eDP-1`.
    pub fn get(&self, name: &str) -> Option<&client::Output> {
        self.outputs.iter().find(|output| output.name == name)
    }
}

// asked outside the write: niri's answer can take up to two seconds
fn refresh() {
    let Ok(outputs) = client::outputs() else {
        return;
    };
    let same = store::find::<Outputs>()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .outputs
        == outputs;
    if !same {
        Outputs::write().outputs = outputs;
    }
}
