# Fork docs

Docs for this fork's additions. Upstream's guide pages in [`docs/src/`](../src/) are unchanged;
the overview of the whole fork, with every commit, is [FORK.md](../../FORK.md).

| Doc | Covers | Status |
|---|---|---|
| [compositor/services.md](compositor/services.md) | `Workspaces`, `Windows`, `Overview`, `Gestures` services, compositor actions | implemented |
| [windows/popups.md](windows/popups.md) | xdg-popups | implemented |
| [windows/app-windows.md](windows/app-windows.md) | Normal windows for real apps: app-id, live settings, state, close veto, dialogs | implemented (phases 1–7 here, 8–9 in amane-shell) |
| [windows/clipboard.md](windows/clipboard.md) | Clipboard, primary selection, text drops, dragging out | implemented |
| [windows/config.md](windows/config.md) | `Config<T>` settings files | implemented |
| [windows/portals.md](windows/portals.md) | `FileDialog`, `Notify` | implemented |
| [graphics/blur.md](graphics/blur.md) | Blur regions, blur shapes, `Backdrop`, vibrancy, drag release hook | implemented |
| [graphics/canvas.md](graphics/canvas.md) | `Path::svg`, `place`, `trim`, line joins, 64 shader rows | implemented |
| [input/input.md](input/input.md) | Drag release, files over, scroll source, double clicks, keys, modifiers, shortcuts | implemented |
| [input/text-input.md](input/text-input.md) | Selection, editing keys, undo, multiline, IME | implemented |
| [system/battery.md](system/battery.md) | `Battery` via UPower | implemented |
| [system/power-profile.md](system/power-profile.md) | `PowerProfile` service | implemented |
| [system/idle.md](system/idle.md) | `Idle` service, ScreenSaver inhibit | implemented |
| [system/appearance.md](system/appearance.md) | `Appearance` service | implemented |
| [system/audio.md](system/audio.md) | Audio device lists and switching | implemented |
| [system/notifications.md](system/notifications.md) | Notification timeout, hints, expire | implemented |
| [system/apps-and-processes.md](system/apps-and-processes.md) | Desktop entry fields, opening files, `spawn_activated` | implemented |
| [system/ipc.md](system/ipc.md) | A socket per app-id, single instance, `ipc_call` | implemented |
| [internals/architecture.md](internals/architecture.md) | Fork additions to ARCHITECTURE.md | implemented |
| [internals/optimization-plan.md](internals/optimization-plan.md) | Resource optimization | partly implemented |
| [api-cheatsheet.md](api-cheatsheet.md) | Fork-only cheatsheet rows | — |
| [design/spec-write-own-layer-rules.md](design/spec-write-own-layer-rules.md) | Blur behind the bar, activation tokens, layer rules | implemented (layer rules in amane-shell) |
| [design/spec-presence-appearance.md](design/spec-presence-appearance.md) | Idle and appearance design | implemented |
