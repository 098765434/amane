use crate::compositor::{self, Action};
use crate::services::worker;
use crate::{Service, WindowInfo};

pub struct Windows {
    list: Vec<WindowInfo>,
    overview: bool,
}

impl Service for Windows {
    fn new() -> Self {
        Self {
            list: Vec::new(),
            overview: false,
        }
    }

    // on an unsupported compositor the list stays empty
    fn listen() {
        compositor::listen_windows(|mut list, overview| {
            // compositors send them in no particular order; a stable sort by
            // id keeps diffs (and any id-keyed UI) from jumping around
            list.sort_by_key(WindowInfo::id);

            let mut windows = Self::write();

            // a title or overview change is a real change; but an event that
            // touched nothing this service shows (e.g. a workspace event on
            // the shared stream) leaves both equal, so redraw nothing
            if windows.list == list && windows.overview == overview {
                windows.quiet();

                return;
            }

            windows.list = list;
            windows.overview = overview;
        });
    }
}

impl Windows {
    // sorted by id
    pub fn list(&self) -> &[WindowInfo] {
        &self.list
    }

    // the one focused window, or none (e.g. a layer surface has focus)
    pub fn focused(&self) -> Option<&WindowInfo> {
        self.list.iter().find(|window| window.focused)
    }

    // whether niri's overview is currently open (always false on backends
    // without overview support)
    pub fn overview(&self) -> bool {
        self.overview
    }

    // ───── control (feature: typed compositor actions) ──────────────────────
    // Each runs on the shared worker thread so a slow socket never stalls
    // drawing, mirroring `Workspaces::focus`.

    pub fn focus(id: u64) {
        worker::run(move || compositor::act(Action::FocusWindow(id)));
    }

    pub fn close(id: u64) {
        worker::run(move || compositor::act(Action::CloseWindow(id)));
    }

    pub fn fullscreen(id: u64) {
        worker::run(move || compositor::act(Action::FullscreenWindow(id)));
    }

    pub fn toggle_floating(id: u64) {
        worker::run(move || compositor::act(Action::ToggleFloating(id)));
    }

    pub fn move_to_workspace(id: u64, workspace: i64) {
        worker::run(move || {
            compositor::act(Action::MoveWindowToWorkspace {
                window: id,
                workspace,
                focus: true,
            })
        });
    }

    // moves the window to `workspace` (none: stays on its own) and its column to
    // `index` there, from 1, without the focus going with it, as a drag in a dock does
    pub fn move_to(id: u64, workspace: Option<i64>, index: usize) {
        worker::run(move || {
            if let Some(workspace) = workspace {
                compositor::act(Action::MoveWindowToWorkspace {
                    window: id,
                    workspace,
                    focus: false,
                });
            }

            compositor::act(Action::MoveColumnToIndex { window: id, index });
        });
    }

    pub fn focus_column_left() {
        worker::run(|| compositor::act(Action::FocusColumnLeft));
    }

    pub fn focus_column_right() {
        worker::run(|| compositor::act(Action::FocusColumnRight));
    }

    // Ask the compositor to spawn a command. Unlike `amane::spawn`, which the
    // shell runs itself, this hands the command to the compositor, so it is
    // parented and tracked like any binding-launched program.
    pub fn spawn(command: Vec<String>) {
        worker::run(move || compositor::act(Action::Spawn(command)));
    }

    pub fn screenshot() {
        worker::run(|| compositor::act(Action::Screenshot));
    }

    pub fn toggle_overview() {
        worker::run(|| compositor::act(Action::ToggleOverview));
    }
}
