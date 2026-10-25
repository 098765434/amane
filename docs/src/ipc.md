# IPC

IPC lets other programs talk to your running shell. Its main use is keybinds: your compositor runs `amane ipc call toggle-launcher`, and your shell opens the launcher.

## Toggling a window from the command line

```rust
use std::time::Duration;

use amane::{App, LayerWindow, Parent, Rectangle, Service};

struct Launcher {
    open: bool,
}

impl Service for Launcher {
    fn new() -> Self {
        Self { open: false }
    }

    fn interval() -> Duration {
        Duration::from_secs(3600)
    }
}

fn main() {
    App::new().ipc("toggle-launcher", toggle).window(view).run();
}

fn view() -> LayerWindow {
    let launcher = Launcher::read();

    LayerWindow::new()
        .width(400.0)
        .height(300.0)
        .visible(launcher.open)
        .child(Rectangle::new().width(Parent).height(Parent).fill("#1e1e2e"))
}

fn toggle(_: &[String]) -> String {
    let mut launcher = Launcher::write();

    launcher.open = !launcher.open;

    if launcher.open {
        String::from("opened")
    } else {
        String::from("closed")
    }
}
```

From a terminal:

```sh
amane ipc call toggle-launcher
```

It prints `opened`, and the launcher appears. Run it again to close it.

## How a call reaches your shell

- `App::ipc(name, handler)` registers a handler under a name. Call it once per name.
- Your shell listens on a socket, `$XDG_RUNTIME_DIR/amane.sock`. Other apps have their own ([One copy at a time](#one-copy-at-a-time)).
- `amane ipc call <name> [arguments...]` connects to that socket, sends the name and arguments, and prints the reply.
- The handler gets the arguments and returns a `String`. Whatever it returns is printed by `amane ipc call`.
- The handler writes to a Service, so every window that reads it redraws ([State and Services](services.md)).

The handler runs on the same thread that draws, so keep it quick. For slow work, start a thread or use `amane::spawn`.

## Passing arguments

Everything after the name is passed to the handler as a list of strings:

```rust,ignore
fn main() {
    App::new().ipc("say", say).window(view).run();
}

fn say(arguments: &[String]) -> String {
    let words = arguments.join(" ");

    Message::write().text = words.clone();

    format!("showing: {words}")
}
```

```sh
amane ipc call say hello world
```

The handler gets `["hello", "world"]`. Arguments can't contain newlines.

Calling a name that has no handler answers `no handler named <name>`.

## Binding calls to keys

niri:

```kdl
binds {
    Mod+Space { spawn "amane" "ipc" "call" "toggle-launcher"; }
}
```

Hyprland:

```text
bind = SUPER, SPACE, exec, amane ipc call toggle-launcher
```

Sway:

```text
bindsym $mod+space exec amane ipc call toggle-launcher
```

## One copy at a time

Each app listens on its own socket, named after its app-id ([`App::app_id`](windows.md#an-app-of-its-own)):

| App-id | Socket in `$XDG_RUNTIME_DIR` |
|---|---|
| `amane`, the default and the shell's | `amane.sock` |
| `amane-settings` | `amane-settings.sock` |
| `org.example.Notes` | `amane-org.example.Notes.sock` |

So the shell and your apps listen side by side, and only one copy of each runs. Starting an app whose socket already answers doesn't start a second copy: the new launch hands its arguments to the running copy and exits, printing `amane: <app id> is already running, it takes this launch`. The running copy then:

- runs `App::on_activate(handler)` with the new launch's arguments, like a file to open, or
- without `on_activate`, brings its window to the front, opening it again if it was closed (an app kept running by `App::keep_alive`).

When the launch came from a launcher with an activation token, the running copy's window takes the focus with it ([Launching apps](processes.md#launching-apps)). From a terminal there's no token, and the compositor decides.

```rust,ignore
fn main() {
    App::new()
        .app_id("amane-notes")
        .normal_window("main", main_window)
        .on_activate(|files| {
            for file in files {
                let file = file.clone();

                open_window_with(file.clone(), move || note(&file));
            }
        })
        .run();
}
```

`amane run` checks for a running shell first and says so in your terminal, and `amane restart` stops the running shell before starting the new one. `amane dev` restarts its own shell, but it can't stop a shell started some other way. Stop your everyday shell (the one from `amane run`) before running `amane dev`.

For a window that should run as many copies as it's started, use `App::new().without_ipc()`. It listens on no socket, so `on_activate` and IPC handlers aren't used.

If `amane ipc call` says `amane is not running`, no shell is listening for IPC in this session.

## Calling another app

`amane ipc call --app <app id> <name> [arguments...]` calls a handler in the app with that app-id instead of the shell. From Rust, `amane::ipc_call(app_id, name, arguments)` does the same and returns the reply, so an app can ask the shell to do something:

```rust,ignore
// a settings app telling the shell to look at its settings again
let _ = amane::ipc_call("amane", "reload", &[]);
```

`ipc_call` waits for the reply, which an Amane app gives within a second. It returns an error when that app isn't running.
