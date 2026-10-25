//! An xdg-popup transient surface — menus, submenus, tooltips, hover cards.
//!
//! Popups are the Wayland primitive built for exactly this UI: transient,
//! positioned relative to a parent via `xdg_positioner`, and (for menus)
//! dismissed by the compositor on outside-click when `.grab()` is set.
//!
//! A popup's view fn runs every frame like a `LayerWindow`'s. Returning
//! `.visible(false)` tells the backend to dismiss any live popup; returning
//! `.visible(true)` tells it to (re)create one on the next frame.

use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use crate::changes;
use crate::input::KeyHandlers;
use crate::{Key, Widget};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gravity {
    None,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Default for Gravity {
    fn default() -> Self {
        Self::BottomRight
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    None,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Default for Anchor {
    fn default() -> Self {
        Self::TopLeft
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ConstraintAdjustment: u32 {
        const NONE   = 0;
        const SLIDE_X = 1;
        const SLIDE_Y = 2;
        const FLIP_X  = 4;
        const FLIP_Y  = 8;
        const RESIZE_X = 16;
        const RESIZE_Y = 32;
    }
}

impl Default for ConstraintAdjustment {
    fn default() -> Self {
        Self::SLIDE_X | Self::SLIDE_Y | Self::FLIP_X | Self::FLIP_Y
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnchorRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

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
}

pub struct PopupNeedsSize {
    namespace: &'static str,
}

pub struct PopupNeedsAnchorRect {
    namespace: &'static str,
    size: (u32, u32),
}

impl Popup {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(namespace: &'static str) -> PopupNeedsSize {
        PopupNeedsSize { namespace }
    }

    pub fn parent(mut self, parent: &'static str) -> Self {
        self.parent = Some(parent);
        self
    }

    // opens over the normal window with this name, like a menu inside an app
    pub fn parent_window(mut self, name: &str) -> Self {
        self.parent_window = Some(String::from(name));
        self
    }

    pub fn anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    pub fn gravity(mut self, gravity: Gravity) -> Self {
        self.gravity = gravity;
        self
    }

    pub fn constraint_adjustment(mut self, c: ConstraintAdjustment) -> Self {
        self.constraint_adjustment = c;
        self
    }

    pub fn offset(mut self, x: i32, y: i32) -> Self {
        self.offset = (x, y);
        self
    }

    pub fn grab(mut self) -> Self {
        self.grab = true;
        self
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    pub fn on_key(mut self, handler: impl Fn(Key) + 'static) -> Self {
        self.keys.press = Some(Rc::new(handler));
        self
    }

    pub fn on_key_release(mut self, handler: impl Fn(Key) + 'static) -> Self {
        self.keys.release = Some(Rc::new(handler));
        self
    }

    /*
     * runs first for keys pressed with ctrl, alt or super held, before a focused text
     * input; return true for a chord it used, like Ctrl+S, so the input doesn't see it
     */
    pub fn on_shortcut(mut self, handler: impl Fn(Key) -> bool + 'static) -> Self {
        self.keys.shortcut = Some(Rc::new(handler));
        self
    }

    pub fn on_dismiss(mut self, handler: impl Fn() + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(handler));
        self
    }

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
        }
    }
}

// a popup view that can carry its own data, like where a context menu was asked for
pub(crate) type PopupView = Arc<dyn Fn() -> Popup + Send + Sync>;

// popups asked for from handlers, given a slot by the event loop when it wakes
pub(crate) static OPENING: Mutex<Vec<PopupView>> = Mutex::new(Vec::new());

/*
 * opens a popup from a handler, like a context menu on a right click; its slot lasts
 * until the popup closes, by .visible(false), a click outside or its parent closing.
 * Opening one with the namespace of a popup that is open replaces it
 */
pub fn open_popup(view: impl Fn() -> Popup + Send + Sync + 'static) {
    OPENING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Arc::new(view));

    changes::mark_all();
}
