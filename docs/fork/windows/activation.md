# Activation (fork addition)

`xdg-activation-v1` lets a program the user just launched take the focus, and lets an app that
is already running come to the front. The fork uses it both ways: as a launcher, amane asks the
compositor for a token and starts programs with it (`spawn_activated`); as an app, amane reads
the token it was started with, or one handed over by a second launch, and focuses its window
with it (`82337f2`). Designed in
[spec-write-own-layer-rules.md](../design/spec-write-own-layer-rules.md#part-b--launch-with-activation-tokens-xdg-activation-v1).

## Usage

### Launching with a token

```rust,ignore
.on_click(|_| amane::spawn_activated("firefox"))
```

`spawn_activated(command)` is `spawn` for launchers, docks and buttons that open an app. It asks
the compositor for an activation token for the input being handled, and starts the program with
it in `XDG_ACTIVATION_TOKEN` (and `DESKTOP_STARTUP_ID`). The program's window can then take the
focus. Call it from input, like a click, since the token stands for that input.

- Without `xdg-activation` in the compositor, or outside an event loop, it's the same as
  `spawn`.
- `DesktopApp::launch`, `DesktopApp::launch_with` and `DesktopAction::launch` launch this way
  ([Apps and processes](../system/apps-and-processes.md#launching-apps)).
- Plain `spawn` no longer passes on amane's own token: a program amane starts gets its own or
  none.

### Being activated

Nothing to call. An amane app's window takes the focus with:

- the `XDG_ACTIVATION_TOKEN` it was started with, on its first configure;
- the token a second launch hands over when the app is already running
  ([One copy at a time](../system/ipc.md#one-copy-at-a-time)). The running copy runs
  `App::on_activate` (or reopens its windows when it has none), and the token goes to the newest
  normal window once it is on screen.

```rust,ignore
fn main() {
    App::new()
        .app_id("amane-notes")
        .normal_window("main", main_window)
        .on_activate(|files| open_files(files))   // the token is used after this runs
        .run();
}
```

From a terminal there's no token, and the compositor decides.

## Design / internals

- `src/wayland/activation.rs`: `WaylandState::activate` runs on a handed-over launch, whose
  first argument is the token (empty without one). `use_token` focuses the newest normal window
  once it has a size; one still waiting for its first configure gets the token then.
- `spawn_activated` queues the command in `process::ACTIVATED` and wakes the loop.
  `handle_spawn_requests` asks for one token per command, with the app's app-id, the seat and
  the last input serial, and the surface with the keyboard focus (or the first window). The
  `ActivationHandler::new_token` callback starts the command with its token.
- `hand_over` in `src/app.rs` sends the token, then the arguments, to the running copy's
  `activate` IPC call.
- Live-tested on niri: `spawn_activated` launching a copy, and a handed-over token moving the
  focus to an already open window.
