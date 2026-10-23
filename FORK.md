# Fork notes — amane

A fork of [amane](https://github.com/MystiaFin/amane), a declarative Rust Wayland-shell library.
It adds **xdg-popups** and **compositor services** (a `Windows` service + a niri `Gestures` service)
so a shell can drive menus/tooltips and animate in lockstep with the compositor. The fork is a thin,
rebasable series on top of upstream — everything else is stock.

## Provenance

| | |
|---|---|
| Upstream | `https://github.com/MystiaFin/amane` (remote: `upstream`) |
| Fork point | upstream commit `72e23eb` (tag `fork-base`) — the real upstream tree the feature was authored against |
| Fork branch | `ocbk` (upstream `main` is kept in sync for PRs) |

> The fork point is a real upstream commit, so `git log fork-base..HEAD` and `git range-diff` line
> up against genuine upstream history.

## What this fork adds

A single feature commit on top of `fork-base`:

- **xdg-popup support** — `Popup` type, `App::popup`, layer-shell and nested-popup parenting, grab
  modes, positioner edge-collision (`ConstraintAdjustment`), outside-click dismiss.
  (`src/popup.rs`, `src/wayland/popup.rs`; adds the `bitflags` dependency.)
  See [docs/popups.md](docs/popups.md).
- **`Windows` compositor service + typed action surface** — focused window / window list, and
  focus/close/fullscreen/float/move/column-nav/spawn/screenshot/overview actions.
  (`src/services/windows.rs`, `src/services/window_info.rs`.) See [docs/compositor.md](docs/compositor.md).
- **`Gestures` service** (niri only) — live per-frame overview progress, view offset + active
  column, and workspace-switch progress, for compositor-synced animation.
  (`src/services/gestures.rs`.) Requires the niri gesture-IPC fork
  ([098765434/niri](https://github.com/098765434/niri)).

- **`Battery` via UPower + `PowerProfile` service** — peripheral batteries, warning level, time
  estimates and AC state from UPower (sysfs kept as the fallback), and power-profiles-daemon's
  power-saver / balanced / performance. Both are signal-driven, not polled.
  (`src/services/battery/`, `src/services/power_profile.rs`.) See [docs/battery.md](docs/battery.md)
  and [docs/power-profile.md](docs/power-profile.md).

- **`Idle` service** — idle detection through the compositor's `ext-idle-notify-v1` at
  shell-chosen levels, and amane serving `org.freedesktop.ScreenSaver` (own-if-absent) so app
  inhibits are honoured. (`src/services/idle.rs`, `src/wayland/idle.rs`.) See
  [docs/idle.md](docs/idle.md).

- **`Appearance` service** — the desktop's light/dark preference (`Scheme`) and accent colour from
  the XDG Settings portal, updated live from `SettingChanged`. Read-only.
  (`src/services/appearance.rs`.) See [docs/appearance.md](docs/appearance.md).

## Consumed by

The [`amane-shell`](https://github.com/098765434/amane-shell) bar, as a git dependency pinned to a
`rev` on this branch. For local co-development the shell carries a git-ignored `.cargo/config.toml`
with `paths = ["../amane"]`, redirecting the pinned dep to a sibling checkout.

> The upstream `amane` *CLI* uses its own bundled library copy in `~/.cache/amane/library` — not
> this fork.

## Updating to newer upstream

The fork keeps its commits on top of the fork point, so updating is a rebase:

```sh
git fetch upstream
git log --oneline fork-base..HEAD          # the fork's delta
git rebase upstream/main                    # re-apply onto newer upstream
git range-diff fork-base upstream/main HEAD # spot commits upstream already has
git rebase --skip                           # for each one range-diff shows as upstreamed
```

Then move the `fork-base` tag to the new base and bump the consuming shell's `rev` +
`outputHashes`.

## Keeping the fork light

New features in their own module; surgical additive edits to existing files; a `docs/<name>.md` per
feature. The goal is to shrink over time as pieces land upstream.
