use smithay_client_toolkit::seat::keyboard::{
    KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers,
};
use wayland_client::{
    Connection, QueueHandle,
    protocol::{wl_keyboard::WlKeyboard, wl_surface::WlSurface},
};

use crate::Key;
use crate::input::focus;

use super::{WaylandState, surface::Role};

impl WaylandState {
    /*
     * the newest open popup that grabbed the input, while the focus is on a window
     * that isn't a popup; one the compositor focused itself keeps the keys
     */
    fn grabbing_popup(&self, focused: &WlSurface) -> Option<WlSurface> {
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
    fn key_target(&self) -> Option<WlSurface> {
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

impl KeyboardHandler for WaylandState {
    // keys only say which keyboard they came from, so the focused window is kept here
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        surface: &WlSurface,
        serial: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        self.keyboard_focus = Some(surface.clone());
        self.last_input_serial = Some(serial);

        // a view may look different with the keyboard on it
        self.request_frame_on(surface);
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        surface: &WlSurface,
        _: u32,
    ) {
        self.keyboard_focus = None;

        // keys stop arriving, so no text input can stay focused
        focus::clear();

        self.request_frame_on(surface);
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        serial: u32,
        event: KeyEvent,
    ) {
        self.last_input_serial = Some(serial);

        self.key_down(&event);
    }

    // a compositor that repeats keys itself sends the repeats here
    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key_down(&event);
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        let Some(surface) = self.key_target() else {
            return;
        };

        let Some(window) = self.window(&surface) else {
            return;
        };

        let Some(on_release) = window.keys.release.clone() else {
            return;
        };

        on_release(to_key(&event));

        self.request_frame_on(&surface);
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        let changed = crate::input::modifiers::set(
            modifiers.ctrl,
            modifiers.shift,
            modifiers.alt,
            modifiers.logo,
        );

        // a view may show something while a modifier is held, like a path while Super is
        if changed && let Some(surface) = self.key_target() {
            self.request_frame_on(&surface);
        }
    }
}

fn to_key(event: &KeyEvent) -> Key {
    match event.keysym {
        Keysym::Return | Keysym::KP_Enter => Key::Enter,
        Keysym::Escape => Key::Escape,
        Keysym::Tab | Keysym::ISO_Left_Tab => Key::Tab,
        Keysym::BackSpace => Key::Backspace,
        Keysym::space => Key::Space,
        Keysym::Up | Keysym::KP_Up => Key::Up,
        Keysym::Down | Keysym::KP_Down => Key::Down,
        Keysym::Left | Keysym::KP_Left => Key::Left,
        Keysym::Right | Keysym::KP_Right => Key::Right,
        Keysym::Home | Keysym::KP_Home => Key::Home,
        Keysym::End | Keysym::KP_End => Key::End,
        Keysym::Delete | Keysym::KP_Delete => Key::Delete,
        Keysym::Insert | Keysym::KP_Insert => Key::Insert,
        Keysym::Page_Up | Keysym::KP_Page_Up => Key::PageUp,
        Keysym::Page_Down | Keysym::KP_Page_Down => Key::PageDown,
        keysym if (Keysym::F1..=Keysym::F24).contains(&keysym) => {
            Key::F((keysym.raw() - Keysym::F1.raw() + 1) as u8)
        }
        _ => character(event),
    }
}

// the text a key types already has shift and the keyboard layout applied
fn character(event: &KeyEvent) -> Key {
    // the normal case: the key typed one printable character
    if let Some(text) = &event.utf8 {
        let mut letters = text.chars();
        if let (Some(letter), None) = (letters.next(), letters.next())
            && !letter.is_control()
        {
            return Key::Character(letter);
        }
    }

    // ctrl (etc.) makes keys type invisible control characters; fall back to the
    // keysym's own character so a chord like Ctrl+A arrives as Character('a') — the
    // handler tells it apart from typed text via `modifiers()`.
    match event.keysym.key_char() {
        Some(letter) if !letter.is_control() => Key::Character(letter),
        _ => Key::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(keysym: Keysym, utf8: Option<&str>) -> KeyEvent {
        KeyEvent {
            time: 0,
            raw_code: 0,
            keysym,
            utf8: utf8.map(String::from),
        }
    }

    #[test]
    fn names_navigation_and_function_keys() {
        assert_eq!(to_key(&event(Keysym::Delete, None)), Key::Delete);
        assert_eq!(to_key(&event(Keysym::KP_Page_Down, None)), Key::PageDown);
        assert_eq!(to_key(&event(Keysym::KP_Left, None)), Key::Left);
        assert_eq!(to_key(&event(Keysym::F1, None)), Key::F(1));
        assert_eq!(to_key(&event(Keysym::F12, None)), Key::F(12));
        assert_eq!(to_key(&event(Keysym::ISO_Left_Tab, None)), Key::Tab);
    }

    #[test]
    fn typed_text_and_chords_become_characters() {
        assert_eq!(to_key(&event(Keysym::s, Some("s"))), Key::Character('s'));

        // ctrl makes the key type a control character, the keysym still names it
        assert_eq!(
            to_key(&event(Keysym::s, Some("\u{13}"))),
            Key::Character('s')
        );
    }
}
