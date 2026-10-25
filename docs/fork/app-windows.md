# App windows (fork addition)

Normal windows (`Window`, `App::normal_window`, `open_window`) were set up once when they opened and never updated after that. This work turns them into windows a regular app can be built on, so apps such as a settings app can run as their own processes next to the shell. The user guide is [windows.md](../src/windows.md#normal-windows). `examples/app_window.rs` shows everything in one app.

## Phase 1: normal windows that can change (done)

| Addition | Notes |
|---|---|
| `Window::app_id`, `App::app_id` | the app-id was hard-coded to `"amane"` |
| `Window::min_size`, `max_size` | `resizable(false)` still pins min = max |
| Live title, min/max, `maximized`, `fullscreen` | diffed like layer settings in `OpenWindow::update_normal` (`src/wayland/normal.rs`), and only what changed is sent |
| `window_state()` | `focused`, `maximized`, `fullscreen`, `tiled`, `resizing`, `server_decorated`, read from every configure |
| `Window::on_close` | `false` keeps the window open; `close_window` doesn't ask |
| `Window::parent`, `modal` | `xdg_toplevel.set_parent`, plus `xdg_wm_dialog_v1` for modal when the compositor has it |
| `open_window_with`, `current_window` | views can be closures with their own data, so one view can show many windows; window names are owned `String`s |
| `minimize_window` | niri ignores it |
| `App::keep_alive` | the app no longer exits when its last window closes |

Fixes along the way:
- A user-resized window keeps its size when a later configure leaves the size to the client.
- The first view run of a normal window goes through `frame::run_view`.
- `AMANE_FRAMES` names normal windows by their key.
- Open, close and minimize requests share one ordered queue (`window::REQUESTS`).

Not done: toplevel window geometry is not set, because without client-side decorations it would be the whole surface anyway.

## Planned

1. Popups, menus and tooltips parented to normal windows, and popups opened at runtime.
2. Keyboard: key repeat, more keys (Delete, PageUp/PageDown, F-keys), key release, and shortcuts that still fire while a `TextInput` has focus.
3. Clipboard and primary selection, text drops, dragging out.
4. `TextInput`: selection, mouse caret placement, word movement, undo, multiline, IME (`text-input-v3`).
5. Separate app processes: an IPC socket per app-id, single instance with activation, `ipc::call` between apps, `xdg-activation`.
6. Platform helpers: a `Config<T>` file service, a file chooser through the portal (with xdg-foreign for the parent), a notification send helper, double click and side buttons.
7. In amane-shell: an `amane-kit` widget crate, then the settings app.

Out of scope for now: client-side decorations, running without layer-shell, accessibility.
