#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo, missing_docs)]

//! The screencasts niri is running, from its event stream (the connection the other niri
//! services share), so a privacy indicator needs no polling. Empty under another compositor.

use crate::Service;
use crate::compositor::{self, niri::client::Cast};

/// The screencasts running now, in the order they started.
#[derive(Default)]
pub struct Casts {
    casts: Vec<Cast>,
}

impl Service for Casts {
    fn new() -> Self {
        Self::default()
    }

    // on another compositor this returns at once and the list stays empty
    fn listen() {
        compositor::listen_casts(|casts| {
            let mut service = Self::write();
            if service.casts == casts {
                service.quiet();
                return;
            }
            service.casts = casts;
        });
    }
}

impl Casts {
    /// Every screencast, active or waiting to start.
    pub fn all(&self) -> &[Cast] {
        &self.casts
    }

    /// The ones sending frames now.
    pub fn active(&self) -> impl Iterator<Item = &Cast> {
        self.casts.iter().filter(|cast| cast.is_active)
    }

    /// Whether anything is casting the screen or a window now.
    pub fn any_active(&self) -> bool {
        self.active().next().is_some()
    }
}
