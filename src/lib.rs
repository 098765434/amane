//! amane — a Rust Wayland shell library (private fork).
//!
//! This crate is a private fork of upstream Amane, extended for use in a
//! niri shell. See `README.md` and `docs/` for the fork's scope and
//! added features.
//!
//! Fork additions (so far):
//! - [`Popup`] — xdg-popup surface role with layer-shell and nested
//!   parenting, `xdg_positioner` configuration, grab semantics, and
//!   per-popup dismiss callbacks. See [`docs/popups.md`](../docs/popups.md).
//! - [`Windows`] — a service exposing the compositor's open windows
//!   ([`WindowInfo`]: focused window, title, app-id) plus typed control
//!   actions (focus/close/fullscreen/move, column nav, spawn, screenshot,
//!   toggle overview). niri-complete. See
//!   [`docs/compositor.md`](../docs/compositor.md).
//! - [`Idle`] — idle levels from the compositor's ext-idle-notify, and
//!   app inhibits through a served `org.freedesktop.ScreenSaver`. See
//!   [`docs/idle.md`](../docs/idle.md).
//! - [`Appearance`] — the desktop's light/dark preference and accent colour
//!   from the XDG Settings portal. See
//!   [`docs/appearance.md`](../docs/appearance.md).

#[cfg(target_env = "gnu")] // skip glibc tuning if there is no glibc
mod allocator;
mod animation;
mod app;
mod blur;
mod changes;
mod compositor;
mod dbus;
mod files;
mod frame;
mod full;
mod graphics;
mod input;
mod ipc;
mod layer_window;
mod macros;
mod monitor;
mod placement;
mod popup;
mod process;
mod scale;
mod services;
mod style;
mod timing;
mod wayland;
mod widgets;
mod window;

pub use animation::{Animation, Blend, Easing, request_frame};
pub use changes::mark_all;
pub use app::App;
pub use blur::{BlurShape, blur_fades, material_available};
pub use dbus::{Argument, Bus, Method, Signal, Value};
pub use files::watch_file;
pub use full::Full;
pub use graphics::{Cap, Color, Gradient, Weight};
pub use input::cursor_names::{
    Crosshair, Default, Grab, Grabbing, Move, NotAllowed, Pointer, ResizeBottom, ResizeBottomLeft,
    ResizeBottomRight, ResizeHorizontal, ResizeLeft, ResizeRight, ResizeTop, ResizeTopLeft,
    ResizeTopRight, ResizeVertical, Text, Wait,
};
pub use input::{Button, Cursor, Key, Point, Scroll, ScrollSource};
pub use ipc::{IpcCall, ipc_socket};
pub use layer_window::{
    Horizontal, InputArea, Keyboard, Layer, LayerWindow, Margin, Vertical, WindowSize, Zone,
};
pub use monitor::Monitor;
pub use popup::{Anchor as PopupAnchor, AnchorRect, ConstraintAdjustment, Gravity, Popup};
pub use placement::{Align, Center, End, Justify, Padding, Size, Start};
pub use process::{lines, output, spawn};
pub use services::{
    AccessPoint, Action, Appearance, Apps, Audio, AudioDevice, AudioKind, Battery, BatteryState,
    Bluetooth, BluetoothDevice, Brightness, Cpu, DesktopApp, DeviceKind, Gestures, Idle, Link,
    Lock, Media, MediaPlayer, Memory, Network, Notification, Notifications, Overview, Palette, Pam,
    PamConfig, PamError, PamMessage, PamMessageKind, PamResult, PowerDevice, PowerProfile, Scheme,
    Service, Timeout, Urgency, WarningLevel, WindowInfo, Windows, Workspace, Workspaces,
};
pub use style::{Fill, Image, Mask, Radius, Shadow};
pub use widgets::{
    Arc, Canvas, Circle, Column, Line, Path, Rectangle, Row, ScrollArea, Shape, Stack, Text,
    TextInput, Widget,
};

pub use placement::Justify::{SpaceAround, SpaceBetween, SpaceEvenly};
pub use placement::Size::Parent;
pub use window::{Window, close_window, open_window, window_size};
