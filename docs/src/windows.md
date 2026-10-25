# Multiple Windows and Monitors

A shell is rarely one window. This page covers several layer windows, one window per monitor, normal app-style windows, and opening and closing windows while the shell runs.

## Several windows

Call `.window` once for each window:

```rust
use amane::{App, Full, Horizontal, LayerWindow, Parent, Rectangle, Vertical};

fn main() {
    App::new().window(bar).window(corner).run();
}

fn bar() -> LayerWindow {
    LayerWindow::new()
        .width(Full)
        .height(30.0)
        .anchor_vertical(Vertical::Top)
        .child(Rectangle::new().width(Parent).height(Parent).fill("#1e1e2e"))
}

fn corner() -> LayerWindow {
    LayerWindow::new()
        .width(200.0)
        .height(60.0)
        .anchor_vertical(Vertical::Bottom)
        .anchor_horizontal(Horizontal::Right)
        .child(Rectangle::new().width(Parent).height(Parent).fill("#313244"))
}
```

Each window has its own view, and each one only redraws for the Services its own view reads. Windows added with `.window` go on whichever monitor the compositor picks, usually the focused one.

## One window per monitor

A bar should be on every monitor, including ones you plug in later:

```rust
use amane::{App, Color, Full, Layer, LayerWindow, Monitor, Parent, Rectangle, Text, Vertical};

fn main() {
    App::new().window_per_monitor(bar).run();
}

fn bar(monitor: &Monitor) -> LayerWindow {
    let label = format!("{} {}x{}", monitor.name, monitor.width, monitor.height);

    LayerWindow::new()
        .width(Full)
        .height(30.0)
        .anchor_vertical(Vertical::Top)
        .layer(Layer::Top)
        .child(
            Rectangle::new()
                .width(Parent)
                .height(Parent)
                .fill(Color::BLUE)
                .child(Text::new(label).color(Color::WHITE)),
        )
}
```

The view takes a `&Monitor`, with the monitor's `name` (like `"DP-1"`), `width`, and `height`. Its dimensions use the same units as widgets, accounting for both the compositor's scale and [the app's scale factor](scaling.md). Amane opens one window for each monitor when it starts, opens a new one when a monitor is plugged in, and closes it when the monitor is unplugged.

Use the monitor to show different things on different screens:

```rust,ignore
if monitor.name == "eDP-1" {
    // the laptop screen
}
```

## Normal windows

A normal window is a regular app window, with a title bar, that the compositor places and tiles like any other app. Use one for a settings panel, or anything you'd want to move around:

```rust
use amane::{App, Center, Color, Parent, Rectangle, Text, Window};

fn main() {
    App::new().normal_window("settings", settings).run();
}

fn settings() -> Window {
    Window::new()
        .title("amane settings")
        .size(480.0, 320.0)
        .resizable(true)
        .child(
            Rectangle::new()
                .width(Parent)
                .height(Parent)
                .fill("#313244")
                .align_child(Center, Center)
                .child(Text::new("settings go here").color(Color::WHITE)),
        )
}
```

- `title` defaults to `"amane"`. It can change while the window is open, for example to mark unsaved changes.
- `app_id` is what the compositor, docks and window rules know the app by, like `"amane-settings"`. It's read once, when the window opens. Without it the window takes `App::app_id`, and without that `"amane"`.
- `size` is the size it opens at, 640 by 480 by default. The compositor may give it another size, and a tiling compositor usually does.
- `resizable` lets the user resize it. It's `true` by default.
- `min_size` and `max_size` limit how small or big the user can make it. A tiling compositor may still give a tiled window less.
- `on_key` works the same as on a layer window. A normal window gets keyboard focus like any app, so it needs no `Keyboard` setting.

The name, `"settings"` here, tells windows apart. It's used to close the window later.

Everything except `app_id` and `size` is read again on every frame, and only what changed is sent to the compositor, the same as for layer windows.

### Maximized and fullscreen

`.maximized(true)` and `.fullscreen(true)` ask the compositor to maximize the window or make it fullscreen. The request is only sent again when the view's answer changes, so the user can still unmaximize it from the compositor without the view fighting back. Leave them unset to let the user decide.

`minimize_window(name)` asks the compositor to hide the window until the user brings it back. Not every compositor can; niri can't, and does nothing.

### What the compositor says

Inside a view, `window_state()` tells you what the compositor last said about the window:

```rust,ignore
let state = window_state();

// grey the window out while another one has focus, like most apps do
let background = if state.focused { "#313244" } else { "#24273a" };
```

| Field | Meaning |
|---|---|
| `focused` | the window the user is working in |
| `maximized`, `fullscreen` | as the compositor has it, which may differ from what the view asked |
| `tiled` | placed edge to edge with other windows |
| `resizing` | the user is dragging its edges right now |
| `server_decorated` | the compositor draws its title bar. When `false`, the window has none, and Amane doesn't draw one |

In a layer window or a popup, all of them are `false`. A change redraws the window.

### Asking before closing

`on_close` runs when the user closes the window from the compositor, like with a keybind or the title bar. Return `false` to keep it open, for example to ask about unsaved changes first:

```rust,ignore
Window::new().on_close(|| {
    if Document::read().unsaved {
        open_window("unsaved", unsaved_dialog);

        return false;
    }

    true
})
```

`close_window` doesn't ask, so a dialog's "Discard" button can close the window for real.

### Dialogs

`.parent(name)` shows a window over the open window with that name, the way a dialog belongs to its app. `.modal(true)` also keeps the user out of the parent until the dialog closes, where the compositor supports it (`xdg-dialog`). Both are read once, when the window opens.

```rust,ignore
fn unsaved_dialog() -> Window {
    Window::new()
        .title("Unsaved changes")
        .size(300.0, 140.0)
        .resizable(false)
        .parent("main")
        .modal(true)
        .child(...)
}
```

### An app of its own

A normal window doesn't need a bar or the shell. An app that runs next to the shell sets its app-id and leaves the IPC socket to the shell:

```rust,ignore
fn main() {
    App::new()
        .app_id("amane-settings")
        .without_ipc()
        .normal_window("main", main_window)
        .run();
}
```

The app exits when its last window closes. `App::keep_alive()` keeps it running anyway, for an app that only opens windows when asked.

See `examples/app_window.rs` for all of this in one app.

## Opening and closing windows later

Windows given to `App` open when the shell starts. To open one later, for example from a button, call `open_window` with a name and a view:

```rust
use amane::{App, Full, LayerWindow, Parent, Pointer, Rectangle, Window, close_window, open_window};

fn main() {
    App::new().window(bar).run();
}

fn bar() -> LayerWindow {
    LayerWindow::new().width(Full).height(30.0).child(
        Rectangle::new()
            .width(Parent)
            .height(Parent)
            .fill("#1e1e2e")
            .cursor(Pointer)
            .on_click(|_| open_window("settings", settings)),
    )
}

fn settings() -> Window {
    Window::new().title("settings").child(
        Rectangle::new()
            .width(120.0)
            .height(40.0)
            .fill("#f38ba8")
            .on_click(|_| close_window("settings")),
    )
}
```

- `open_window(name, view)` opens a normal window. If a window with that name is already open, it does nothing, so clicking the button twice doesn't open two.
- `close_window(name)` closes it. Closing a window that isn't open does nothing.
- The user can also close a normal window from its title bar or with their compositor's keybind.

They all work from anywhere: input handlers, IPC handlers, or a Service's thread.

### Many windows from one view

A view given to `open_window` is a plain function, so every window it opens shows the same thing. `open_window_with` takes a closure instead, so each window can carry its own data, like the document it shows:

```rust,ignore
open_window_with(format!("document {path}"), move || document(&path));
```

The name still tells windows apart, so give each one its own. Inside a view, `current_window()` gives the name of the window being drawn. Move it into a handler to close the window from inside it:

```rust,ignore
fn document(path: &str) -> Window {
    let me = current_window();

    Window::new().child(close_button().on_click(move |_| close_window(&me)))
}
```

`open_window` only opens normal windows. To show and hide a layer window, like a popup or a launcher, keep it open and use `.visible(...)` ([Layer Windows](layer_windows.md)).

## The real window size

A compositor can give a window another size than it asked for. Inside a view, `window_size()` gives the size the window really has, in widget units before [the app's scale factor](scaling.md):

```rust,ignore
let (width, height) = window_size();
```

It's `(0.0, 0.0)` until the compositor has said, which can happen for the very first frame.
