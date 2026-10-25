pub mod focus;
pub mod modifiers;
mod pointer;
mod target;

pub use modifiers::Modifiers;
pub use pointer::Pointer;
pub use target::{Handlers, KeyHandlers, Target, clip};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Button {
    #[default]
    Left,

    Right,

    // pressing down on the scroll wheel
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    // a key that types something: a letter, digit or symbol, already shifted
    Character(char),

    Enter,

    Escape,

    Tab,

    Backspace,

    Space,

    Up,

    Down,

    Left,

    Right,

    Home,

    End,

    Delete,

    Insert,

    PageUp,

    PageDown,

    // a function key, F(1) to F(24)
    F(u8),

    // a key amane has no name for yet
    Other,
}

// where the pointer is, measured from a widget's top-left corner
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

thread_local! {
    static POSITION: std::cell::Cell<Point> = const { std::cell::Cell::new(Point { x: 0.0, y: 0.0 }) };
}

/*
 * where the pointer last was, measured from the top-left corner of the window it was on,
 * in the units of Popup::anchor_rect; anchor a context menu here from a click handler
 */
pub fn pointer_position() -> Point {
    POSITION.get()
}

pub(crate) fn set_pointer_position(x: f32, y: f32) {
    POSITION.set(Point { x, y });
}

// how many pixels a compositor usually reports for one wheel step
pub const PIXELS_PER_LINE: f32 = 15.0;

// measured in lines, so a mouse wheel and a touchpad move things the same distance
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Scroll {
    // positive scrolls right
    pub x: f32,

    // positive scrolls down
    pub y: f32,

    // what made it, so a handler can follow fingers but step a wheel
    pub source: ScrollSource,

    // the fingers lifted (or a continuous scroll ended); x and y may be zero
    pub stop: bool,
}

impl Scroll {
    // the scroll in logical pixels, for following fingers one to one
    pub fn pixels(&self) -> (f32, f32) {
        (self.x * PIXELS_PER_LINE, self.y * PIXELS_PER_LINE)
    }
}

// what a scroll came from, as the compositor reports it
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollSource {
    // a wheel that clicks in steps, and what an unknown source counts as
    #[default]
    Wheel,

    // fingers on a touchpad, which end in a stop
    Finger,

    // a continuous source without steps, like a trackpoint
    Continuous,

    // a wheel tilted sideways
    Tilt,
}

// the pointer's look while it is over a widget
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cursor {
    Default,
    Pointer,
    Text,
    Grab,
    Grabbing,
    Move,
    NotAllowed,
    Wait,
    Crosshair,

    ResizeTop,
    ResizeBottom,
    ResizeLeft,
    ResizeRight,
    ResizeTopLeft,
    ResizeTopRight,
    ResizeBottomLeft,
    ResizeBottomRight,
    ResizeHorizontal,
    ResizeVertical,
}

pub mod cursor_names {
    #![allow(non_upper_case_globals)]

    use super::Cursor;

    /*
     * values, not types, so `Text` and `Pointer` can sit next to
     * the Text widget without a Cursor:: prefix
     */
    pub const Default: Cursor = Cursor::Default;
    pub const Pointer: Cursor = Cursor::Pointer;
    pub const Text: Cursor = Cursor::Text;
    pub const Grab: Cursor = Cursor::Grab;
    pub const Grabbing: Cursor = Cursor::Grabbing;
    pub const Move: Cursor = Cursor::Move;
    pub const NotAllowed: Cursor = Cursor::NotAllowed;
    pub const Wait: Cursor = Cursor::Wait;
    pub const Crosshair: Cursor = Cursor::Crosshair;

    pub const ResizeTop: Cursor = Cursor::ResizeTop;
    pub const ResizeBottom: Cursor = Cursor::ResizeBottom;
    pub const ResizeLeft: Cursor = Cursor::ResizeLeft;
    pub const ResizeRight: Cursor = Cursor::ResizeRight;
    pub const ResizeTopLeft: Cursor = Cursor::ResizeTopLeft;
    pub const ResizeTopRight: Cursor = Cursor::ResizeTopRight;
    pub const ResizeBottomLeft: Cursor = Cursor::ResizeBottomLeft;
    pub const ResizeBottomRight: Cursor = Cursor::ResizeBottomRight;
    pub const ResizeHorizontal: Cursor = Cursor::ResizeHorizontal;
    pub const ResizeVertical: Cursor = Cursor::ResizeVertical;
}
