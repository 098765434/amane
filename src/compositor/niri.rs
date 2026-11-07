use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

use serde_json::{Value, json};

use super::{Action, Gesture};
use crate::{WindowInfo, Workspace};

// the niri events amane uses, everything else niri sends is skipped
pub enum Event {
    // the full list, sent first and again whenever one is added or removed
    Workspaces(Vec<Workspace>),

    // focused is false when it only became the one shown on its monitor
    Activated { id: i64, focused: bool },

    Urgent { id: i64, urgent: bool },

    // every window, sent first and whenever the set changes
    Windows(Vec<WindowInfo>),

    // a window opened, or any of its fields changed (title, workspace, …)
    WindowChanged(WindowInfo),

    WindowClosed { id: u64 },

    // focus moved; id is None when no window is focused (e.g. a layer
    // surface took it). All other windows are no longer focused.
    WindowFocus { id: Option<u64> },

    // niri's overview was opened or closed
    Overview { open: bool },

    // overview open/close animation progress: 0.0 closed .. 1.0 fully open
    OverviewProgress { progress: f64 },

    // horizontal scroll position of the focused output's active workspace
    ViewOffset {
        output: String,
        view_pos: f64,
        active_column_idx: usize,
    },

    // vertical (fractional) workspace position of the focused output
    WorkspaceSwitch {
        output: String,
        workspace_idx: f64,
        active_workspace_idx: usize,
    },
}

// niri's events only say what changed, so the rest is remembered here
#[derive(Default)]
struct State {
    list: Vec<Workspace>,

    // every window by its id, and the workspace it is on
    windows: HashMap<u64, Option<i64>>,
}

/*
 * niri reads one json request per line, then answers on the same connection;
 * none when niri is not running, like under another compositor
 */
pub fn send(request: &str) -> Option<UnixStream> {
    let path = env::var("NIRI_SOCKET").ok()?;

    let mut stream = UnixStream::connect(path).ok()?;

    let line = format!("{request}\n");

    stream.write_all(line.as_bytes()).ok()?;

    Some(stream)
}

pub fn focus_workspace(id: i64) {
    let request = json!({
        "Action": {
            "FocusWorkspace": {
                "reference": { "Id": id }
            }
        }
    });

    let Some(stream) = send(&request.to_string()) else {
        return;
    };

    // waiting for the one-line answer keeps niri from seeing a connection that closed mid-request
    let mut reply = String::new();

    let _ = BufReader::new(stream).read_line(&mut reply);
}

// follows niri's events until niri exits, handing over the full list after each one
pub fn listen(mut on_change: impl FnMut(Vec<Workspace>)) {
    let Some(stream) = send("\"EventStream\"") else {
        return;
    };

    let mut state = State::default();

    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else {
            return;
        };

        let Some(event) = parse(&line) else {
            continue;
        };

        state.apply(event);

        on_change(state.list.clone());
    }
}

// follows the same event stream as `listen`, but hands over the full window
// list (and whether niri's overview is open) after each window event
pub fn listen_windows(mut on_change: impl FnMut(Vec<WindowInfo>, bool)) {
    let Some(stream) = send("\"EventStream\"") else {
        return;
    };

    let mut windows: HashMap<u64, WindowInfo> = HashMap::new();
    let mut overview = false;

    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else {
            return;
        };

        let Some(event) = parse(&line) else {
            continue;
        };

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
fn apply_window(windows: &mut HashMap<u64, WindowInfo>, overview: &mut bool, event: Event) -> bool {
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
// gesture event. Opens its own connection, like `listen`/`listen_windows`.
pub fn listen_gestures(mut on_change: impl FnMut(Gesture)) {
    let Some(stream) = send("\"EventStream\"") else {
        return;
    };

    let mut gesture = Gesture::default();

    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else {
            return;
        };

        let Some(event) = parse(&line) else {
            continue;
        };

        // only the three gesture events move anything here; every other event
        // on the shared stream is ignored
        if apply_gesture(&mut gesture, event) {
            on_change(gesture.clone());
        }
    }
}

// true when a gesture event updated the snapshot, so the caller should emit
fn apply_gesture(gesture: &mut Gesture, event: Event) -> bool {
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
        Action::MoveWindowToWorkspace { window, workspace } => json!({
            "Action": {
                "MoveWindowToWorkspace": {
                    "window_id": window,
                    "reference": { "Id": workspace },
                    "focus": true
                }
            }
        }),
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

fn parse(line: &str) -> Option<Event> {
    let value: Value = serde_json::from_str(line).ok()?;

    // every event is an object with one key, the event's name
    let (name, body) = value.as_object()?.iter().next()?;

    match name.as_str() {
        "WorkspacesChanged" => workspaces_changed(body),
        "WorkspaceActivated" => workspace_activated(body),
        "WorkspaceUrgencyChanged" => urgency_changed(body),
        "WindowsChanged" => windows_changed(body),
        "WindowOpenedOrChanged" => window_changed(body),
        "WindowClosed" => window_closed(body),
        "WindowFocusChanged" => window_focus(body),
        "OverviewOpenedOrClosed" => overview_changed(body),
        "OverviewProgress" => overview_progress(body),
        "ViewOffsetChanged" => view_offset(body),
        "WorkspaceSwitchProgress" => workspace_switch(body),
        _ => None,
    }
}

fn overview_changed(body: &Value) -> Option<Event> {
    Some(Event::Overview {
        open: body["is_open"].as_bool()?,
    })
}

// the three gesture events from the niri fork (live, per-frame values)

fn overview_progress(body: &Value) -> Option<Event> {
    Some(Event::OverviewProgress {
        progress: body["progress"].as_f64()?,
    })
}

fn view_offset(body: &Value) -> Option<Event> {
    Some(Event::ViewOffset {
        output: body["output"].as_str()?.to_string(),
        view_pos: body["view_pos"].as_f64()?,
        active_column_idx: body["active_column_idx"].as_u64()? as usize,
    })
}

fn workspace_switch(body: &Value) -> Option<Event> {
    Some(Event::WorkspaceSwitch {
        output: body["output"].as_str()?.to_string(),
        workspace_idx: body["workspace_idx"].as_f64()?,
        active_workspace_idx: body["active_workspace_idx"].as_u64()? as usize,
    })
}

fn windows_changed(body: &Value) -> Option<Event> {
    let mut windows = Vec::new();

    for window in body["windows"].as_array()? {
        windows.push(parse_window(window)?);
    }

    Some(Event::Windows(windows))
}

fn window_changed(body: &Value) -> Option<Event> {
    Some(Event::WindowChanged(parse_window(&body["window"])?))
}

fn window_closed(body: &Value) -> Option<Event> {
    let id = body["id"].as_u64()?;

    Some(Event::WindowClosed { id })
}

fn window_focus(body: &Value) -> Option<Event> {
    // null when no window is focused
    Some(Event::WindowFocus {
        id: body["id"].as_u64(),
    })
}

fn parse_window(window: &Value) -> Option<WindowInfo> {
    Some(WindowInfo {
        id: window["id"].as_u64()?,

        // null while the app has set neither
        app_id: window["app_id"].as_str().map(String::from),
        title: window["title"].as_str().map(String::from),

        focused: window["is_focused"].as_bool().unwrap_or(false),
        floating: window["is_floating"].as_bool().unwrap_or(false),

        // null while the window is on no workspace
        workspace: window["workspace_id"].as_i64(),
    })
}

fn workspaces_changed(body: &Value) -> Option<Event> {
    let mut list = Vec::new();

    for value in body["workspaces"].as_array()? {
        list.push(parse_workspace(value)?);
    }

    Some(Event::Workspaces(list))
}

fn workspace_activated(body: &Value) -> Option<Event> {
    let id = body["id"].as_i64()?;

    let focused = body["focused"].as_bool()?;

    Some(Event::Activated { id, focused })
}

fn urgency_changed(body: &Value) -> Option<Event> {
    let id = body["id"].as_i64()?;

    let urgent = body["urgent"].as_bool()?;

    Some(Event::Urgent { id, urgent })
}

fn parse_workspace(value: &Value) -> Option<Workspace> {
    let index = value["idx"].as_u64()?;

    let workspace = Workspace {
        id: value["id"].as_i64()?,
        index: index as u32,

        // null for workspaces the user never named
        name: value["name"].as_str().map(String::from),
        output: value["output"].as_str().map(String::from),

        active: value["is_active"].as_bool()?,
        focused: value["is_focused"].as_bool()?,
        urgent: value["is_urgent"].as_bool()?,

        // counted from the window events, after every event
        windows: 0,
    };

    Some(workspace)
}

impl State {
    fn apply(&mut self, event: Event) {
        match event {
            Event::Workspaces(list) => self.list = list,
            Event::Activated { id, focused } => self.activate(id, focused),
            Event::Urgent { id, urgent } => self.mark_urgent(id, urgent),

            Event::Windows(windows) => {
                self.windows = windows.into_iter().map(|w| (w.id, w.workspace)).collect();
            }

            Event::WindowChanged(window) => {
                self.windows.insert(window.id, window.workspace);
            }

            Event::WindowClosed { id } => {
                self.windows.remove(&id);
            }

            // neither focus, overview, nor the live gesture values move a
            // window between workspaces, so the counts this state tracks are
            // unaffected
            Event::WindowFocus { .. }
            | Event::Overview { .. }
            | Event::OverviewProgress { .. }
            | Event::ViewOffset { .. }
            | Event::WorkspaceSwitch { .. } => {}
        }

        self.count_windows();
    }

    fn activate(&mut self, id: i64, focused: bool) {
        let Some(activated) = self.list.iter().find(|workspace| workspace.id == id) else {
            return;
        };

        let output = activated.output.clone();

        // only one workspace is shown per monitor, and only one has focus overall
        for workspace in &mut self.list {
            if workspace.output == output {
                workspace.active = workspace.id == id;
            }

            if focused {
                workspace.focused = workspace.id == id;
            }
        }
    }

    fn mark_urgent(&mut self, id: i64, urgent: bool) {
        for workspace in &mut self.list {
            if workspace.id == id {
                workspace.urgent = urgent;
            }
        }
    }

    // a new workspace list starts at 0 windows each, so the counts are made again after any event
    fn count_windows(&mut self) {
        for workspace in &mut self.list {
            let mut count = 0;

            for on in self.windows.values() {
                if *on == Some(workspace.id) {
                    count += 1;
                }
            }

            workspace.windows = count;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // lines copied from niri's event stream, trimmed to the fields amane reads plus a few it skips
    #[test]
    fn reads_niri_event_lines() {
        let line = r#"{"WorkspacesChanged":{"workspaces":[{"id":3,"idx":1,"name":null,"output":"eDP-1","is_urgent":false,"is_active":true,"is_focused":true,"active_window_id":null}]}}"#;

        let Some(Event::Workspaces(list)) = parse(line) else {
            panic!("failed to parse workspaces");
        };

        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, 3);
        assert_eq!(list[0].index, 1);
        assert_eq!(list[0].name, None);
        assert!(list[0].focused);

        let line = r#"{"WindowOpenedOrChanged":{"window":{"id":7,"title":"foot","app_id":"foot","workspace_id":null,"is_focused":true,"is_floating":false}}}"#;

        let Some(Event::WindowChanged(window)) = parse(line) else {
            panic!("failed to parse window");
        };

        assert_eq!(window.id, 7);
        assert_eq!(window.title.as_deref(), Some("foot"));
        assert_eq!(window.app_id.as_deref(), Some("foot"));
        assert!(window.focused);
        assert!(!window.floating);
        assert_eq!(window.workspace, None);

        // focus changes arrive as their own event, with the newly focused id
        assert!(matches!(
            parse(r#"{"WindowFocusChanged":{"id":7}}"#),
            Some(Event::WindowFocus { id: Some(7) })
        ));

        // null id means no window is focused
        assert!(matches!(
            parse(r#"{"WindowFocusChanged":{"id":null}}"#),
            Some(Event::WindowFocus { id: None })
        ));

        // overview open/close drives the taskbar's expanded mode
        assert!(matches!(
            parse(r#"{"OverviewOpenedOrClosed":{"is_open":true}}"#),
            Some(Event::Overview { open: true })
        ));

        // events amane doesn't use are skipped
        assert!(
            parse(
                r#"{"KeyboardLayoutsChanged":{"keyboard_layouts":{"names":[],"current_idx":0}}}"#
            )
            .is_none()
        );
    }
}
