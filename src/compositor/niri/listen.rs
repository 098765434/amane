//! Following niri's event stream for windows, gestures, keyboard layouts, lock keys, outputs
//! and casts, and sending it actions.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};

use serde_json::json;

use super::super::{Action, Gesture};
use super::*;
use crate::WindowInfo;

// follows the same event stream as `listen` (one connection, shared: stream.rs),
// but hands over the full window list (and whether niri's overview is open) after each window event
pub fn listen_windows(mut on_change: impl FnMut(Vec<WindowInfo>, bool)) {
    let mut windows: HashMap<u64, WindowInfo> = HashMap::new();
    let mut overview = false;

    for event in stream::subscribe() {
        // only emit on events the bar reads (windows or overview); plain
        // workspace events on the same stream change nothing here
        if apply_window(&mut windows, &mut overview, event) {
            let mut list: Vec<WindowInfo> = windows.values().cloned().collect();

            list.sort_by_key(|window| window.id);

            on_change(list, overview);
        }
    }
}

// true when the event changed something the caller shows, so it should emit
pub(super) fn apply_window(
    windows: &mut HashMap<u64, WindowInfo>,
    overview: &mut bool,
    event: Event,
) -> bool {
    match event {
        Event::Overview { open } => {
            *overview = open;
            true
        }

        Event::Windows(list) => {
            *windows = list.into_iter().map(|w| (w.id, w)).collect();
            true
        }

        Event::WindowChanged(window) => {
            // niri sends WindowFocusChanged for focus moves, but a change
            // event can also arrive already focused; keep the "one focused"
            // invariant either way
            if window.focused {
                for other in windows.values_mut() {
                    other.focused = false;
                }
            }

            windows.insert(window.id, window);
            true
        }

        Event::WindowClosed { id } => {
            windows.remove(&id);
            true
        }

        // sizes change on every frame of a resize, so only a move counts
        Event::WindowLayouts(changes) => {
            let mut moved = false;

            for (id, column) in changes {
                if let Some(window) = windows.get_mut(&id)
                    && window.column != column
                {
                    window.column = column;
                    moved = true;
                }
            }

            moved
        }

        Event::WindowFocus { id } => {
            for (window_id, window) in windows.iter_mut() {
                window.focused = Some(*window_id) == id;
            }
            true
        }

        _ => false,
    }
}

// follows the same event stream, but hands over the live gesture values
// (overview progress, view offset, workspace-switch progress) after each
// gesture event
pub fn listen_gestures(mut on_change: impl FnMut(Gesture)) {
    let mut gesture = Gesture::default();

    for event in stream::subscribe() {
        // only the gesture events (and DesktopObscured) move anything here; every
        // other event on the shared stream is ignored
        if apply_gesture(&mut gesture, event) {
            on_change(gesture.clone());
        }
    }
}

// follows the same event stream for the keyboard layouts and the one in use
pub fn listen_keyboard_layouts(mut on_change: impl FnMut(Vec<String>, u8)) {
    let mut names = Vec::new();

    for event in stream::subscribe() {
        match event {
            Event::KeyboardLayouts {
                names: list,
                current,
            } => {
                names = list;
                on_change(names.clone(), current);
            }
            Event::KeyboardLayoutSwitched { current } => on_change(names.clone(), current),
            _ => {}
        }
    }
}

// follows the same event stream for Caps Lock and Num Lock (the niri fork)
pub fn listen_lock_keys(mut on_change: impl FnMut(bool, bool)) {
    for event in stream::subscribe() {
        if let Event::LockKeys {
            caps_lock,
            num_lock,
        } = event
        {
            on_change(caps_lock, num_lock);
        }
    }
}

// calls back after each OutputsChanged (the niri fork)
pub fn listen_outputs_changed(mut on_change: impl FnMut()) {
    for event in stream::subscribe() {
        if matches!(event, Event::OutputsChanged) {
            on_change();
        }
    }
}

// hands over every running screencast after each cast event
pub fn listen_casts(mut on_change: impl FnMut(Vec<client::Cast>)) {
    let mut casts: Vec<client::Cast> = Vec::new();

    for event in stream::subscribe() {
        if apply_cast(&mut casts, event) {
            on_change(casts.clone());
        }
    }
}

pub(super) fn apply_cast(casts: &mut Vec<client::Cast>, event: Event) -> bool {
    match event {
        Event::Casts(list) => *casts = list,
        Event::CastChanged(cast) => {
            match casts.iter_mut().find(|c| c.stream_id == cast.stream_id) {
                Some(known) => *known = cast,
                None => casts.push(cast),
            }
            casts.sort_by_key(|cast| cast.stream_id);
        }
        Event::CastStopped { stream_id } => casts.retain(|cast| cast.stream_id != stream_id),
        _ => return false,
    }
    true
}

// true when a gesture event updated the snapshot, so the caller should emit
pub(super) fn apply_gesture(gesture: &mut Gesture, event: Event) -> bool {
    match event {
        Event::OverviewProgress { progress } => {
            gesture.overview_progress = progress;
            true
        }

        Event::ViewOffset {
            output,
            view_pos,
            active_column_idx,
        } => {
            gesture.output = output;
            gesture.view_pos = view_pos;
            gesture.active_column_idx = active_column_idx;
            true
        }

        Event::WorkspaceSwitch {
            output,
            workspace_idx,
            active_workspace_idx,
        } => {
            gesture.output = output;
            gesture.workspace_idx = workspace_idx;
            gesture.active_workspace_idx = active_workspace_idx;
            true
        }

        Event::DesktopObscured { output, obscured } => {
            gesture.obscured.insert(output, obscured);
            true
        }

        _ => false,
    }
}

// sends a control request and drains the one-line reply, so niri never sees a
// connection that closed mid-request (as `focus_workspace` does)
pub fn act(action: Action) {
    let request = match action {
        Action::FocusWindow(id) => json!({ "Action": { "FocusWindow": { "id": id } } }),
        Action::CloseWindow(id) => json!({ "Action": { "CloseWindow": { "id": id } } }),
        Action::FullscreenWindow(id) => {
            json!({ "Action": { "FullscreenWindow": { "id": id } } })
        }
        Action::ToggleFloating(id) => {
            json!({ "Action": { "ToggleWindowFloating": { "id": id } } })
        }
        Action::MoveWindowToWorkspace {
            window,
            workspace,
            focus,
        } => json!({
            "Action": {
                "MoveWindowToWorkspace": {
                    "window_id": window,
                    "reference": { "Id": workspace },
                    "focus": focus
                }
            }
        }),
        // the window_id from the niri fork; stock niri moves the focused column, so the
        // window is focused first there
        Action::MoveColumnToIndex { window, index } => {
            if !client::features().has("move-column-to-index-window-id") {
                act(Action::FocusWindow(window));

                json!({ "Action": { "MoveColumnToIndex": { "index": index } } })
            } else {
                json!({
                    "Action": { "MoveColumnToIndex": { "window_id": window, "index": index } }
                })
            }
        }
        Action::FocusColumnLeft => json!({ "Action": { "FocusColumnLeft": {} } }),
        Action::FocusColumnRight => json!({ "Action": { "FocusColumnRight": {} } }),
        Action::Spawn(command) => json!({ "Action": { "Spawn": { "command": command } } }),
        Action::Screenshot => json!({ "Action": { "Screenshot": { "show_pointer": true } } }),
        Action::ToggleOverview => json!({ "Action": { "ToggleOverview": {} } }),
    };

    let Some(stream) = send(&request.to_string()) else {
        return;
    };

    let mut reply = String::new();

    let _ = BufReader::new(stream).read_line(&mut reply);
}
