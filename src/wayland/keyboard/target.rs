//! Which surface takes the keys (a popup that grabs the keyboard first), and a key going down.

use smithay_client_toolkit::seat::keyboard::KeyEvent;
use wayland_client::protocol::wl_surface::WlSurface;

use crate::input::focus;

use super::super::{WaylandState, surface::Role};

use super::*;

impl WaylandState {
    /*
     * the newest open popup that grabbed the input, while the focus is on a window
     * that isn't a popup; one the compositor focused itself keeps the keys
     */
    pub(super) fn grabbing_popup(&self, focused: &WlSurface) -> Option<WlSurface> {
        let on_popup = self.windows.iter().any(|window| {
            matches!(window.role, Role::Popup { .. }) && window.role.wl_surface() == focused
        });

        if on_popup {
            return None;
        }

        self.windows
            .iter()
            .rev()
            .find_map(|window| match &window.role {
                Role::Popup {
                    grab: true, popup, ..
                } => Some(popup.wl_surface().clone()),
                _ => None,
            })
    }
}

impl WaylandState {
    // the window keys go to; a compositor may keep the focus on the window under a menu
    pub(super) fn key_target(&self) -> Option<WlSurface> {
        let focused = self.keyboard_focus.clone()?;

        Some(self.grabbing_popup(&focused).unwrap_or(focused))
    }

    // a key going down, pressed or repeated while held
    pub fn key_down(&mut self, event: &KeyEvent) {
        let Some(surface) = self.key_target() else {
            return;
        };

        let key = to_key(event);

        let Some(window) = self.window(&surface) else {
            return;
        };

        let keys = window.keys.clone();

        // chords go to the window's shortcuts first, so Ctrl+S saves even while typing
        let shortcut = crate::modifiers().command()
            && keys.shortcut.is_some_and(|on_shortcut| on_shortcut(key));

        // a focused text input takes the key before the window's on_key sees it
        let used = shortcut || focus::send(key);

        if !used && let Some(on_key) = keys.press {
            on_key(key);
        }

        // a service the handler changed wakes its own readers, this window redraws for the rest
        self.request_frame_on(&surface);
    }
}
