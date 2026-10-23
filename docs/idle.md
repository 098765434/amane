# Idle (fork addition)

`Idle` says when the user has been away from the mouse and keyboard, and
whether a program has asked not to idle, like a browser playing a video.
The compositor's **ext-idle-notify** protocol gives the first, and amane
serving **org.freedesktop.ScreenSaver** gives the second.

```rust
const DIM: Duration = Duration::from_secs(120);
const LOCK: Duration = Duration::from_secs(300);

// once, before App::run or from an input handler
Idle::set_levels(&[DIM, LOCK]);

let idle = Idle::read();

if !idle.inhibited() {
    if idle.crossed(LOCK) {
        // Lock::start(), from outside the view
    } else if idle.crossed(DIM) {
        // draw the bar dimmed
    }
}
```

| Function | Gives |
| --- | --- |
| `is_idle()` | `true` while the user has been away longer than at least one level |
| `crossed(level)` | `true` while the user has been away longer than this level |
| `levels()` | the levels being watched, 120s and 300s by default |
| `inhibited()` | `true` while any program holds a ScreenSaver inhibit. Check it before dimming or locking. |
| `inhibitors()` | the `(application, reason)` of each inhibit, for a presence chip |
| `running()` | `false` when another program already serves `org.freedesktop.ScreenSaver` |
| `Idle::set_levels(levels)` | replaces the levels, and starts every timer again |

## How it works

**Idle detection.** The backend binds `ext_idle_notifier_v1` when the
compositor offers it (niri, Sway, Hyprland and other wlroots-style
compositors). Once a shell first uses `Idle`, it creates one
`ext_idle_notification_v1` per seat and level. `idled` crosses the level
and `resumed` clears it. These come from the main event loop, so they
redraw on the next frame. The timers follow the compositor's own idle
inhibitors, so a fullscreen video that inhibits through Wayland also keeps
`crossed` clear. Without the protocol, nothing is ever crossed.

**Inhibits.** On a bare session nothing serves `org.freedesktop.ScreenSaver`,
so programs' inhibits are lost. amane owns the name, plus
`org.kde.screensaver` where it is free. It answers on both
`/org/freedesktop/ScreenSaver` (Chromium) and `/ScreenSaver` (Firefox and
KDE programs):

- `Inhibit(application, reason) -> cookie`
- `UnInhibit(cookie)`
- `GetActive() -> is_idle()`

It also answers `Introspect`, since Qt programs read it before calling.
A program that quits or crashes without `UnInhibit` is released when its
unique name leaves the bus (`NameOwnerChanged`).

If something already owns the name, amane stays passive: `running()` is
`false`, `inhibited()` stays `false`, and only the compositor's timers are
followed. niri, for one, serves `org.freedesktop.ScreenSaver` itself and
turns those inhibits into Wayland idle inhibitors. Under niri, a video that
inhibits therefore keeps `crossed` clear on its own.

amane never dims, locks or blanks anything by itself. The shell decides,
from `crossed`, `inhibited` and `Lock::start()`.

There is no logind `SetIdleHint` yet.
