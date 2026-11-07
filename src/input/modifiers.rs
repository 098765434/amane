#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo, missing_docs)]

use std::sync::atomic::{AtomicU8, Ordering};

// the keyboard modifiers held right now, set from the compositor's modifier events
// and readable from a key handler (`amane::modifiers()`). A chord like Ctrl+A comes
// through as `Key::Character('a')` with `modifiers().ctrl == true`.
static STATE: AtomicU8 = AtomicU8::new(0);

const CTRL: u8 = 1 << 0;
const SHIFT: u8 = 1 << 1;
const ALT: u8 = 1 << 2;
const LOGO: u8 = 1 << 3;

/// The keyboard modifiers held right now, from modifiers(); a chord like Ctrl+A comes
/// through as `Key::Character('a')` with `ctrl` set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    /// Either Ctrl key.
    pub ctrl: bool,
    /// Either Shift key.
    pub shift: bool,
    /// Either Alt key.
    pub alt: bool,
    /// The "super" / Windows / Command key.
    pub logo: bool,
}

impl Modifiers {
    /// True when any of ctrl/alt/logo is held — i.e. this key is a command chord, not
    /// text to insert (shift alone still types, so it is excluded).
    pub fn command(self) -> bool {
        self.ctrl || self.alt || self.logo
    }
}

// whether that changed what's held
pub(crate) fn set(ctrl: bool, shift: bool, alt: bool, logo: bool) -> bool {
    let bits = (u8::from(ctrl) * CTRL)
        | (u8::from(shift) * SHIFT)
        | (u8::from(alt) * ALT)
        | (u8::from(logo) * LOGO);

    STATE.swap(bits, Ordering::Relaxed) != bits
}

/// The modifiers held right now, as the compositor last reported them; read it from a
/// key handler.
pub fn get() -> Modifiers {
    let bits = STATE.load(Ordering::Relaxed);

    Modifiers {
        ctrl: bits & CTRL != 0,
        shift: bits & SHIFT != 0,
        alt: bits & ALT != 0,
        logo: bits & LOGO != 0,
    }
}
