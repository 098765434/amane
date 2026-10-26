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
| Text styling | parley layout: italic, line height, spacing, case, decoration, align, direction, axes | `Case`, `Decoration`, `TextAlign`, `TextDirection`, `Weight::Number` | [text](graphics/text.md) | `examples/text.rs` | done |
| Animation | spring easing that keeps its speed, `retime` | `Easing::Spring`, `Animation::retime` | [animation](graphics/animation.md) | — | done |
| `on_track` | pointer motion that redraws nothing; also scroll source, modifiers, shortcuts, double clicks | `on_track`, `Scroll`, `modifiers` | [input](input/input.md) | — | done |
| Text input | selection, editing keys, undo, multiline, IME | `TextInput` | [text-input](input/text-input.md) | `examples/text_editor.rs` | done |
| Battery | UPower: state, warning, time left, peripherals | `Battery`, `PowerDevice` | [battery](system/battery.md) | `examples/system.rs` | done |
| Power profile | power-profiles-daemon | `PowerProfile` | [power-profile](system/power-profile.md) | — | done |
| Idle | ext-idle-notify levels and ScreenSaver inhibits | `Idle` | [idle](system/idle.md) | `examples/idle.rs` | done |
| Appearance | colour scheme, accent, contrast, reduced motion | `Appearance`, `Scheme` | [appearance](system/appearance.md) | `examples/appearance.rs` | done |
| Audio | device lists, kinds, default switching | `AudioDevice`, `AudioKind` | [audio](system/audio.md) | `examples/audio.rs` | done |
| Notifications | timeout, hints, icon path, expire | `Timeout`, `Notifications::expire` | [notifications](system/notifications.md) | `examples/notifications.rs` | done |
| Apps and processes | desktop entry id, categories, actions, MIME, opening files | `DesktopApp`, `DesktopAction` | [apps-and-processes](system/apps-and-processes.md) | `examples/apps.rs` | done |
| IPC | a socket per app-id, one copy per app, calling another app | `ipc_call`, `App::on_activate` | [ipc](system/ipc.md) | `examples/app_window.rs` | done |

## Groups

- **Compositor:** niri services and actions, each a `Service` like upstream's.
  [compositor/services.md](compositor/services.md).
- **Windows:** surfaces beyond layer windows, and what apps need around them.
  [popups](windows/popups.md), [app-windows](windows/app-windows.md),
  [activation](windows/activation.md), [clipboard](windows/clipboard.md),
  [portals](windows/portals.md), [config](windows/config.md).
- **Graphics:** what is drawn and how it moves. [blur](graphics/blur.md),
  [canvas](graphics/canvas.md), [text](graphics/text.md), [animation](graphics/animation.md).
- **Input:** pointer and keyboard additions. [input](input/input.md),
  [text-input](input/text-input.md).
- **System:** D-Bus and portal services. [battery](system/battery.md),
  [power-profile](system/power-profile.md), [idle](system/idle.md),
  [appearance](system/appearance.md), [audio](system/audio.md),
  [notifications](system/notifications.md), [apps-and-processes](system/apps-and-processes.md),
  [ipc](system/ipc.md).
- **Internals:** how the fork changes amane inside, and the resource plan.
  [architecture](internals/architecture.md) (sources, `on_track`, frame skip, GPU bounds, frozen
  windows), [optimization-plan](internals/optimization-plan.md).

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
