mod hyprland;
mod mango;
mod niri;
mod sway;

use std::collections::HashMap;
use std::env;

use crate::{WindowInfo, Workspace};

enum Compositor {
    Niri,
    Hyprland,
    Sway,
    Mango,
}

/// A control request sent to the running compositor. niri implements every
/// variant; the other backends implement what maps cleanly and ignore the
/// rest (see each backend's `act`).
pub(crate) enum Action {
    FocusWindow(u64),
    CloseWindow(u64),
    FullscreenWindow(u64),
    ToggleFloating(u64),
    // focus: whether the focus follows the window there
    MoveWindowToWorkspace {
        window: u64,
        workspace: i64,
        focus: bool,
    },
    // the window's column to `index` on its workspace, from 1, the focus staying put
    MoveColumnToIndex {
        window: u64,
        index: usize,
    },
    FocusColumnLeft,
    FocusColumnRight,
    Spawn(Vec<String>),
    Screenshot,
    ToggleOverview,
}

/// Live, per-frame gesture values from the compositor: overview open/close
/// progress, the focused output's horizontal view offset, and its vertical
/// (fractional) workspace-switch progress. Only niri reports these (via the
/// gesture-IPC fork); other backends never call `on_change`, so the snapshot
/// stays at its default.
#[derive(Debug, Clone, Default)]
pub(crate) struct Gesture {
    /// Overview progress: 0.0 closed .. 1.0 fully open.
    pub overview_progress: f64,
    /// Horizontal scroll position of the focused output, in logical pixels.
    pub view_pos: f64,
    /// Index of the active column on the focused output.
    pub active_column_idx: usize,
    /// Current, possibly fractional, workspace position of the focused output.
    pub workspace_idx: f64,
    /// Index of the active workspace on the focused output.
    pub active_workspace_idx: usize,
    /// Name of the focused output the values above belong to.
    pub output: String,
    /// Each output's desktop obscured: nothing under the workspaces shows there.
    pub obscured: HashMap<String, bool>,
}

// each compositor sets its own variable for the programs it starts
fn running() -> Option<Compositor> {
    if env::var_os("NIRI_SOCKET").is_some() {
        return Some(Compositor::Niri);
    }

    if env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
        return Some(Compositor::Hyprland);
    }

    if env::var_os("SWAYSOCK").is_some() {
        return Some(Compositor::Sway);
    }

    if env::var_os("MANGO_INSTANCE_SIGNATURE").is_some() {
        return Some(Compositor::Mango);
    }

    None
}

/*
 * hands over the full workspace list after every compositor event,
 * until the compositor exits; returns right away on an unsupported one
 */
pub fn listen(on_change: impl FnMut(Vec<Workspace>)) {
    let Some(compositor) = running() else {
        return;
    };

    match compositor {
        Compositor::Niri => niri::listen(on_change),
        Compositor::Hyprland => hyprland::listen(on_change),
        Compositor::Sway => sway::listen(on_change),
        Compositor::Mango => mango::listen(on_change),
    }
}

pub fn focus_workspace(id: i64) {
    let Some(compositor) = running() else {
        return;
    };

    match compositor {
        Compositor::Niri => niri::focus_workspace(id),
        Compositor::Hyprland => hyprland::focus_workspace(id),
        Compositor::Sway => sway::focus_workspace(id),
        Compositor::Mango => mango::focus_workspace(id),
    }
}

/*
 * hands over the full window list after every compositor event, like
 * `listen` but for windows; returns right away on an unsupported one
 */
pub(crate) fn listen_windows(on_change: impl FnMut(Vec<WindowInfo>, bool)) {
    let Some(compositor) = running() else {
        return;
    };

    match compositor {
        Compositor::Niri => niri::listen_windows(on_change),
        Compositor::Hyprland => hyprland::listen_windows(on_change),
        Compositor::Sway => sway::listen_windows(on_change),
        Compositor::Mango => mango::listen_windows(on_change),
    }
}

/*
 * hands over the live gesture snapshot after every gesture event, like
 * `listen_windows` but for overview/view/workspace progress. Only niri
 * reports these; on any other compositor this returns right away, so the
 * Gestures service simply stays at its default.
 */
pub(crate) fn listen_gestures(on_change: impl FnMut(Gesture)) {
    let Some(compositor) = running() else {
        return;
    };

    match compositor {
        Compositor::Niri => niri::listen_gestures(on_change),
        Compositor::Hyprland | Compositor::Sway | Compositor::Mango => {}
    }
}

pub(crate) fn act(action: Action) {
    let Some(compositor) = running() else {
        return;
    };

    match compositor {
        Compositor::Niri => niri::act(action),
        Compositor::Hyprland => hyprland::act(action),
        Compositor::Sway => sway::act(action),
        Compositor::Mango => mango::act(action),
    }
}
