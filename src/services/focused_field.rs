//! The text field that has the keyboard in any app, as an input method sees it.
//!
//! Only an app that called `App::input_method` hears about other apps' fields: it becomes
//! the seat's input method (zwp_input_method_v2), which there is one of. What a field
//! holds is never kept, only what kind of field it is, so a password manager can offer to
//! fill a password field and never reads one.
//!
//! Text goes in through `FocusedField::fill`, which only works on a compositor with
//! niri-secure-fill: the compositor puts it in only if the same field still has the
//! keyboard, and says whose field it is from the app's socket and process, not from what
//! the app calls itself.

#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo)]

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use zeroize::Zeroizing;

use crate::{Service, changes};

/// What a field is for, as the app said (zwp_text_input_v3 content purpose).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Purpose {
    #[default]
    Normal,
    Alpha,
    Digits,
    Number,
    Phone,
    Url,
    Email,
    Name,
    Password,
    Pin,
    Date,
    Time,
    Datetime,
    Terminal,
}

impl Purpose {
    pub(crate) fn from_wire(purpose: u32) -> Self {
        match purpose {
            1 => Self::Alpha,
            2 => Self::Digits,
            3 => Self::Number,
            4 => Self::Phone,
            5 => Self::Url,
            6 => Self::Email,
            7 => Self::Name,
            8 => Self::Password,
            9 => Self::Pin,
            10 => Self::Date,
            11 => Self::Time,
            12 => Self::Datetime,
            13 => Self::Terminal,
            _ => Self::Normal,
        }
    }
}

/// How the app said the field behaves (zwp_text_input_v3 content hints).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hints(pub u32);

impl Hints {
    const HIDDEN_TEXT: u32 = 0x40;
    const SENSITIVE_DATA: u32 = 0x80;
    const MULTILINE: u32 = 0x200;

    // the characters are hidden, like a password's dots
    pub fn hidden_text(self) -> bool {
        self.0 & Self::HIDDEN_TEXT != 0
    }

    // what is typed is private
    pub fn sensitive(self) -> bool {
        self.0 & Self::SENSITIVE_DATA != 0
    }

    pub fn multiline(self) -> bool {
        self.0 & Self::MULTILINE != 0
    }
}

/// Who owns the focused field, as the compositor knows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldOwner {
    /// From the app's socket, none when the compositor couldn't tell.
    pub pid: Option<u32>,

    /// What that process runs, read by the compositor from /proc.
    pub exe: Option<PathBuf>,

    /// The window's app id: what the app calls itself, so only a hint.
    pub app_id: String,

    /// The window's id in the compositor's IPC, none for a layer surface.
    pub window_id: Option<u64>,

    pub in_window: bool,
    pub in_layer: bool,

    /// The app runs in a sandbox (a security context).
    pub sandboxed: bool,

    /// The app has a text input turned on; without one nothing can be filled.
    pub takes_text: bool,
}

/// Why a fill didn't go in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillError {
    /// Another field, window or app has the keyboard now.
    Changed,
    NoTextInput,
    Locked,
    Expired,
    Used,
    TooSoon,
    InvalidText,

    /// The compositor lacks niri-secure-fill, or this app isn't the input method.
    Unsupported,
}

pub type FillDone = Box<dyn FnOnce(Result<(), FillError>) + Send>;

// what fill asked for, sent by the event loop when it wakes
pub(crate) struct FillRequest {
    pub text: Zeroizing<String>,
    pub generation: u64,
    pub done: FillDone,
}

pub(crate) static FILLS: Mutex<Vec<FillRequest>> = Mutex::new(Vec::new());

#[derive(Debug, Default)]
pub struct FocusedField {
    available: bool,
    secure_fill: bool,

    active: bool,
    purpose: Purpose,
    hints: Hints,

    // whether it holds text, when the app said what it holds
    empty: Option<bool>,

    // a new number each time a field gets or loses the keyboard, or its owner is learned
    generation: u64,

    owner: Option<FieldOwner>,

    // the field is one of this app's own
    own: bool,

    // where the compositor put the popup's field, in the popup's coordinates
    field_rect: Option<(i32, i32, i32, i32)>,
}

// changes through the input method's events rather than polling
impl Service for FocusedField {
    fn new() -> Self {
        Self::default()
    }

    fn listen() {}
}

impl FocusedField {
    /// Whether this app is the seat's input method; false without App::input_method, or when
    /// another input method (like fcitx5 or ibus) has the seat.
    pub fn available(&self) -> bool {
        self.available
    }

    /// Whether fill can work: the compositor has niri-secure-fill.
    pub fn can_fill(&self) -> bool {
        self.available && self.secure_fill
    }

    /// A field in some app has the keyboard.
    pub fn active(&self) -> bool {
        self.active
    }

    pub fn purpose(&self) -> Purpose {
        self.purpose
    }

    pub fn hints(&self) -> Hints {
        self.hints
    }

    /// A password field: its purpose, or hidden sensitive text.
    pub fn is_password(&self) -> bool {
        self.purpose == Purpose::Password || (self.hints.sensitive() && self.hints.hidden_text())
    }

    /// Whether the field holds text; none when the app didn't say (a password field never does).
    pub fn empty(&self) -> Option<bool> {
        self.empty
    }

    /// Changes whenever the focused field does; fill only goes to the generation it names.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Who owns the field, once the compositor said.
    pub fn owner(&self) -> Option<&FieldOwner> {
        self.owner.as_ref()
    }

    /// The field is in one of this app's own windows.
    pub fn own(&self) -> bool {
        self.own
    }

    /// Where the field is relative to the popup, once the compositor placed it.
    pub fn field_rect(&self) -> Option<(i32, i32, i32, i32)> {
        self.field_rect
    }

    /*
     * puts text in the field of `generation`, if that field still has the keyboard; done
     * runs on the event loop's thread with how it went. The text is wiped once sent. Call
     * it from anywhere, like the thread that fetched a password
     */
    pub fn fill(
        text: Zeroizing<String>,
        generation: u64,
        done: impl FnOnce(Result<(), FillError>) + Send + 'static,
    ) {
        FILLS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(FillRequest {
                text,
                generation,
                done: Box::new(done),
            });

        changes::wake();
    }

    pub(crate) fn set_available(&mut self, available: bool, secure_fill: bool) {
        self.available = available;
        self.secure_fill = secure_fill;

        if !available {
            self.clear();
        }
    }

    // a field got the keyboard or changed kind
    pub(crate) fn set_field(&mut self, purpose: Purpose, hints: Hints, empty: Option<bool>) {
        let changed = !self.active || self.purpose != purpose || self.hints != hints;

        self.active = true;
        self.purpose = purpose;
        self.hints = hints;
        self.empty = empty;

        if changed {
            self.generation += 1;
            self.owner = None;
            self.field_rect = None;
        }
    }

    pub(crate) fn set_own(&mut self, own: bool) {
        self.own = own;
    }

    pub(crate) fn set_owner(&mut self, generation: u64, owner: FieldOwner) {
        if generation == self.generation && self.active {
            self.owner = Some(owner);
        }
    }

    pub(crate) fn set_field_rect(&mut self, rect: (i32, i32, i32, i32)) {
        self.field_rect = Some(rect);
    }

    pub(crate) fn clear(&mut self) {
        if self.active {
            self.generation += 1;
        }

        self.active = false;
        self.purpose = Purpose::Normal;
        self.hints = Hints::default();
        self.empty = None;
        self.owner = None;
        self.field_rect = None;
        self.own = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_field_is_a_new_generation_without_an_owner() {
        let mut field = FocusedField::default();

        field.set_field(Purpose::Email, Hints::default(), Some(true));
        let first = field.generation();

        field.set_owner(first, FieldOwner::default());
        assert!(field.owner().is_some());

        // the same field said again keeps its generation and owner
        field.set_field(Purpose::Email, Hints::default(), Some(false));
        assert_eq!(field.generation(), first);
        assert!(field.owner().is_some());

        field.set_field(Purpose::Password, Hints::default(), None);
        assert_ne!(field.generation(), first);
        assert!(field.owner().is_none());

        // an owner learned for an older field is dropped
        field.set_owner(first, FieldOwner::default());
        assert!(field.owner().is_none());

        field.clear();
        assert!(!field.active());
    }

    #[test]
    fn password_fields_are_known_by_purpose_or_hints() {
        let mut field = FocusedField::default();

        field.set_field(Purpose::Password, Hints::default(), None);
        assert!(field.is_password());

        field.set_field(Purpose::Normal, Hints(0x40 | 0x80), None);
        assert!(field.is_password());

        field.set_field(Purpose::Normal, Hints(0x80), None);
        assert!(!field.is_password());
    }
}
