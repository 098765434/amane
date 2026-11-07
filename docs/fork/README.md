# amane fork docs

This fork of [amane](https://github.com/MystiaFin/amane) adds what the
[amane-shell](https://github.com/098765434/amane-shell) desktop needs: xdg-popups, niri
compositor services that follow gestures frame by frame, app windows that real apps run in,
blur shapes and vibrancy, parley text, spring animation, and more system services. The work is
on branch `ocbk`, on top of upstream commit `72e23eb` (tag `fork-base`).
[maintaining.md](maintaining.md) has the rebase workflow, the commit map and the housekeeping
rules.

Upstream's own guide ([`docs/src/`](../src/), the mdBook) and `ARCHITECTURE.md` are left exactly
as upstream has them; everything the fork adds is here.

## Features

| Feature | Summary | Key types | Doc | Example | Status |
|---|---|---|---|---|---|
| Workspaces | the workspace list (upstream) | `Workspaces`, `Workspace` | [services](compositor/services.md#workspaces) | `examples/workspaces.rs` | upstream |
| Windows | open windows, the focused one, and compositor actions | `Windows`, `WindowInfo` | [services](compositor/services.md#windows-fork-addition) | — | done |
| niri client | one shared event stream; asking niri for outputs, windows, casts and the fork's features | `amane::niri`, `niri::features()` | [services](compositor/services.md#asking-niri-amaneniri-fork-addition) | — | done |
| Keyboard layouts | the layouts and the one in use | `KeyboardLayouts` | [services](compositor/services.md#keyboardlayouts-fork-addition-niri-only) | — | done, niri |
| Overview | niri's overview progress alone | `Overview` | [services](compositor/services.md#overview-fork-addition-niri-only) | — | done, niri fork |
| Gestures | per-frame overview, view scroll and workspace switch | `Gestures` | [services](compositor/services.md#gestures-fork-addition-niri-only) | — | done, niri fork |
| WorkspaceSwitch | the workspace switch alone | `WorkspaceSwitch` | [services](compositor/services.md#workspaceswitch-fork-addition-niri-only) | — | done, niri fork |
| Desktop | whether each output's desktop is obscured; frozen layer windows | `Desktop`, `LayerWindow::frozen` | [services](compositor/services.md#desktop-fork-addition-niri-only) | — | done, niri fork |
| Popups | xdg-popups over layer and normal windows, grabs, nesting | `Popup`, `open_popup` | [popups](windows/popups.md) | `examples/app_window.rs` | done |
| App windows | normal windows with app-ids, live settings, state, close veto, dialogs | `Window`, `window_state`, `open_window_with` | [app-windows](windows/app-windows.md) | `examples/app_window.rs` | done |
| Activation | launching with tokens, being activated | `spawn_activated`, `App::on_activate` | [activation](windows/activation.md) | `examples/app_window.rs` | done |
| Clipboard | clipboard, primary selection, text drops, dragging out | `Clipboard`, `PrimarySelection`, `DragData` | [clipboard](windows/clipboard.md) | `examples/app_window.rs` | done |
| Portals | file dialog and sending notifications | `FileDialog`, `Notify` | [portals](windows/portals.md) | `examples/app_window.rs` | done |
| Config | settings files shared live between programs | `Config`, `Settings` | [config](windows/config.md) | — | done |
| Blur | blur regions and rounded, fading blur shapes with materials | `BlurShape`, `blur_region`, `blur_shapes` | [blur](graphics/blur.md) | `examples/blur_shapes.rs` | done; shapes need the niri fork |
| Backdrop | niri's luminance readings behind each shape | `Backdrop` | [blur](graphics/blur.md#blur-shapes--niri-blur-shapes-v1) | — | done, niri fork |
| Vibrancy | grey content blended with the frosted backdrop | `vibrant` | [blur](graphics/blur.md#vibrancy-niri-vibrancy-v1) | — | done, niri fork |
| Canvas paths | SVG paths, placing, trimming, line joins; 64 shader rows | `Path::svg`, `Join` | [canvas](graphics/canvas.md) | `examples/stroke_clock.rs` | done |
| Shader effects | multi-pass WGSL over a rectangle's drawing, its backdrop or an image; hot reload; compile errors; a pausable shader clock; frame stats; shadow spread | `Effect`, `Input`, `shader_errors`, `pause_shader_time`, `frame_stats`, `Shadow::spread` | [effects](graphics/effects.md) | `examples/effect.rs` | done |
| Text styling | parley layout: italic, line height, spacing, case, decoration, align, direction, axes; color emoji | `Case`, `Decoration`, `TextAlign`, `TextDirection`, `Weight::Number` | [text](graphics/text.md) | `examples/text.rs` | done |
| Video | NV12 frames from another thread, drawn as quads | `VideoFeed`, `VideoFrame`, `Video` | [video](graphics/video.md) | — | done |
| Animation | spring easing that keeps its speed, `retime` | `Easing::Spring`, `Animation::retime` | [animation](graphics/animation.md) | — | done |
| Scrolling | coasting after a flick, eased wheel steps, following a chat's end, keeping the view as content is added above | `ScrollArea::scroll_to_end`, `at_end`, `keep_bottom` | [input](input/input.md#scrolling) | — | done |
| `on_track` | pointer motion that redraws nothing; also scroll source, modifiers, shortcuts, double clicks | `on_track`, `Scroll`, `modifiers` | [input](input/input.md) | — | done |
| Text input | selection, editing keys, undo, multiline, IME | `TextInput` | [text-input](input/text-input.md) | `examples/text_editor.rs` | done |
| Input method | be the seat's input method: a popup by the focused field in any app, and checked fill through niri-secure-fill | `App::input_method`, `FocusedField`, `FieldOwner`, `FillError` | [input-method](input/input-method.md) | `examples/input_method.rs` | done, niri fork for fill |
| Battery | UPower: state, warning, time left, peripherals | `Battery`, `PowerDevice` | [battery](system/battery.md) | `examples/system.rs` | done |
| Power profile | power-profiles-daemon | `PowerProfile` | [power-profile](system/power-profile.md) | — | done |
| Bluetooth | signals, operations and failures, pairing agent, rfkill, discoverable | `Bluetooth::serve_agent`, `PairingRequest`, `BluetoothError` | [bluetooth](system/bluetooth.md) | — | done |
| KDE Connect | phones: pairing, battery, ring, files, clipboard, run commands, remote input, text messages with attachments and new conversations, contact photos, addresses, notifications | `KdeConnect`, `PhoneDevice`, `PhoneMessages`, `SmsAttachment` | [kdeconnect](system/kdeconnect.md) | — | done |
| Tray | the system tray: StatusNotifierItem watcher and host, item icons (theme, own folder or pixels), dbusmenu menus, clicks and scrolling | `Tray`, `TrayItem`, `TrayMenuItem`, `TrayStatus`, `TrayToggle` | [tray](system/tray.md) | — | done |
| Status items | a program's own item in the status bar and its activities (something going on now), over the bar's `org.amane.StatusBar1`; registered again when the bar restarts | `StatusItem`, `LiveActivity`, `status_item::{ItemSpec, ActivityContent}` | [status-items](system/status-items.md) | `examples/status_item.rs` | done (client; the bar serves it) |
| adb | Android phones over USB and Wireless debugging: devices, mDNS discovery, pairing, connecting | `Adb`, `AdbDevice`, `WirelessService`, `amane::adb` | [adb](system/adb.md) | — | done |
| Brightness | in HDR, the panel's SDR white in place of the backlight; uevents instead of polling | `Brightness::hdr`, `HdrPanel` | [brightness](system/brightness.md) | — | done |
| Idle | ext-idle-notify levels and ScreenSaver inhibits | `Idle` | [idle](system/idle.md) | `examples/idle.rs` | done |
| Polkit agent | the session's authentication agent: polkit's prompts as the shell's own | `Polkit::serve_agent`, `AuthRequest` | [polkit](system/polkit.md) | — | done |
| Appearance | colour scheme, accent, contrast, reduced motion | `Appearance`, `Scheme` | [appearance](system/appearance.md) | `examples/appearance.rs` | done |
| Audio | device lists, kinds, default switching | `AudioDevice`, `AudioKind` | [audio](system/audio.md) | `examples/audio.rs` | done |
| Notifications | timeout, hints, icon path, expire | `Timeout`, `Notifications::expire` | [notifications](system/notifications.md) | `examples/notifications.rs` | done |
| Apps and processes | desktop entry id, categories, actions, MIME, opening files | `DesktopApp`, `DesktopAction` | [apps-and-processes](system/apps-and-processes.md) | `examples/apps.rs` | done |
| Radios | rfkill switches from `/dev/rfkill`, airplane mode, blocking | `Rfkill`, `Radio` | [rfkill](system/rfkill.md) | — | done |
| Session | logind: active, locked and idle hints, lock requests, sleep, inhibitors | `Logind` | [logind](system/logind.md) | — | done |
| Clipboard watcher | the selection as it changes, from any program, for a history | `ClipboardWatcher` | [clipboard-watcher](system/clipboard-watcher.md) | — | done |
| Media | players followed by their MPRIS signals instead of a poll; the position worked out from the last reading | `Media`, `MediaPlayer::position` | [media](system/media.md) | — | done |
| Network | NetworkManager's signals; VPN, WireGuard, hotspot and wired profiles | `Network::profiles`, `NetworkProfile`, `Wired` | [network](system/network.md) | — | done |
| Lock keys | Caps Lock and Num Lock | `LockKeys` | [lock-keys](system/lock-keys.md) | — | done |
| Timers | jobs and redraws on the event loop at a time | `Timer`, `TimerHandle`, `redraw_after` | [timer](system/timer.md) | — | done |
| Subscriptions | one service following another; polling only while watched | `Service::subscribe`, `Subscription`, `Service::idle_stop` | [subscriptions](system/subscriptions.md) | — | done |
| Launching | starting programs without panics, each in its own systemd scope | `Launch`, `LaunchError` | [apps-and-processes](system/apps-and-processes.md#each-program-in-its-own-scope) | — | done |
| IPC | a socket per app-id, one copy per app, calling another app | `ipc_call`, `App::on_activate` | [ipc](system/ipc.md) | `examples/app_window.rs` | done |

## Groups

- **Compositor:** niri services and actions, each a `Service` like upstream's.
  [compositor/services.md](compositor/services.md).
- **Windows:** surfaces beyond layer windows, and what apps need around them.
  [popups](windows/popups.md), [app-windows](windows/app-windows.md),
  [activation](windows/activation.md), [clipboard](windows/clipboard.md),
  [portals](windows/portals.md), [config](windows/config.md).
- **Graphics:** what is drawn and how it moves. [blur](graphics/blur.md),
  [canvas](graphics/canvas.md), [text](graphics/text.md), [video](graphics/video.md),
  [animation](graphics/animation.md).
- **Input:** pointer and keyboard additions. [input](input/input.md),
  [text-input](input/text-input.md), [input-method](input/input-method.md).
- **System:** D-Bus and portal services. [battery](system/battery.md),
  [power-profile](system/power-profile.md), [bluetooth](system/bluetooth.md),
  [kdeconnect](system/kdeconnect.md), [tray](system/tray.md),
  [status-items](system/status-items.md), [adb](system/adb.md), [idle](system/idle.md),
  [polkit](system/polkit.md),
  [appearance](system/appearance.md), [audio](system/audio.md),
  [notifications](system/notifications.md), [apps-and-processes](system/apps-and-processes.md),
  [ipc](system/ipc.md), [rfkill](system/rfkill.md), [logind](system/logind.md),
  [clipboard-watcher](system/clipboard-watcher.md), [network](system/network.md),
  [lock-keys](system/lock-keys.md), [media](system/media.md).
- **Internals:** how the fork changes amane inside, and the resource plan.
  [architecture](internals/architecture.md) (sources, `on_track`, frame skip, GPU bounds, frozen
  windows), [memory](internals/memory.md) (what a process holds in GPU memory and RAM, the vello
  fork's shell-sized scratch buffers, how to measure), [optimization-plan](internals/optimization-plan.md).

## API cheatsheet

[api-cheatsheet.md](api-cheatsheet.md) has the fork's rows for upstream's
[cheatsheet](../src/cheatsheet.md).

## Other repos

- **Server side:** the [niri fork's docs](https://github.com/098765434/niri/blob/ocbk/docs/fork/README.md)
  cover the protocols and IPC these features use: blur shapes, vibrancy, materials, gesture
  events, `DesktopObscured`, `MoveColumnToIndex` by window id.
- **Main consumer:** [amane-shell's docs](https://github.com/098765434/amane-shell/blob/main/docs/README.md).

## Design history

Implemented specs, kept for their reasoning:

- [spec-presence-appearance.md](design/spec-presence-appearance.md): Idle and Appearance.
- [spec-write-own-layer-rules.md](design/spec-write-own-layer-rules.md): blur behind the bar and
  activation tokens.
