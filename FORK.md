# Fork notes — amane

A fork of [amane](https://github.com/MystiaFin/amane), a declarative Rust Wayland-shell library,
maintained for the [amane-shell](https://github.com/098765434/amane-shell) bar. It adds
xdg-popups, compositor services that follow niri's gestures frame by frame, system services
(battery, power profiles, idle, appearance, audio devices, richer notifications), rounded
liquid-glass blur, and a number of input and animation improvements. It's a thin, rebasable
series on top of upstream; everything else is stock.

## Provenance

| | |
|---|---|
| Upstream | `https://github.com/MystiaFin/amane` (remote: `upstream`) |
| Fork point | upstream commit `72e23eb` (tag `fork-base`), the real upstream tree the fork was authored against |
| Fork branch | `ocbk` (upstream `main` is kept in sync for PRs) |
| The delta | `git log --oneline fork-base..ocbk` |

> The fork point is a real upstream commit, so `git log fork-base..HEAD` and `git range-diff` line
> up against genuine upstream history.

## At a glance

| Feature | Kind | Commits | Docs | Upstreamable? |
|---|---|---|---|---|
| [xdg-popups](#xdg-popups) | feature | `19d012d` | [popups](docs/fork/popups.md) | yes |
| [`Windows` service + compositor actions](#compositor-services) | feature | `19d012d` | [compositor](docs/fork/compositor.md) | yes |
| [`Gestures` / `Overview` services](#compositor-services) | feature (niri fork only) | `19d012d`, `14c6994` | [compositor](docs/fork/compositor.md) | no — needs the niri fork's IPC |
| [Battery via UPower + `PowerProfile`](#battery-and-power-profiles) | feature | `dd6f110` | [battery](docs/fork/battery.md), [power-profile](docs/fork/power-profile.md) | yes |
| [`Idle` service](#idle) | feature | `2953df9` | [idle](docs/fork/idle.md) | yes |
| [`Appearance` service](#appearance) | feature | `317943e` | [appearance](docs/fork/appearance.md) | yes |
| [Audio devices](#audio-devices) | feature + fix | `dedcda4` | [audio](docs/fork/audio.md) | yes |
| [Notification timeout and hints](#notifications) | feature | `7a16b39` | [notifications](docs/fork/notifications.md) | yes |
| [Blur regions and blur shapes](#blur) | feature | `db581d0`, `bcc53ac` | [blur](docs/fork/blur.md) | regions yes; shapes need the niri fork |
| [Drag release hook, drag cursor](#blur) | feature + fix | `db581d0`, `2e9f8e0` | [blur](docs/fork/blur.md#drag-release-hook) | yes |
| [Spring easing, retargeting](#animation) | feature | `ba43071`, `289f191` | this file only (not in [animation](docs/src/animation.md) yet) | yes |
| [Scroll source and finger lift](#input) | feature | `7f48e10` | [input](docs/src/input.md) | yes |
| [Keyboard modifiers, Ctrl chords](#input) | feature | `e3b2efb`, `bc9729e`, `c24533c` | [input](docs/src/input.md) | yes |
| [App categories, keywords, actions, `find_icon`](#apps) | feature | `0ebcfd2`, `7f48e10`, `bc9729e` | [apps](docs/src/apps.md) | yes |
| [64 shader value rows](#fixes-and-internals) | internal | `16480b2` | [shaders](docs/src/shaders.md) | yes |
| [Glyph atlas fills mid-frame](#fixes-and-internals) | fix | `389fc29` | — | yes |
| [Fork docs and housekeeping](#fixes-and-internals) | internal | `be25bb3`, `fe941a9`, `fe45a04`, `6bfdba8` | this file | no |

Work not on `ocbk` yet: see [In progress](#in-progress). All fork-only docs are in
[`docs/fork/`](docs/fork/README.md); changes to upstream's guide pages are in `docs/src/`.

## Features

### xdg-popups

A `Popup` type and `App::popup`, with layer-shell and nested-popup parenting, grab modes,
positioner edge collision (`ConstraintAdjustment`) and outside-click dismiss.

- **Files:** `src/popup.rs`, `src/wayland/popup.rs`; adds the `bitflags` dependency.
- **Review it:** `git diff fork-base..ocbk -- src/popup.rs src/wayland/popup.rs`

### Compositor services

- **`Windows`:** the focused window and the window list, plus a typed action surface: focus,
  close, fullscreen, float, move, column navigation, spawn, screenshot and overview.
- **`Gestures`** (niri only): live per-frame overview progress, view offset + active column, and
  workspace-switch progress, for compositor-synced animation. Requires the niri fork's
  gesture-progress IPC ([098765434/niri](https://github.com/098765434/niri)).
- **`Overview`** (niri only): the overview progress alone, so a view that only follows the
  overview isn't redrawn on every view scroll (`14c6994`).

- **Files:** `src/services/{windows,window_info,gestures,overview}.rs`, `src/compositor/`.
- **Review it:** `git show 19d012d 14c6994`

### Battery and power profiles

`Battery` reads from UPower when it answers at start-up. Updates are signal-driven, not polled,
and the sysfs reader stays as the fallback. New getters: `state`, `on_ac`, `warning`,
`time_to_empty`, `time_to_full`, `icon`, `peripherals`. The new `PowerProfile` service covers
power-profiles-daemon: `active`, `available`, `degraded`, `set`.

- **Files:** `src/services/battery/`, `src/services/power_profile.rs`.
- **Review it:** `git show dd6f110`

### Idle

Idle detection through the compositor's `ext-idle-notify-v1`, at levels the shell chooses. amane
also serves `org.freedesktop.ScreenSaver` (only if no one else owns it), so inhibits from apps
are honoured. `Bus` gains `Method::sender()`.

- **Files:** `src/services/idle.rs`, `src/wayland/idle.rs`; example `examples/idle.rs`.
- **Review it:** `git show 2953df9`

### Appearance

The desktop's light/dark preference (`Scheme`) and accent colour, read from the XDG Settings
portal and updated live from `SettingChanged`. Read-only.

- **Files:** `src/services/appearance.rs`; example `examples/appearance.rs`.
- **Review it:** `git show 317943e`

### Audio devices

`Audio::outputs()` / `inputs()` list each device with its `AudioKind` (built-in, speaker,
headphones, headset, display …). `set_output` / `set_input` switch the default. Also fixes an
abort when a thread that had used audio exited (the pulse context is now dropped before its
mainloop).

- **Files:** `src/services/audio/{device,pulse}.rs`.
- **Review it:** `git show dedcda4`

### Notifications

New accessors: `timeout()` (the sender's `expire_timeout`), `category()`, `desktop_entry()`,
`transient()`, and an `icon_path()` resolved from the icon's theme name.
`Notifications::expire(id)` closes a toast the shell is done with. amane still never closes a
notification by itself; the server now advertises `persistence`.

- **Files:** `src/services/notifications/`.
- **Review it:** `git show 7a16b39`

### Blur

- `LayerWindow::blur_region` sends `ext-background-effect-v1` rectangles.
- `LayerWindow::blur_shapes` sends rounded, fading shapes with a material and tint over the niri
  fork's `niri-blur-shapes-v1`. On other compositors it falls back to a stepped region.
- `blur_fades()` and `material_available()` let views adapt.
- `Rectangle::on_release` fires when a drag that started on the rectangle ends, and the dragged
  target keeps the cursor shape until the drop.

- **Files:** `src/blur.rs`, `src/wayland/{background_effect,blur_shapes,update}.rs`,
  `src/layer_window.rs`, `src/input/pointer.rs`, `src/widgets/rectangle/targets.rs`,
  `protocols/niri-blur-shapes-v1.xml`; example `examples/blur_shapes.rs`.
- **Review it:** `git show db581d0 bcc53ac 2e9f8e0`

### Animation

- `Easing::Spring { bounce }` is a damped spring that settles within the animation's duration
  (`bounce` 0 = no overshoot, up to 0.9). `Easing` loses `Eq` because of the `f32` field.
- A retargeted spring keeps its speed: `Animation::to` carries the current velocity into the
  new way through `Blend::along`, instead of starting from rest.
- `Animation::retime` reaches the same target over a new duration.
- `Easing::apply` is public.

- **Files:** `src/animation.rs`.
- **Review it:** `git show ba43071 289f191`

### Input

- **Scroll source:** `Scroll` carries `source` (`ScrollSource::Wheel`, `Finger`, `Continuous`,
  `Tilt`) and `stop`, from `wl_pointer`'s `axis_source` / `axis_stop`, plus `pixels()`. A
  handler can follow fingers one to one and settle when they lift.
- **Modifiers:** `modifiers()` reports Shift/Ctrl/Alt/Super held now. A Ctrl+letter chord
  arrives as `Key::Character(letter)` instead of `Other`, and a focused `TextInput` ignores
  command chords.

- **Files:** `src/input.rs`, `src/input/modifiers.rs`, `src/wayland/{pointer,keyboard}.rs`.
- **Review it:** `git show 7f48e10 e3b2efb`

### Apps

- `DesktopApp::categories()` / `keywords()` return the entry's freedesktop `Categories` and
  `Keywords`, for filtering and searching.
- `Apps::find_icon` exposes the icon-theme lookup for windows without a `.desktop` file.
- `DesktopApp::actions()` returns the `[Desktop Action …]` sections the entry's `Actions` lists
  (`DesktopAction`: `name()`, `exec()`, `icon()`, `launch()`), for an app's menu.

- **Files:** `src/services/apps/`.
- **Review it:** `git show 0ebcfd2 7f48e10`

## Fixes and internals

- `16480b2` gives a shader 64 value rows instead of 16 (1 KiB of uniform). This is enough to
  light many shapes in one pass, e.g. the shell's glass rims over a grid of modules.
- `389fc29` stops letters being dropped when the glyph atlas fills mid-frame: the queued letters
  are drawn, the atlas restarts, and the letter is placed again. A skipped frame also no longer
  sends a blur ahead of its buffer.
- `be25bb3` adds FORK.md and the README banner; `fe941a9` renames the branch to `ocbk`.
- `fe45a04` adds the optimization plan (see below).
- `6bfdba8`, `bc9729e`, `c24533c` add docs for the features above, in `docs/src/` and the
  cheatsheet.

## In progress

| Work | Where | Status |
|---|---|---|
| Resource optimization: cut idle wake-ups, per-frame CPU, VRAM, RAM and binary size | [`docs/fork/optimization-plan.md`](docs/fork/optimization-plan.md) | planned, not implemented |
| Blur behind the bar + activation-token launching (`xdg-activation-v1`) + `write_niri_rules` | [`docs/fork/spec-write-own-layer-rules.md`](docs/fork/spec-write-own-layer-rules.md) (untracked) | draft; the blur part shipped as [blur regions](#blur), while activation tokens and writing layer rules are not started |
| Presence and appearance | [`docs/fork/spec-presence-appearance.md`](docs/fork/spec-presence-appearance.md) (untracked) | draft spec, since implemented as [Idle](#idle) and [Appearance](#appearance) |
| Dock support: `WindowInfo::column` (niri's `WindowLayoutsChanged`), `Windows::move_to` (to a workspace without the focus, then `MoveColumnToIndex` by id, which needs the niri fork), `Rectangle::on_files_over` (files held over a widget, read on enter, accept or refuse per spot), `DesktopApp::{mime_types, opens, commands_with, launch_with}` with a shared-mime-info lookup (`services/apps/mime.rs`) | `ocbk` | done with tests |
| App windows: `app_id`, live title and min/max/maximized/fullscreen, `window_state()`, `on_close`, dialogs, `open_window_with`, `keep_alive` | [`docs/fork/app-windows.md`](docs/fork/app-windows.md), branch `app-windows` | phases 1 (windows), 2 (popups over windows, `open_popup`) 3 (key repeat, more keys, `on_shortcut`, `on_key_release`) and 4 (clipboard, primary selection, text drops, drag out) done and tested live on niri, later phases planned |

The local `power` branch is already merged into `ocbk` and can be deleted.

## Reviewing the fork

```sh
git log --oneline --stat fork-base..ocbk     # every fork commit and the files it touched
git diff --stat fork-base..ocbk              # the whole delta, by file
git diff fork-base..ocbk -- <paths>          # one feature (paths listed above)
git range-diff <old-base> <new-base> ocbk    # after a rebase: what changed in each commit
```

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

- Put new features in their own module.
- Keep edits to existing files surgical and additive.
- Give each feature a `docs/fork/<name>.md`, plus a row in [At a glance](#at-a-glance).

The goal is for the fork to shrink over time as pieces land upstream.
