use std::rc::Rc;

use crate::input::KeyHandlers;
use crate::{BlurShape, Full, Key, Widget};

pub struct LayerWindow {
    pub(crate) width: WindowSize,
    pub(crate) height: WindowSize,

    pub(crate) vertical: Vertical,
    pub(crate) horizontal: Horizontal,

    pub(crate) margin: Margin,
    pub(crate) layer: Layer,
    pub(crate) keyboard: Keyboard,
    pub(crate) zone: Zone,

    pub(crate) namespace: &'static str,

    pub(crate) visible: bool,

    // none means the whole window takes the pointer
    pub(crate) input_region: Option<Vec<InputArea>>,
    pub(crate) blur_region: Option<Vec<InputArea>>,
    pub(crate) blur_shapes: Option<Vec<BlurShape>>,

    pub(crate) root: Option<Box<dyn Widget>>,

    pub(crate) keys: KeyHandlers,
}

pub struct LayerWindowNeedsWidth;

pub struct LayerWindowNeedsHeight {
    pub(crate) width: WindowSize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowSize {
    Full,
    Fixed(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Vertical {
    Top,

    #[default]
    Middle,

    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Horizontal {
    Left,

    #[default]
    Middle,

    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Margin {
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub left: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layer {
    // under everything
    Background,

    // under normal windows
    Bottom,

    // above normal windows
    Top,

    // above everything
    #[default]
    Overlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Keyboard {
    // never takes keyboard focus
    #[default]
    None,

    // takes all keyboard input while open
    Exclusive,

    // takes focus when clicked
    OnDemand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Zone {
    // keeps other windows out of the edge the window sits on
    Reserve,

    // reserves nothing, stays out of what others reserve
    #[default]
    Respect,

    // reserves nothing, covers what others reserve
    Ignore,
}

// a part of the window that takes pointer input, from its top left corner
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputArea {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl LayerWindow {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> LayerWindowNeedsWidth {
        LayerWindowNeedsWidth
    }

    pub fn anchor_vertical(mut self, vertical: Vertical) -> Self {
        self.vertical = vertical;

        self
    }

    pub fn anchor_horizontal(mut self, horizontal: Horizontal) -> Self {
        self.horizontal = horizontal;

        self
    }

    pub fn margin(mut self, margin: Margin) -> Self {
        self.margin = margin;

        self
    }

    pub fn layer(mut self, layer: Layer) -> Self {
        self.layer = layer;

        self
    }

    pub fn keyboard(mut self, keyboard: Keyboard) -> Self {
        self.keyboard = keyboard;

        self
    }

    pub fn space(mut self, zone: Zone) -> Self {
        self.zone = zone;

        self
    }

    // the name compositors match rules on, like niri's layer-rule; only read when the window opens
    pub fn namespace(mut self, namespace: &'static str) -> Self {
        self.namespace = namespace;

        self
    }

    // a hidden window keeps running and shows again once the view says so
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;

        self
    }

    // only these areas take the pointer, everywhere else clicks go to the windows below
    pub fn input_region(mut self, areas: Vec<InputArea>) -> Self {
        self.input_region = Some(areas);

        self
    }

    // no area takes the pointer, so every click goes to the windows below
    pub fn click_through(self) -> Self {
        self.input_region(Vec::new())
    }

    /*
     * only these areas get the compositor's blur behind them (ext-background-effect);
     * none set leaves it to the compositor's own rules, and an empty list blurs nowhere
     */
    pub fn blur_region(mut self, areas: Vec<InputArea>) -> Self {
        self.blur_region = Some(areas);

        self
    }

    /*
     * rounded shapes the compositor blurs behind, each with its own fade and
     * material; they take the place of a blur region, and where the
     * compositor lacks them they become one (see BlurShape)
     */
    pub fn blur_shapes(mut self, shapes: Vec<BlurShape>) -> Self {
        self.blur_shapes = Some(shapes);

        self
    }

    // keys only arrive while the window has keyboard focus, see Keyboard
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

    pub fn child(mut self, child: impl Widget + 'static) -> Self {
        self.root = Some(Box::new(child));

        self
    }
}

impl LayerWindowNeedsWidth {
    pub fn width(self, width: impl Into<WindowSize>) -> LayerWindowNeedsHeight {
        LayerWindowNeedsHeight {
            width: width.into(),
        }
    }
}

impl LayerWindowNeedsHeight {
    pub fn height(self, height: impl Into<WindowSize>) -> LayerWindow {
        LayerWindow {
            width: self.width,
            height: height.into(),

            vertical: Vertical::default(),
            horizontal: Horizontal::default(),

            margin: Margin::default(),
            layer: Layer::default(),
            keyboard: Keyboard::default(),
            zone: Zone::default(),

            namespace: "amane",

            visible: true,

            input_region: None,
            blur_region: None,
            blur_shapes: None,

            root: None,

            keys: KeyHandlers::default(),
        }
    }
}

impl From<f32> for WindowSize {
    fn from(pixels: f32) -> Self {
        Self::Fixed(pixels)
    }
}

impl From<Full> for WindowSize {
    fn from(_: Full) -> Self {
        Self::Full
    }
}
