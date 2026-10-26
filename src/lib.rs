//! amane, a Rust Wayland shell library: the `ocbk` fork for amane-shell.
//!
//! The fork adds popups, niri compositor services, app windows, blur shapes,
//! parley text and more system services. Each is documented in
//! [`docs/fork/README.md`](https://github.com/098765434/amane/blob/ocbk/docs/fork/README.md).

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
pub use timing::{FrameStats, frame_stats};
// for deriving Settings without depending on serde yourself: #[serde(crate = "amane::serde")]
pub use app::App;
pub use blur::{BlurShape, blur_fades, material_available};
pub use dbus::{Argument, Bus, Method, Signal, Value};
pub use files::watch_file;
pub use full::Full;
pub use graphics::shader_files::{
    pause_shader_time, shader_errors, shader_time, shader_time_paused,
};
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
    BatteryState, Bluetooth, BluetoothDevice, Brightness, Cpu, Desktop, DesktopAction, DesktopApp,
    DeviceKind, Gestures, Idle, Link, Lock, Media, MediaPlayer, Memory, Network, Notification,
    Notifications, Overview, Palette, Pam, PamConfig, PamError, PamMessage, PamMessageKind,
    PamResult, PowerDevice, PowerProfile, Scheme, Service, Timeout, Urgency, WarningLevel,
    WindowInfo, Windows, Workspace, WorkspaceSwitch, Workspaces,
};
pub use style::{Effect, Fill, Image, Input, Mask, Radius, Shadow};
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
