use std::mem;
use std::num::NonZeroU32;
use std::sync::PoisonError;

use smithay_client_toolkit::globals::ProvidesBoundGlobal;
use smithay_client_toolkit::reexports::protocols::xdg::dialog::v1::client::{
    xdg_dialog_v1::{self, XdgDialogV1},
    xdg_wm_dialog_v1::XdgWmDialogV1,
};
use smithay_client_toolkit::shell::{
    WaylandSurface,
    xdg::window::{
        DecorationMode, Window as XdgWindow, WindowConfigure, WindowDecorations, WindowHandler,
    },
};
use wayland_client::{Connection, Dispatch, QueueHandle};

use crate::frame;
use crate::scale::ScaleFactor;
use crate::window::{REQUESTS, Request, WindowView};
use crate::{Window, WindowState};

use super::{
    WaylandState,
    surface::{OpenWindow, Role, View},
};

// the settings of a normal window the compositor is told about, compared to send only changes
#[derive(Clone, PartialEq)]
pub struct NormalSettings {
    title: String,

    // in surface pixels, like the sizes the compositor sends back
    min_size: Option<(u32, u32)>,
    max_size: Option<(u32, u32)>,

    maximized: Option<bool>,
    fullscreen: Option<bool>,
}

impl NormalSettings {
    fn new(window: &Window, scale_factor: ScaleFactor) -> Self {
        let pixels =
            |(width, height): (f32, f32)| (scale_factor.pixels(width), scale_factor.pixels(height));

        // the same smallest and biggest size is how a window says it cannot be resized
        let (min_size, max_size) = if window.resizable {
            (window.min_size.map(pixels), window.max_size.map(pixels))
        } else {
            let size = pixels((window.width, window.height));

            (Some(size), Some(size))
        };

        Self {
            title: window.title.clone(),

            min_size,
            max_size,

            maximized: window.maximized,
            fullscreen: window.fullscreen,
        }
    }

    // sends every field that differs from what was sent before
    fn send(&self, before: Option<&Self>, window: &XdgWindow) {
        if before.is_none_or(|before| before.title != self.title) {
            window.set_title(self.title.clone());
        }

        if before.is_none_or(|before| before.min_size != self.min_size) {
            window.set_min_size(self.min_size);
        }

        if before.is_none_or(|before| before.max_size != self.max_size) {
            window.set_max_size(self.max_size);
        }

        // asking for what a new window already is would only be noise
        let opening = before.is_none();

        if before.is_none_or(|before| before.maximized != self.maximized) {
            match self.maximized {
                Some(true) => window.set_maximized(),
                Some(false) if !opening => window.unset_maximized(),
                _ => {}
            }
        }

        if before.is_none_or(|before| before.fullscreen != self.fullscreen) {
            match self.fullscreen {
                Some(true) => window.set_fullscreen(None),
                Some(false) if !opening => window.unset_fullscreen(),
                _ => {}
            }
        }
    }
}

// what makes a window a dialog over its parent, gone with the window
pub struct Dialog(XdgDialogV1);

impl Drop for Dialog {
    fn drop(&mut self) {
        self.0.destroy();
    }
}

impl WaylandState {
    pub fn open_normal(&mut self, key: String, view: WindowView) {
        let surface = self.compositor.create_surface(&self.qh);

        // the view runs as it will once open, with its name and nothing known about it yet
        let (window, _) = frame::run_view(
            || {
                crate::window::set_state(WindowState::default());

                View::Normal(key.clone(), view.clone()).run()
            },
            0,
            0,
            self.scale_factor,
        );

        let crate::wayland::surface::Content::Normal(window) = window else {
            unreachable!("a normal view gives a normal window");
        };

        // the compositor draws the title bar when it can, amane has none of its own
        let xdg_window =
            self.xdg_shell
                .create_window(surface, WindowDecorations::RequestServer, &self.qh);

        xdg_window.set_app_id(window.app_id.clone().unwrap_or_else(|| self.app_id.clone()));

        let settings = NormalSettings::new(&window, self.scale_factor);

        settings.send(None, &xdg_window);

        let mut dialog = None;

        if let Some(parent) = window.parent.as_deref().and_then(|key| self.normal(key)) {
            xdg_window.set_parent(Some(&parent));

            // the dialog protocol is only needed to make it modal, and is optional
            let manager: Option<XdgWmDialogV1> = self.xdg_shell.bound_global().ok();

            if let (true, Some(manager)) = (window.modal, manager) {
                let xdg_dialog = manager.get_xdg_dialog(xdg_window.xdg_toplevel(), &self.qh, ());

                xdg_dialog.set_modal();

                dialog = Some(Dialog(xdg_dialog));
            }
        }

        // the compositor answers the first commit with a configure, drawing starts there
        xdg_window.commit();

        let size = (
            self.scale_factor.pixels(window.width),
            self.scale_factor.pixels(window.height),
        );

        let role = Role::Normal {
            dialog,
            window: xdg_window,
            size,
            settings,
            state: WindowState::default(),
        };

        self.add(View::Normal(key, view), None, role);
    }

    // the open normal window with this name
    fn normal(&self, key: &str) -> Option<XdgWindow> {
        self.windows.iter().find_map(|window| match &window.role {
            Role::Normal { window: xdg, .. } if window.view.shows(key) => Some(xdg.clone()),
            _ => None,
        })
    }

    // what open_window, close_window and minimize_window asked for, in the order they asked
    pub fn handle_window_requests(&mut self) {
        let mut queue = REQUESTS.lock().unwrap_or_else(PoisonError::into_inner);

        let requests = mem::take(&mut *queue);

        drop(queue);

        for request in requests {
            match request {
                Request::Open(key, view) => {
                    // a name that is already open stays as it is
                    if self.normal(&key).is_none() {
                        self.open_normal(key, view);
                    }
                }

                Request::Close(key) => {
                    let mut surfaces = Vec::new();

                    for window in &self.windows {
                        if window.view.shows(&key) {
                            surfaces.push(window.role.wl_surface().clone());
                        }
                    }

                    for surface in surfaces {
                        self.close(&surface);
                    }
                }

                Request::Minimize(key) => {
                    if let Some(window) = self.normal(&key) {
                        window.set_minimized();
                    }
                }
            }
        }
    }
}

impl OpenWindow {
    // sends what changed since the last frame, the way update_surface does for layers
    pub fn update_normal(&mut self, window: &Window) {
        let Role::Normal {
            window: xdg_window,
            settings: current,
            ..
        } = &mut self.role
        else {
            return;
        };

        let settings = NormalSettings::new(window, self.scale_factor);

        if settings == *current {
            return;
        }

        // these go out with the next commit, which the frame after this makes
        settings.send(Some(current), xdg_window);

        *current = settings;
    }
}

impl WindowHandler for WaylandState {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, xdg_window: &XdgWindow) {
        let surface = xdg_window.wl_surface();

        let on_close = self
            .window(surface)
            .and_then(|window| window.on_close.clone());

        // the handler can keep it open, and may have changed what it shows
        if on_close.is_some_and(|on_close| !on_close()) {
            self.request_frame_on(surface);

            return;
        }

        self.close(surface);
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        xdg_window: &XdgWindow,
        configure: WindowConfigure,
        _: u32,
    ) {
        let Some(window) = self.window(xdg_window.wl_surface()) else {
            return;
        };

        // none means the compositor leaves the size to the window, like 0 does for layers
        let (width, height) = configure.new_size;

        let width = width.map_or(0, NonZeroU32::get);
        let height = height.map_or(0, NonZeroU32::get);

        if let Role::Normal { size, state, .. } = &mut window.role {
            *state = WindowState {
                focused: configure.is_activated(),
                maximized: configure.is_maximized(),
                fullscreen: configure.is_fullscreen(),
                tiled: configure.is_tiled(),
                resizing: configure.is_resizing(),
                server_decorated: configure.decoration_mode == DecorationMode::Server,
            };

            // a later configure that leaves the size to the window keeps the one the user gave it
            if width > 0 && height > 0 {
                *size = (width, height);
            }
        }

        window.resize(width, height);
    }
}

// asking for modal is all amane does with a dialog, which sends nothing back
impl Dispatch<XdgDialogV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &XdgDialogV1,
        _: xdg_dialog_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
