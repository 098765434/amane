#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo, missing_docs)]

use crate::Service;
use crate::compositor;

/// The compositor's keyboard layouts and the one in use, from niri's event stream (the same
/// connection the other niri services share). Empty under another compositor.
#[derive(Default)]
pub struct KeyboardLayouts {
    names: Vec<String>,
    current: u8,
}

impl Service for KeyboardLayouts {
    fn new() -> Self {
        Self::default()
    }

    // on an unsupported compositor this returns at once and the list stays empty
    fn listen() {
        compositor::listen_keyboard_layouts(|names, current| {
            let mut layouts = Self::write();

            if layouts.names == names && layouts.current == current {
                layouts.quiet();

                return;
            }

            layouts.names = names;
            layouts.current = current;
        });
    }
}

impl KeyboardLayouts {
    /// Every layout's name, like "English (US)", in the order the config lists them.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// The index of the layout in use.
    pub fn current_index(&self) -> usize {
        usize::from(self.current)
    }

    /// The name of the layout in use, if there is a list.
    pub fn current(&self) -> Option<&str> {
        self.names.get(self.current_index()).map(String::as_str)
    }

    /// Whether there's more than one layout, so switching means anything.
    pub fn switchable(&self) -> bool {
        self.names.len() > 1
    }
}
