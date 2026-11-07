//! The niri events the fork reads beyond workspaces and windows: layouts, focus, the overview,
//! gestures, the desktop, keyboard layouts.

use serde_json::Value;

use super::*;
use crate::WindowInfo;

pub(super) fn overview_changed(body: &Value) -> Option<Event> {
    Some(Event::Overview {
        open: body["is_open"].as_bool()?,
    })
}

// the three gesture events from the niri fork (live, per-frame values)

pub(super) fn overview_progress(body: &Value) -> Option<Event> {
    Some(Event::OverviewProgress {
        progress: body["progress"].as_f64()?,
    })
}

pub(super) fn view_offset(body: &Value) -> Option<Event> {
    Some(Event::ViewOffset {
        output: body["output"].as_str()?.to_string(),
        view_pos: body["view_pos"].as_f64()?,
        active_column_idx: body["active_column_idx"].as_u64()? as usize,
    })
}

pub(super) fn workspace_switch(body: &Value) -> Option<Event> {
    Some(Event::WorkspaceSwitch {
        output: body["output"].as_str()?.to_string(),
        workspace_idx: body["workspace_idx"].as_f64()?,
        active_workspace_idx: body["active_workspace_idx"].as_u64()? as usize,
    })
}

pub(super) fn desktop_obscured(body: &Value) -> Option<Event> {
    Some(Event::DesktopObscured {
        output: body["output"].as_str()?.to_string(),
        obscured: body["obscured"].as_bool()?,
    })
}

pub(super) fn keyboard_layouts(body: &Value) -> Option<Event> {
    let layouts = &body["keyboard_layouts"];
    let names = layouts["names"]
        .as_array()?
        .iter()
        .filter_map(|name| name.as_str().map(String::from))
        .collect();
    let current = u8::try_from(layouts["current_idx"].as_u64()?).ok()?;

    Some(Event::KeyboardLayouts { names, current })
}

pub(super) fn keyboard_switched(body: &Value) -> Option<Event> {
    let current = u8::try_from(body["idx"].as_u64()?).ok()?;

    Some(Event::KeyboardLayoutSwitched { current })
}

pub(super) fn window_layouts(body: &Value) -> Option<Event> {
    let mut changes = Vec::new();

    // each change is an [id, layout] pair
    for change in body["changes"].as_array()? {
        changes.push((change[0].as_u64()?, column(&change[1])));
    }

    Some(Event::WindowLayouts(changes))
}

// a window layout's [column, tile] pair, null while floating
pub(super) fn column(layout: &Value) -> Option<(usize, usize)> {
    let pos = &layout["pos_in_scrolling_layout"];

    Some((pos[0].as_u64()? as usize, pos[1].as_u64()? as usize))
}

pub(super) fn window_focus(body: &Value) -> Option<Event> {
    // null when no window is focused
    Some(Event::WindowFocus {
        id: body["id"].as_u64(),
    })
}

pub(super) fn parse_window(window: &Value) -> Option<WindowInfo> {
    Some(WindowInfo {
        id: window["id"].as_u64()?,

        // null while the app has set neither
        app_id: window["app_id"].as_str().map(String::from),
        title: window["title"].as_str().map(String::from),

        focused: window["is_focused"].as_bool().unwrap_or(false),
        floating: window["is_floating"].as_bool().unwrap_or(false),
        fullscreen: window["is_fullscreen"].as_bool().unwrap_or(false),

        // null while the window is on no workspace
        workspace: window["workspace_id"].as_i64(),

        // older niri sends no layout
        column: column(&window["layout"]),
    })
}
