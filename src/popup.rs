//! An xdg-popup transient surface — menus, submenus, tooltips, hover cards.
//!
//! Popups are the Wayland primitive built for exactly this UI: transient,
//! positioned relative to a parent via `xdg_positioner`, and (for menus)
//! dismissed by the compositor on outside-click when `.grab()` is set.
//!
//! A popup's view fn runs every frame like a `LayerWindow`'s. Returning
//! `.visible(false)` tells the backend to dismiss any live popup; returning
//! `.visible(true)` tells it to (re)create one on the next frame.

#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo, missing_docs)]

use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use crate::changes;
use crate::input::KeyHandlers;
use crate::{Key, Widget};

/// Which way the popup grows from its anchor point (xdg_positioner gravity).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Gravity {
    /// Centered on the anchor point.
    None,
    /// Upward.
    Top,
    /// Downward.
    Bottom,
    /// To the left.
    Left,
    /// To the right.
    Right,
    /// Up and to the left.
    TopLeft,
    /// Up and to the right.
    TopRight,
    /// Down and to the left.
    BottomLeft,
    /// Down and to the right, the default.
    #[default]
    BottomRight,
}

/// Which point of the anchor rectangle the popup is placed from (xdg_positioner anchor).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Anchor {
    /// The rectangle's center.
    None,
    /// The middle of its top edge.
    Top,
    /// The middle of its bottom edge.
    Bottom,
    /// The middle of its left edge.
    Left,
    /// The middle of its right edge.
    Right,
    /// Its top left corner, the default.
    #[default]
    TopLeft,
    /// Its top right corner.
    TopRight,
    /// Its bottom left corner.
    BottomLeft,
    /// Its bottom right corner.
    BottomRight,
}

bitflags::bitflags! {
    /// How the compositor may move or resize a popup that would go off screen
    /// (xdg_positioner constraint adjustment); sliding and flipping by default.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ConstraintAdjustment: u32 {
        /// Leave it where it is, even off screen.
        const NONE   = 0;
        /// Slide it left or right until it fits.
        const SLIDE_X = 1;
        /// Slide it up or down until it fits.
        const SLIDE_Y = 2;
        /// Flip its anchor and gravity left to right.
        const FLIP_X  = 4;
        /// Flip its anchor and gravity top to bottom.
        const FLIP_Y  = 8;
        /// Make it narrower.
        const RESIZE_X = 16;
        /// Make it shorter.
        const RESIZE_Y = 32;
    }
}

impl Default for ConstraintAdjustment {
    fn default() -> Self {
        Self::SLIDE_X | Self::SLIDE_Y | Self::FLIP_X | Self::FLIP_Y
    }
}

/// The area of the parent a popup is placed against, in logical pixels from the parent's
/// top left, like the button a menu opens from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnchorRect {
    /// The left edge.
    pub x: i32,
    /// The top edge.
    pub y: i32,
    /// The width.
    pub width: i32,
    /// The height.
    pub height: i32,
}

/// A popup's description, returned by its view fn every frame: Popup::new(namespace),
/// then .size(), then .anchor_rect(), then the rest.
pub struct Popup {
    pub(crate) namespace: &'static str,

    // the parent surface; None = parent to the first registered LayerWindow
    pub(crate) parent: Option<&'static str>,

    // a normal window to open over instead, by its name
    pub(crate) parent_window: Option<String>,

    pub(crate) size: (u32, u32),
    pub(crate) anchor_rect: AnchorRect,
    pub(crate) anchor: Anchor,
    pub(crate) gravity: Gravity,
    pub(crate) constraint_adjustment: ConstraintAdjustment,
    pub(crate) offset: (i32, i32),

    pub(crate) grab: bool,
    pub(crate) visible: bool,

    pub(crate) root: Option<Box<dyn Widget>>,
    pub(crate) keys: KeyHandlers,
    pub(crate) on_dismiss: Option<Rc<dyn Fn()>>,

    // its grey content blended with what's behind it, see vibrant
    pub(crate) vibrant: bool,
}

pub struct PopupNeedsSize {
    namespace: &'static str,
}

pub struct PopupNeedsAnchorRect {
    namespace: &'static str,
    size: (u32, u32),
}

impl Popup {
    /// Starts a popup named `namespace`, which tells it apart from the program's other surfaces;
    /// its size comes next.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(namespace: &'static str) -> PopupNeedsSize {
        PopupNeedsSize { namespace }
    }

    /// Hangs it under the layer window or popup with this namespace, like a submenu under its
    /// menu; the first layer window otherwise.
    pub fn parent(mut self, parent: &'static str) -> Self {
        self.parent = Some(parent);
        self
    }

    /// Opens over the normal window with this name, like a menu inside an app.
    pub fn parent_window(mut self, name: &str) -> Self {
        self.parent_window = Some(String::from(name));
        self
    }

    /// Which point of the anchor rectangle it's placed from.
    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Which way it grows from the anchor point.
    pub fn gravity(mut self, gravity: Gravity) -> Self {
        self.gravity = gravity;
        self
    }

    /// How the compositor may move or resize it to keep it on screen; slides and flips by
    /// default.
    pub fn constraint_adjustment(mut self, c: ConstraintAdjustment) -> Self {
        self.constraint_adjustment = c;
        self
    }

    /// Moves it from where the anchor and gravity put it, in logical pixels.
    pub fn offset(mut self, x: i32, y: i32) -> Self {
        self.offset = (x, y);
        self
    }

    /// Its light and dark gray content blended with what the compositor
    /// frosted behind it rather than painted over it, on a compositor with
    /// niri-vibrancy: labels that stay legible on any backdrop.
    pub fn vibrant(mut self, vibrant: bool) -> Self {
        self.vibrant = vibrant;

        self
    }

    /// Takes the keyboard and pointer, so the compositor closes it on a click outside, like a
    /// menu.
    pub fn grab(mut self) -> Self {
        self.grab = true;
        self
    }

    /// False closes a popup that is open; true opens it again on the next frame.
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Runs for each key pressed while it has the keyboard.
    pub fn on_key(mut self, handler: impl Fn(Key) + 'static) -> Self {
        self.keys.press = Some(Rc::new(handler));
        self
    }

    /// Runs for each key let go while it has the keyboard.
    pub fn on_key_release(mut self, handler: impl Fn(Key) + 'static) -> Self {
        self.keys.release = Some(Rc::new(handler));
        self
    }

    /// Runs first for keys pressed with ctrl, alt or super held, before a focused text
    /// input; return true for a chord it used, like Ctrl+S, so the input doesn't see it.
    pub fn on_shortcut(mut self, handler: impl Fn(Key) -> bool + 'static) -> Self {
        self.keys.shortcut = Some(Rc::new(handler));
        self
    }

    /// Runs when the compositor closes it, like on a click outside.
    pub fn on_dismiss(mut self, handler: impl Fn() + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(handler));
        self
    }

    /// What it shows.
    pub fn child(mut self, child: impl Widget + 'static) -> Self {
        self.root = Some(Box::new(child));
        self
    }
}

impl PopupNeedsSize {
    pub fn size(self, width: u32, height: u32) -> PopupNeedsAnchorRect {
        PopupNeedsAnchorRect {
            namespace: self.namespace,
            size: (width, height),
        }
    }
}

impl PopupNeedsAnchorRect {
    pub fn anchor_rect(self, x: i32, y: i32, width: i32, height: i32) -> Popup {
        Popup {
            namespace: self.namespace,
            parent: None,
            parent_window: None,
            size: self.size,
            anchor_rect: AnchorRect {
                x,
                y,
                width,
                height,
            },
            anchor: Anchor::default(),
            gravity: Gravity::default(),
            constraint_adjustment: ConstraintAdjustment::default(),
            offset: (0, 0),
            grab: false,
            visible: true,
            root: None,
            keys: KeyHandlers::default(),
            on_dismiss: None,
            vibrant: false,
        }
    }
}

// a popup view that can carry its own data, like where a context menu was asked for
pub(crate) type PopupView = Arc<dyn Fn() -> Popup + Send + Sync>;

// popups asked for from handlers, given a slot by the event loop when it wakes
pub(crate) static OPENING: Mutex<Vec<PopupView>> = Mutex::new(Vec::new());

/// Opens a popup from a handler, like a context menu on a right click; its slot lasts
/// until the popup closes, by .visible(false), a click outside or its parent closing.
/// Opening one with the namespace of a popup that is open replaces it.
pub fn open_popup(view: impl Fn() -> Popup + Send + Sync + 'static) {
    OPENING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Arc::new(view));

    changes::mark_all();
}
