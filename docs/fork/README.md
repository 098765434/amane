# Fork docs

Docs for this fork's additions. Changes to upstream's guide pages live in [`docs/src/`](../src/);
the overview of the whole fork, with every commit, is [FORK.md](../../FORK.md).

| Doc | Covers | Status |
|---|---|---|
| [popups.md](windows/popups.md) | xdg-popups | implemented |
| [compositor.md](compositor/services.md) | `Windows`, `Gestures`, `Overview` services | implemented |
| [battery.md](system/battery.md) | `Battery` via UPower | implemented |
| [power-profile.md](system/power-profile.md) | `PowerProfile` service | implemented |
| [idle.md](system/idle.md) | `Idle` service, ScreenSaver inhibit | implemented |
| [appearance.md](system/appearance.md) | `Appearance` service | implemented |
| [audio.md](system/audio.md) | Audio device lists and switching | implemented |
| [notifications.md](system/notifications.md) | Notification timeout, hints, expire | implemented |
| [blur.md](graphics/blur.md) | Blur regions, blur shapes, drag release hook | implemented |
| [app-windows.md](windows/app-windows.md) | Normal windows for real apps: app-id, live settings, state, close veto, dialogs | phases 1–7 implemented, shell kit and settings app planned |
| [optimization-plan.md](internals/optimization-plan.md) | Resource optimization | planned |
| [spec-write-own-layer-rules.md](design/spec-write-own-layer-rules.md) | Blur behind the bar, activation tokens, layer rules | draft (untracked); blur part shipped |
| [spec-presence-appearance.md](design/spec-presence-appearance.md) | Idle and appearance design | draft (untracked); implemented |
