//! amane — a Rust Wayland shell library (private fork).
//!
//! This crate is a private fork of upstream Amane, extended for use in a
//! niri shell. See `README.md` and `docs/` for the fork's scope and
//! added features.
//!
//! Fork additions (so far):
//! - [`Popup`] — xdg-popup surface role with layer-shell and nested
//!   parenting, `xdg_positioner` configuration, grab semantics, and
//!   per-popup dismiss callbacks. See [`docs/fork/popups.md`](../docs/fork/popups.md).
//! - [`Windows`] — a service exposing the compositor's open windows
//!   ([`WindowInfo`]: focused window, title, app-id) plus typed control
//!   actions (focus/close/fullscreen/move, column nav, spawn, screenshot,
//!   toggle overview). niri-complete. See
//!   [`docs/fork/compositor.md`](../docs/fork/compositor.md).
//! - [`Idle`] — idle levels from the compositor's ext-idle-notify, and
//!   app inhibits through a served `org.freedesktop.ScreenSaver`. See
//!   [`docs/fork/idle.md`](../docs/fork/idle.md).
//! - [`Appearance`] — the desktop's light/dark preference and accent colour
//!   from the XDG Settings portal. See
//!   [`docs/fork/appearance.md`](../docs/fork/appearance.md).

#[cfg(target_env = "gnu")] // skip glibc tuning if there is no glibc
mod allocator;
mod animation;
mod app;
mod blur;
mod changes;
mod clipboard;
mod compositor;
mod config;
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
mod notify;
mod placement;
mod popup;
mod portal;
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
pub use clipboard::{Clipboard, DragData, PrimarySelection};
pub use config::{Config, Settings};
pub use notify::Notify;
pub use portal::FileDialog;
// for deriving Settings without depending on serde yourself: #[serde(crate = "amane::serde")]
pub use app::App;
pub use blur::{BlurShape, blur_fades, material_available};
pub use dbus::{Argument, Bus, Method, Signal, Value};
pub use files::watch_file;
pub use full::Full;
pub use graphics::{Cap, Color, Gradient, Join, Weight};
pub use input::cursor_names::{
    Crosshair, Default, Grab, Grabbing, Move, NotAllowed, Pointer, ResizeBottom, ResizeBottomLeft,
    ResizeBottomRight, ResizeHorizontal, ResizeLeft, ResizeRight, ResizeTop, ResizeTopLeft,
    ResizeTopRight, ResizeVertical, Text, Wait,
};
pub use input::modifiers::get as modifiers;
pub use input::{
    Button, Cursor, Key, Modifiers, Point, Scroll, ScrollSource, click_count, pointer_position,
};
pub use ipc::{IpcCall, call as ipc_call, ipc_socket, ipc_socket_for};
pub use layer_window::{
    Horizontal, InputArea, Keyboard, Layer, LayerWindow, Margin, Vertical, WindowSize, Zone,
};
pub use monitor::Monitor;
pub use placement::{Align, Center, End, Justify, Padding, Size, Start};
pub use popup::{
    Anchor as PopupAnchor, AnchorRect, ConstraintAdjustment, Gravity, Popup, open_popup,
};
pub use process::{lines, output, spawn, spawn_activated};
pub use serde;
pub use services::{
    AccessPoint, Action, Appearance, Apps, Audio, AudioDevice, AudioKind, Backdrop, Battery,
    BatteryState, Bluetooth, BluetoothDevice, Brightness, Cpu, DesktopAction, DesktopApp,
    DeviceKind, Gestures, Idle, Link, Lock, Media, MediaPlayer, Memory, Network, Notification,
    Notifications, Overview, Palette, Pam, PamConfig, PamError, PamMessage, PamMessageKind,
    PamResult, PowerDevice, PowerProfile, Scheme, Service, Timeout, Urgency, WarningLevel,
    WindowInfo, Windows, Workspace, Workspaces,
};
pub use style::{Fill, Image, Mask, Radius, Shadow};
pub use widgets::{
    Arc, Canvas, Case, Circle, Column, Decoration, Line, Path, Rectangle, Row, ScrollArea, Shape,
    Stack, Text, TextAlign, TextDirection, TextInput, Widget,
};

pub use placement::Justify::{SpaceAround, SpaceBetween, SpaceEvenly};
pub use placement::Size::Parent;
pub use window::{
    Window, WindowState, close_window, current_window, minimize_window, open_window,
    open_window_with, window_size, window_state,
};
