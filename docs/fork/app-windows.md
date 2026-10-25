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

## Phase 2: popups over app windows (done)

| Addition | Notes |
|---|---|
| `Popup::parent_window(name)` | parents a popup to a normal window's `xdg_surface` |
| `open_popup(closure)` | a slot opened from a handler and dropped once its popup has closed; a namespace that is already open is replaced |
| `pointer_position()` | where the pointer last was in its window, for anchoring a context menu |
| Popups close before their window | `WaylandState::close` closes a normal or layer window's popups first (xdg-shell forbids the reverse) |
| Keys to a grabbing popup | when the compositor keeps the focus on the window under a menu, keys go to the newest grabbing popup |

The popup docs are [popups.md](popups.md). Reading the configure's `position` (to know a popup was flipped) was planned but left out, because nothing needs it yet.

## Phase 3: keyboard (done)

| Addition | Notes |
|---|---|
| Key repeat | SCTK's `get_keyboard_with_repeat` on the event loop, plus `repeat_key` for compositors that repeat keys themselves; both run the same key-down path |
| `Key::{Delete, Insert, PageUp, PageDown, F(n)}` | keypad keys map to what they stand for; `ISO_Left_Tab` (Shift+Tab) is `Tab` |
| `on_key_release` | on `Window`, `LayerWindow` and `Popup` |
| `on_shortcut(Fn(Key) -> bool)` | runs before a focused `TextInput` for chords with Ctrl, Alt or Super; `true` keeps the key from the input |
| Focus redraw | a window redraws when it gets the keyboard, as it already did when it lost it |

Internally, a window's key handlers are one `KeyHandlers` struct (`src/input/target.rs`) instead of a lone `on_key`.

## Phase 4: clipboard and drag and drop (done)

| Addition | Notes |
|---|---|
| `Clipboard::{copy, paste}` | `wl_data_device` selection; text offered as `text/plain;charset=utf-8`, `UTF8_STRING`, `text/plain`, `TEXT`, `STRING` |
| `PrimarySelection::{copy, paste}` | `zwp_primary_selection_v1`, optional |
| `Rectangle::on_text_drop` | text drags are taken anywhere and land on the innermost handler |
| `Rectangle::drag_out(Fn() -> DragData)` | starts `wl_data_device.start_drag` after 8 px of motion with the left button down, using the press serial |
| `changes::wake()` | wakes the loop without redrawing every window |

Data is written and pasted text read on their own threads (`src/wayland/clipboard.rs`). When amane owns the selection or the drag, it uses its own copy instead of reading a pipe it would have to write on the same thread.

Live-tested on niri with `wl-copy`/`wl-paste` in both directions for both selections. Drag-out and text drops are not live-tested, because no pointer automation tool is available; the uri-list encoding has a round-trip test. The shell's `text_edit.rs` still uses `wl-copy`/`wl-paste` until it pins an amane rev with this in it.

## Phase 5: text input (done)

| Addition | Notes |
|---|---|
| Selection | Shift with any movement, mouse drag, Ctrl+A; drawn behind the text (`selection_color`) |
| Mouse | a press places the cursor (Shift extends), a drag selects, a middle click pastes the primary selection there |
| Editing keys | Delete, word movement and deletion with Ctrl, Ctrl+Home/End |
| Clipboard | Ctrl+C/X/V; selections go to the primary selection; a password never leaves the input |
| Undo/redo | Ctrl+Z, Ctrl+Shift+Z/Ctrl+Y; typing is grouped a word at a time; 200 steps per input |
| `multiline(true)` | hard line breaks, Up/Down keep the column; no soft wrapping yet |
| Scrolling | a one-line input scrolls to keep the cursor in sight |
| IME | `zwp_text_input_v3` (`src/wayland/text_input.rs`): enabled while an input has focus and the window has the keyboard; surrounding text, cursor rectangle and content type (password, multiline) are sent on change; preedit drawn underlined |
| `TextInput::text(id)` | reads an input's text |

Other changes:
- The focused input's handler returns whether it used a key, so chords it doesn't know (Ctrl+Q) reach the window.
- `Clipboard::paste` callbacks now run on the event loop rather than the reader thread.

The editing model is `src/widgets/text_input/field.rs`, with unit tests. Live-tested on niri: typing, selection drawing, Shift+Home, cut and paste via the system clipboard, undo, Ctrl+Backspace, multiline Up/Down, one-line scrolling. Mouse selection and the IME are not live-tested, because no pointer automation tool or input method is installed; the IME's byte/letter conversions have unit tests.

## Planned

1. Separate app processes: an IPC socket per app-id, single instance with activation, `ipc::call` between apps, `xdg-activation`.
2. Platform helpers: a `Config<T>` file service, a file chooser through the portal (with xdg-foreign for the parent), a notification send helper, double click and side buttons.
3. In amane-shell: an `amane-kit` widget crate, then the settings app.

Out of scope for now: client-side decorations, running without layer-shell, accessibility.
