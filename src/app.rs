use std::env;
use std::os::unix::net::UnixStream;
use std::sync::Arc;

#[cfg(target_env = "gnu")]
use crate::allocator;
use crate::graphics::{GpuSession, font};
use crate::ipc::{self, ACTIVATE, IpcHandlers, ipc_socket_for};
use crate::popup::Popup;
use crate::scale::ScaleFactor;
use crate::wayland::WaylandApp;
use crate::window::WindowView;
use crate::{LayerWindow, Monitor, Window};

#[derive(Default)]
pub struct App {
    font: Option<String>,
    scale_factor: ScaleFactor,

    windows: Vec<fn() -> LayerWindow>,
    normal_windows: Vec<(String, WindowView)>,
    popups: Vec<fn() -> Popup>,
    per_monitor: Vec<fn(&Monitor) -> LayerWindow>,
    lock: Option<fn(&Monitor) -> LayerWindow>,

    handlers: IpcHandlers,
    ipc_disabled: bool,

    app_id: Option<String>,
    keep_alive: bool,
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn font(mut self, family: &str) -> Self {
        self.font = Some(String::from(family));

        self
    }

    /// Scales every window and its widgets, in addition to the monitor's scale.
    ///
    /// The default is `1.0`. Panics if the factor is not positive and finite.
    pub fn scale_factor(mut self, factor: f32) -> Self {
        self.scale_factor = ScaleFactor::new(factor);

        self
    }

    // each call adds one more window, on the monitor the compositor chooses
    pub fn window(mut self, view: fn() -> LayerWindow) -> Self {
        self.windows.push(view);

        self
    }

    // a regular desktop window with a title bar, next to layer windows; close_window takes the name
    pub fn normal_window(mut self, name: &'static str, view: fn() -> Window) -> Self {
        self.normal_windows
            .push((String::from(name), Arc::new(view)));

        self
    }

    /// Registers a popup slot. The view fn is called each tick to decide
    /// whether to map a transient xdg-popup. Set `.visible(false)` on the
    /// returned `Popup` to tear a live popup down.
    pub fn popup(mut self, view: fn() -> Popup) -> Self {
        self.popups.push(view);
        self
    }

    // one window on every monitor, following monitors as they are plugged in and out
    pub fn window_per_monitor(mut self, view: fn(&Monitor) -> LayerWindow) -> Self {
        self.per_monitor.push(view);

        self
    }

    /*
     * the lock screen, shown on every monitor once Lock::start locks the session;
     * it stays locked until Lock::unlock gets a password pam accepts
     */
    pub fn lock(mut self, view: fn(&Monitor) -> LayerWindow) -> Self {
        self.lock = Some(view);

        self
    }

    // lets `amane ipc call <name> [arguments...]` run the handler while the shell is running
    pub fn ipc(mut self, name: &str, handler: fn(&[String]) -> String) -> Self {
        self.handlers.insert(name, handler);

        self
    }

    // what normal windows are known by when they don't set Window::app_id; "amane" by default
    pub fn app_id(mut self, app_id: &str) -> Self {
        self.app_id = Some(String::from(app_id));

        self
    }

    // keeps running after the last window closes, for an app that opens windows on an ipc call
    pub fn keep_alive(mut self) -> Self {
        self.keep_alive = true;

        self
    }

    /*
     * runs when the app is launched again while it is running, with that launch's
     * arguments, like a file to open; the second launch hands them over and exits, so
     * only one copy runs. Without it, the app's window is brought to the front, opened
     * again if it was closed
     */
    pub fn on_activate(mut self, handler: fn(&[String])) -> Self {
        self.handlers.on_activate = Some(handler);

        self
    }

    /// Runs without an IPC socket, allowing a standalone app alongside the shell.
    /// Every launch then runs on its own, without `on_activate`.
    pub fn without_ipc(mut self) -> Self {
        self.ipc_disabled = true;

        self
    }

    pub fn run(self) {
        let no_windows = self.windows.is_empty() && self.normal_windows.is_empty();

        if no_windows && self.per_monitor.is_empty() && self.lock.is_none() {
            panic!("failed to run: no window set");
        }

        let app_id = self.app_id.unwrap_or_else(|| String::from("amane"));

        // a copy already running takes this launch over, so only one runs
        if !self.ipc_disabled && hand_over(&app_id) {
            return;
        }

        #[cfg(target_env = "gnu")]
        allocator::limit();

        if let Some(family) = &self.font {
            font::set_default(family);
        }

        // Created first and dropped last, including when backend startup unwinds.
        let _gpu = GpuSession;

        let mut backend = WaylandApp::new(
            self.windows,
            self.normal_windows,
            self.popups,
            self.per_monitor,
            self.lock,
            if self.ipc_disabled {
                None
            } else {
                Some(self.handlers)
            },
            self.scale_factor,
            app_id,
            self.keep_alive,
        );

        backend.run();
    }
}

/*
 * tells the copy of this app that is running about this launch, with its arguments and
 * the token that lets it take the focus; false when none is running
 */
fn hand_over(app_id: &str) -> bool {
    if UnixStream::connect(ipc_socket_for(app_id)).is_err() {
        return false;
    }

    let token = env::var("XDG_ACTIVATION_TOKEN").unwrap_or_default();

    // one argument a line, so a newline inside one becomes a space
    let mut arguments = vec![token];

    arguments.extend(
        env::args()
            .skip(1)
            .map(|argument| argument.replace('\n', " ")),
    );

    if ipc::call(app_id, ACTIVATE, &arguments).is_err() {
        return false;
    }

    eprintln!("amane: {app_id} is already running, it takes this launch");

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_scale_factors_before_connecting() {
        for factor in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(std::panic::catch_unwind(|| App::new().scale_factor(factor)).is_err());
        }
    }
}
