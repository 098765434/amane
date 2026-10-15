use smithay_client_toolkit::seat::keyboard::{
    KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers,
};
use wayland_client::{
    Connection, QueueHandle,
    protocol::{wl_keyboard::WlKeyboard, wl_surface::WlSurface},
};

use crate::Key;
use crate::input::focus;

use super::WaylandState;

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
        let Some(surface) = self.keyboard_focus.clone() else {
            return;
        };

        let key = to_key(&event);

        // a focused text input takes the key before the window's on_key sees it
        if focus::send(key) {
            self.request_frame_on(&surface);

            return;
        }

        let Some(window) = self.window(&surface) else {
            return;
        };

        let Some(on_key) = &window.on_key else {
            return;
        };

        on_key(key);

        // a service the handler changed wakes its own readers, this window redraws for the rest
        self.request_frame_on(&surface);
    }

    // only sent to keyboards made with key repeat, which this one is not
    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
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
        crate::input::modifiers::set(
            modifiers.ctrl,
            modifiers.shift,
            modifiers.alt,
            modifiers.logo,
        );
    }
}

fn to_key(event: &KeyEvent) -> Key {
    match event.keysym {
        Keysym::Return | Keysym::KP_Enter => Key::Enter,
        Keysym::Escape => Key::Escape,
        Keysym::Tab => Key::Tab,
        Keysym::BackSpace => Key::Backspace,
        Keysym::space => Key::Space,
        Keysym::Up => Key::Up,
        Keysym::Down => Key::Down,
        Keysym::Left => Key::Left,
        Keysym::Right => Key::Right,
        Keysym::Home => Key::Home,
        Keysym::End => Key::End,
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
