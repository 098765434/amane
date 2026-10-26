# Fork API cheatsheet

The fork's rows for upstream's [Cheatsheet](../src/cheatsheet.md): only what the fork adds or
changes. Each section matches a section there.

## Windows

| Code | Does |
|---|---|
| `.app_id("amane-settings")` | what the compositor and docks know it by, `App::app_id` by default |
| `.resizable(true)` | lets the user resize it, `true` by default (upstream: `false`) |
| `.min_size(w, h)` / `.max_size(w, h)` | limits for the user's resizing |
| `.maximized(true)` / `.fullscreen(true)` | asks the compositor, again only when the answer changes |
| `.parent("name")` / `.modal(true)` | a dialog over another normal window |
| `.on_shortcut(handler)` | chords with Ctrl/Alt/Super, before a focused text input, `Fn(Key) -> bool` |
| `.on_key_release(handler)` | keys going up, `Fn(Key)` |
| `Clipboard::copy(text)` / `Clipboard::paste(then)` | the Ctrl+C/Ctrl+V clipboard; `then` runs once the text arrives |
| `PrimarySelection::copy` / `::paste` | the middle-click selection |
| `Rectangle::on_text_drop(handler)` / `.drag_out(data)` | text dragged in; dragging `DragData` out |
| `Config::<T>::read()` / `::change(edit)` | a settings file in `~/.config`, shared live between programs |
| `FileDialog::open(title)...show(then)` | the desktop's file dialog through the portal |
| `Notify::new(summary).body(..).send()` | sends a desktop notification |
| `click_count()` | 1, 2 or 3 for the click being handled |
| `.on_close(handler)` | asked when the user closes it, `Fn() -> bool`; `false` keeps it open |
| `open_window_with(name, closure)` | `open_window` with a view that carries its own data |
| `close_window("name")` | closes it, without asking `on_close` |
| `minimize_window("name")` | asks the compositor to minimize it |
| `window_state()` | `focused`, `maximized`, `fullscreen`, `tiled`, `resizing`, `server_decorated` |
| `current_window()` | the name of the normal window being drawn |
| `App::app_id("id")` / `App::keep_alive()` | the app's default app-id and IPC socket; keep running with no windows |
| `App::on_activate(handler)` | the app was launched again, `fn(&[String])` with that launch's arguments |
| `ipc_call(app_id, name, args)` | calls another app's IPC handler, returns its reply |
| `spawn_activated(command)` | starts a program with an activation token, for launchers |

## Input

| Code | Gets |
|---|---|
| `.on_scroll(\|scroll\| ...)` | `Scroll { x, y, source, stop }`: `x`, `y` in lines, positive `y` is down; `source` is `Wheel`, `Finger`, `Continuous` or `Tilt`; `stop` when the fingers lift; `pixels()` |
| `.on_release(\|point\| ...)` | `Point`, once when a drag's button is let go (before `on_click`) |
| `.on_drop(\|files\| ...)` | the `Vec<PathBuf>` dragged in from another program |
| `.on_files_over(\|at, files\| ...)` | `Option<Point>` and the held files; answer `true` if they can be let go there |
| `modifiers()` | the keys held now: `ctrl`, `shift`, `alt`, `logo`; `command()` for Ctrl, Alt or Super. `Ctrl`+letter reaches `on_key` as `Character(letter)` |

## Services

| Service | Reads | Controls |
|---|---|---|
| `Battery` | adds `state`, `on_ac`, `warning`, `time_to_empty`, `time_to_full`, `icon`, `peripherals` | |
| `PowerProfile` | `present`, `active`, `available`, `degraded` | `set` |
| `Idle` | `is_idle`, `crossed`, `levels`, `inhibited`, `inhibitors`, `running` | `set_levels` |
| `Apps` | each `DesktopApp` adds `id`, `categories`, `keywords`, `actions`, `mime_types`, `opens(file)`, `commands_with(files)` | `launch_with(files)` on each `DesktopApp` |
| `Appearance` | `present`, `scheme`, `prefers_dark`, `accent` | |
