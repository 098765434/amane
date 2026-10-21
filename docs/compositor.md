# Compositor integration

Amane talks to the running Wayland compositor over its native IPC to
surface live **workspaces** and **windows**, and to send **control
actions** back. The backend is picked at runtime from the environment
([src/compositor.rs](../src/compositor.rs)); niri is the fully implemented
one, with partial support for hyprland, sway, and mango.

Everything here is exposed as a `Service` (read with `::read()` inside a
view, which subscribes the surface to changes) plus static control methods,
matching the rest of Amane's service layer.

## Services

### `Workspaces`

The ordered workspace list (upstream feature). `Workspaces::read().list()`
gives `&[Workspace]`, sorted by monitor then position. Each `Workspace`
exposes `id`, `index`, `name`, `output`, `active`, `focused`, `urgent`,
and a `windows` count. Control: `Workspaces::focus(id)`.

### `Windows` (fork addition)

The open-window list and the focused window — niri has no server-side
titlebars, so this is the only place a window title or app-id is available
to shell chrome.

```rust
let windows = Windows::read();

if let Some(window) = windows.focused() {
    let title = window.title().unwrap_or("");
    let app = window.app_id().unwrap_or("");
}

for window in windows.list() { /* build a dock / switcher */ }
```

`WindowInfo` fields (all via getters): `id() -> u64`,
`app_id() -> Option<&str>`, `title() -> Option<&str>`,
`focused() -> bool`, `floating() -> bool`, `workspace() -> Option<i64>`.
The list is sorted by `id`, so id-keyed UI stays stable across updates.
`focused()` returns `None` when focus is on a non-window surface (e.g. a
layer-shell panel).

`Windows::read().overview()` reports whether niri's overview is currently
open — handy for a shell that wants to expand or restyle itself while the
overview is up. It tracks the `OverviewOpenedOrClosed` event and is always
`false` on backends without overview support.

> Named `WindowInfo`, not `Window`, because the crate root already exports
> [`Window`](../src/window.rs) for the xdg-toplevel *surface role* a shell
> opens itself.

### `Gestures` (fork addition, niri only)

Live, **per-frame** gesture values from niri, so a shell can animate in
lockstep with the compositor's own overview zoom, column scroll, and
workspace switch — rather than snapping on a discrete event. They require
the gesture-IPC niri fork; under stock niri (or another compositor) every
value stays at its default and the service simply never updates.

```rust
let g = Gestures::read();

let progress = g.overview_progress();     // 0.0 closed .. 1.0 fully open
let bar_height = compact + (tall - compact) * progress as f32;
```

Getters: `overview_progress() -> f64` (global, `0.0`..`1.0`),
`view_pos() -> f64` (focused output's horizontal scroll, logical px) with
`active_column_idx() -> usize`, and `workspace_idx() -> f64` (focused
output's fractional workspace position) with `active_workspace_idx() ->
usize`. `output() -> &str` names the focused output the per-output values
(`view_pos`/`workspace_idx`) belong to.

niri emits these only while something is moving and coalesces sub-epsilon
changes, so the service is silent at rest; the Wayland frame callback then
collapses the 60–144 Hz stream into one redraw per refresh. Because the
values are continuous, feed them straight into widget geometry — niri has
already done the easing.

## Control actions (fork addition)

Typed requests to the compositor, each dispatched on a shared worker thread
so a slow socket never stalls drawing (like `Workspaces::focus`):

| Method | niri action |
| --- | --- |
| `Windows::focus(id)` | `FocusWindow` |
| `Windows::close(id)` | `CloseWindow` |
| `Windows::fullscreen(id)` | `FullscreenWindow` |
| `Windows::toggle_floating(id)` | `ToggleWindowFloating` |
| `Windows::move_to_workspace(id, ws)` | `MoveWindowToWorkspace` |
| `Windows::focus_column_left()` | `FocusColumnLeft` |
| `Windows::focus_column_right()` | `FocusColumnRight` |
| `Windows::spawn(cmd)` | `Spawn` |
| `Windows::screenshot()` | `Screenshot` |
| `Windows::toggle_overview()` | `ToggleOverview` |

`Windows::spawn(cmd)` asks the *compositor* to launch a command, so it is
tracked like any keybinding-launched program; use the crate-level
[`spawn`](../src/process.rs) when the shell should own the child itself.

## How it works

Each `Service` owns its thread and a blocking `listen()` loop
([src/services.rs](../src/services.rs)). `Workspaces` and `Windows` each
open their **own** niri `EventStream` connection (niri allows multiple) and
keep only the slice they need:

- `compositor::listen(|workspaces| …)` — the workspace list, re-emitted
  after every relevant event.
- `compositor::listen_windows(|windows| …)` — the window list.
- `compositor::listen_gestures(|gesture| …)` — the live gesture snapshot
  (niri only; a no-op on other backends).

All parse the same niri event set
([src/compositor/niri.rs](../src/compositor/niri.rs)) —
`WorkspacesChanged`, `WorkspaceActivated`, `WorkspaceUrgencyChanged`,
`WindowsChanged`, `WindowOpenedOrChanged`, `WindowClosed`,
`WindowFocusChanged`, `OverviewOpenedOrClosed`, and the fork's
`OverviewProgress` / `ViewOffsetChanged` / `WorkspaceSwitchProgress` — and
ignore the rest. An event that changes nothing a
service reads (e.g. a workspace event seen by the window listener) leaves
its list equal, and the service calls `quiet()` to redraw nothing.

Two parallel listeners are a deliberate choice over one shared stream
fanned out to both services: it keeps the one-thread-per-service model
intact and each service self-contained. The cost is a second socket
connection and a second parse of the stream — negligible for an event
stream.

## Backend support

| | Workspaces | `focus_workspace` | Windows | actions | Gestures |
| --- | --- | --- | --- | --- | --- |
| niri | ✅ | ✅ | ✅ | ✅ | ✅ |
| hyprland | ✅ | ✅ | — | — | — |
| sway | ✅ | ✅ | — | — | — |
| mango | ✅ | ✅ | — | — | — |

On a backend without window support `Windows::read().list()` is empty and
the control methods are no-ops; the per-backend `listen_windows`/`act`
stubs (e.g. [hyprland.rs](../src/compositor/hyprland.rs)) are where that
support would be added.

## niri version note

Action request shapes and window-event fields are verified against the
niri IPC (`niri-ipc` crate) for **niri 26.04**. niri's IPC has changed
field names across versions; a mismatched request fails silently (niri
replies with an error that `act` currently drains and discards). If actions
stop working after a niri upgrade, re-check the JSON against the installed
`niri msg --json` / the `niri_ipc::Action` enum.
