# Maintaining the fork

How the fork is kept on top of upstream, which commits make up each feature, and the housekeeping
rules. What each feature does is in the [index](README.md).

## Provenance

| | |
|---|---|
| Upstream | `https://github.com/MystiaFin/amane` (remote: `upstream`) |
| Fork point | upstream commit `72e23eb` (tag `fork-base`), the real upstream tree the fork was authored against |
| Fork branch | `ocbk`; the fork's `main` mirrors upstream's `main` (fast-forward it: `git push origin upstream/main:main`) |
| The delta | `git log --oneline fork-base..ocbk` |

The fork point is a real upstream commit, so `git log fork-base..HEAD` and `git range-diff` line
up against genuine upstream history.

Branches: `ocbk` is the only work branch, and `main` only follows upstream. Anything else is
merged into `ocbk` and can be deleted: the local `power` branch and `origin/gesture-services`
both are.

## Upstream files stay as they were

All fork documentation lives in `docs/fork/`. Upstream's mdBook (`docs/src/`) and
`ARCHITECTURE.md` are byte-identical to `fork-base`; the fork's additions to them are in
[`docs/fork/`](README.md) instead. Only two upstream-adjacent files change: a one-line banner in
`README.md`, and the root `FORK.md` stub. Check it with:

```sh
git diff --stat fork-base HEAD -- docs/src ARCHITECTURE.md   # prints nothing
```

So the docs only ever conflict on `FORK.md` and the README banner line. The code is another
matter: the fork rewrites a good part of some upstream files, and those are where a rebase
conflicts. By lines changed, the largest are `src/widgets/text_input/field.rs`,
`src/graphics/gpu/paint.rs`, `src/graphics/gpu/quads/draw.rs`, `src/compositor/niri.rs`,
`src/wayland/dropped.rs`, `src/graphics/path.rs`, `src/input/pointer.rs`,
`src/graphics/gpu/quads/batch.rs`, `src/widgets/scroll_area/offsets.rs`,
`src/widgets/text_input.rs`, `src/graphics/font.rs`, `src/animation.rs` and
`src/wayland/normal.rs`; and `Cargo.lock` conflicts every time (take upstream's, then let cargo
add the fork's crates back). List them again with:

```sh
git diff --stat fork-base HEAD -- src Cargo.lock | sort -t'|' -k2 -rn | head -20
```

The crate doc at the top of `src/lib.rs` is a few fork lines linking here.

## Lints

The lints in `Cargo.toml` (`[lints.rust]`, `[lints.clippy]`) are ones the whole crate already
passes, upstream's code included. A lint upstream's code fails is allowed there for the crate,
with the reason beside it (`needless_range_loop`, for upstream's image filters), rather than with
an attribute in an upstream file, which a rebase would have to carry.

The fork's own files hold themselves to more: each module file the fork added starts with

```rust
#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo)]
```

and its submodules inherit it. Give a new fork module the same line. `clippy.toml` allows
`unwrap` and `dbg!` in tests. CI (`.github/workflows/fork.yml`) runs clippy with `-D warnings`.

## Commit map

| Feature | Commits | Files | Upstreamable? |
|---|---|---|---|
| xdg-popups | `19d012d`, `b485d59` | `src/popup.rs`, `src/wayland/popup.rs`; adds `bitflags` | yes |
| `Windows` service + compositor actions | `19d012d`, `c029184` | `src/services/{windows,window_info}.rs`, `src/compositor/` | yes; `move_to` needs the niri fork |
| `Gestures`, `Overview`, `WorkspaceSwitch`, `Desktop`, frozen windows | `19d012d`, `14c6994`, `916aecd` | `src/services/{gestures,overview,workspace_switch,desktop}.rs`, `src/layer_window.rs`, `src/wayland/redraw.rs` | no, needs the niri fork's IPC |
| Battery via UPower, `PowerProfile` | `dd6f110` | `src/services/battery/`, `src/services/power_profile.rs` | yes |
| `Idle` | `2953df9` | `src/services/idle.rs`, `src/wayland/idle.rs`; `examples/idle.rs` | yes |
| `Appearance` | `317943e`, `b1187e6` | `src/services/appearance.rs`; `examples/appearance.rs` | yes |
| Audio devices (and a pulse teardown fix) | `dedcda4` | `src/services/audio/{device,pulse}.rs` | yes |
| Notification timeout, hints, expire | `7a16b39` | `src/services/notifications/` | yes |
| Blur regions, blur shapes, `Backdrop` | `db581d0`, `bcc53ac`, `2ea1d46`, `e71c91e` | `src/blur.rs`, `src/services/backdrop.rs`, `src/wayland/{background_effect,blur_shapes,update}.rs`, `protocols/niri-blur-shapes-v1.xml`; `examples/blur_shapes.rs` | regions yes; shapes need the niri fork |
| Drag release hook, drag cursor | `db581d0`, `2e9f8e0` | `src/input/pointer.rs`, `src/widgets/rectangle/targets.rs` | yes |
| Vibrancy | `c74990b`, `6ef0c4e`, `1ba9c2d` | `src/graphics/vibrancy.rs`, `src/wayland/vibrancy.rs`, `protocols/niri-vibrancy-v1.xml` | no, needs the niri fork |
| Paths: `Path::svg`, `place`, `trim`, joins | `17fca64` | `src/widgets/shape/path.rs`, `src/graphics/{path,join}.rs`; `examples/stroke_clock.rs` | yes |
| 64 shader value rows | `16480b2` | `src/graphics/renderer/effect.rs`, `src/graphics/gpu/shader.rs` | yes |
| Shader effects (backdrop and image inputs), shader hot reload and errors | `d421b31`, `254c8ee`, `966ff75` | `src/style/effect.rs`, `src/graphics/{shader_files.rs,gpu/effect.rs,gpu/shade.rs,gpu/shader.rs}`; `examples/effect.rs` | yes |
| Parley text | `06ead81` | `src/widgets/text.rs`, `src/widgets/text/{style,layout}.rs`, `src/graphics/font.rs`; `examples/text.rs` | yes |
| Spring easing, retargeting, `retime` | `ba43071`, `289f191`, `527c538` | `src/animation.rs` | yes |
| Scroll source, modifiers, Ctrl chords | `7f48e10`, `e3b2efb`, `bc9729e`, `c24533c` | `src/input.rs`, `src/input/modifiers.rs`, `src/wayland/{pointer,keyboard}.rs` | yes |
| `on_track` | `6b0099b` | `src/widgets/rectangle/targets.rs`, `src/input/pointer.rs` | yes |
| Apps: categories, keywords, actions, id, MIME, `find_icon` | `0ebcfd2`, `7f48e10`, `d9d3d46`, `c029184`, `3c3d56b` | `src/services/apps/` | yes |
| App windows, keyboard, clipboard, text input, single instance, activation, platform helpers | `31a3eb6`, `b485d59`, `1ac9e4c`, `72b699e`, `21bca54`, `82337f2`, `76c02f1`, `301b3ae`, `7c36e11` | `src/window.rs`, `src/wayland/{normal,clipboard,text_input,activation}.rs`, `src/{clipboard,config,portal,notify}.rs`, `src/widgets/text_input/`; `examples/{app_window,text_editor}.rs` | yes |
| Render and redraw work | `e56b843`, `6d2123b`, `3f57058`, `527c538`, `45e0edc`, `0856be2`, `65f1d33` | `src/graphics/gpu/{present,bounds}.rs`, `src/graphics/renderer.rs`, `src/changes.rs`, `src/services/{network,write,windows}.rs` | yes |
| Glyph atlas filling mid-frame | `389fc29` | `src/graphics/gpu/{atlas,paint}.rs`, `src/wayland/{redraw,update}.rs` | yes |
| vello's scratch buffers sized for a shell (fork of vello), `vello-stats` | `6fd7bd5`, `3e9d18c` | `Cargo.toml` (`[patch.crates-io]`), `flake.nix`, `src/graphics/gpu/flush.rs`; [memory](internals/memory.md) | the sizing belongs upstream in vello, not amane |
| Hidden windows release their GPU memory, spares age out, Fifo presents | `3e9d18c` | `src/graphics/gpu.rs`, `src/graphics/gpu/{pool,present,target,atlas,quads}.rs`, `src/wayland/update.rs`; [memory](internals/memory.md) | yes |
| `vello N` in the frame log; an empty vibrant layer unmaps and releases; a hide waits for the gpu; wgpu counts under `vello-stats` | `767c1d0`, `6dbdb9f`, `30e46a3` | `src/timing.rs`, `src/wayland/{redraw,vibrancy}.rs`, `src/graphics/gpu.rs`, `src/graphics/gpu/present.rs` | the log and the wait yes; the layer needs the niri fork |
| vello's scratch pool let go when idle (`AMANE_VELLO_IDLE`), fork of vello's `vello` crate too | `efa3bfc` | `Cargo.toml` (`[patch.crates-io]`), `flake.nix`, `src/wayland/idle_gpu.rs`, `src/graphics/gpu/shared.rs`; [memory](internals/memory.md) | the pool ageing belongs upstream in vello |
| Drop shadows drawn by the quads (`AMANE_VELLO_SHADOWS`) | `9d1df38` | `src/graphics/gpu/quads.wgsl`, `src/graphics/gpu/quads/{batch,draw}.rs`, `src/graphics/gpu/paint.rs`; [memory](internals/memory.md) | yes |
| Arcs, lines and areas under a line drawn by the quads (`AMANE_VELLO_SHAPES`); paths know when they are one arc | `51a14a1` | `src/graphics/path.rs`, `src/graphics/gpu/quads.wgsl`, `src/graphics/gpu/quads/{batch,draw}.rs`, `src/graphics/gpu/paint.rs`; [memory](internals/memory.md) | yes |
| Quads: small filled outlines, two-stop gradients | `8d099e5` | `src/graphics/gpu/quads.wgsl`, `src/graphics/gpu/quads/{batch,draw,pipeline}.rs`, `src/graphics/gpu/paint.rs` | yes |
| Shadow spread, a pausable shader clock, frame stats; shader reloads follow links | `8ed7f16`, `a1c6311` | `src/style/shadow.rs`, `src/graphics/shader_files.rs`, `src/graphics/gpu/{shade,present}.rs` | yes |
| Colour emoji: colour fonts' letters drawn as pictures | `6fbea6e` | `src/graphics/{image,letter}.rs`, `src/graphics/renderer/text.rs` | yes |
| ScrollArea coasting after a flick, eased wheel steps | `54bf6f4` | `src/widgets/scroll_area/{offsets,targets}.rs` | yes |
| Layer windows: `Zone::Thickness`; settings go with the frame's commit and clicks survive a redraw; an app's window comes back after its monitor goes; windows put on a monitor, popups under the pointer's monitor | `8320df7`, `2e06481`, `d89a557`, `9192b70` | `src/layer_window.rs`, `src/monitor.rs`, `src/wayland/{layer,output,windows,redraw,update,compositor}.rs` | yes |
| Drops read off the event loop | `ba5faf7` | `src/wayland/{dropped,wake}.rs` | yes |
| D-Bus: errors with their reason, one sender's signals, owner changes, error replies | `a0ef626` | `src/dbus/{bus,error,method,signal}.rs` | yes |
| Battery on AC from the adapter | `ac012d8` | `src/services/battery.rs` | yes |
| Bluetooth: BlueZ's signals, operations and failures; the pairing agent | `dabb319` | `src/services/bluetooth/{actions,adapter,agent,errors,model}.rs` | yes |
| Tray: StatusNotifierItem watcher and host, dbusmenu menus | `168413e` | `src/services/tray/`, `src/dbus/signal.rs` | yes |
| Status items and activities: a client of the bar's `org.amane.StatusBar1` | `d84c88c` | `src/status_item.rs`, `src/status_item/{client,protocol}.rs`; `examples/status_item.rs` | no, the bar's own interface |
| Polkit: the session's authentication agent | `db14388` | `src/services/polkit.rs`, `src/services/polkit/agent.rs` | yes |
| KDE Connect: phones, text messages, contacts, attachments, file and text sharing | `b5be6de`, `60c969e`, `e1ea6ab`, `8a19284`, `711ed5f`; docs `613d918` | `src/services/kdeconnect/` | yes |
| Video frames, adb, the phone's address and notifications | `af0f854` | `src/video.rs`, `src/widgets/video.rs`, `src/graphics/gpu/quads/video.rs`, `src/services/adb.rs`; adds `ffmpeg-next` | the video yes; adb is the shell's |
| Input method, secure fill, passwords that leave no copies | `74b5ddb` | `src/wayland/{input_method_v2,secure_fill}.rs`, `src/services/focused_field.rs`, `protocols/niri-secure-fill-v1.xml`; `examples/input_method.rs` | the input method yes; secure fill needs the niri fork |
| MimeApps: default apps the way xdg-open picks them | `1d0bd3e` | `src/services/apps/{defaults,mime}.rs` | yes |
| `WindowInfo::fullscreen` and `::workspace` | `00b7103` | `src/services/window_info.rs`, `src/compositor/niri.rs` | no, needs the niri fork's `is_fullscreen` |
| Brightness: SDR white in place of the backlight while the panel is in HDR | `5c57d28` | `src/services/brightness.rs`, `src/compositor/niri.rs` | no, needs the niri fork's HDR |
| Timers, subscriptions, services that stop while unwatched | `e05ba14` | `src/timer.rs`, `src/services/{subscribe,store,write}.rs`, `src/services.rs`, `src/wayland/windows.rs`; [timer](system/timer.md), [subscriptions](system/subscriptions.md) | yes |
| One niri event stream, `amane::niri`, `Features`, `KeyboardLayouts` | `dd6ae32` | `src/compositor/niri/{stream,client}.rs`, `src/compositor/niri.rs`, `src/services/keyboard_layouts.rs` | the stream and client yes; `features()` is the niri fork's |
| `Launch`: no panics, each program in its own systemd scope | `b2f770a` | `src/launch.rs`, `src/process.rs`, `src/wayland/activation.rs` | yes |
| uevents, `Rfkill`, `Logind`, `ClipboardWatcher`, `LockKeys`, `Casts`, NetworkManager signals and profiles, fewer rescans | `9d953dc` | `src/services/{uevent,rfkill,logind,clipboard_watcher,lock_keys,casts}.rs`, `src/services/network/connections.rs`, `src/services/apps/fingerprint.rs`, `src/services/{brightness,network}.rs`; [rfkill](system/rfkill.md), [logind](system/logind.md), [clipboard-watcher](system/clipboard-watcher.md), [network](system/network.md), [lock-keys](system/lock-keys.md) | yes; `LockKeys` and `OutputsChanged` use the niri fork's events where it has them |
| CI, lints and formatting | `18ddea4`, `14eb0dd`, `db47c38`, `01180f7` | `.github/workflows/fork.yml`, `Cargo.toml` (`[lints]`), `clippy.toml`, the fork's files | no |
| Fork docs and housekeeping | `be25bb3`, `fe941a9`, `fe45a04`, `6bfdba8`, `033b216`, `81aa6c4`, and the `docs(fork)` commits; `1d60d4e` and others edited `ARCHITECTURE.md`, reset to upstream's by `7d257a6` | `FORK.md`, `docs/fork/` | no |

`389fc29` stops letters being dropped when the glyph atlas fills mid-frame: the queued letters
are drawn, the atlas restarts, and the letter is placed again. A skipped frame also no longer
sends a blur ahead of its buffer.

## Reviewing the fork

```sh
git log --oneline --stat fork-base..ocbk     # every fork commit and the files it touched
git diff --stat fork-base..ocbk              # the whole delta, by file
git diff fork-base..ocbk -- <paths>          # one feature (paths in the commit map)
git range-diff <old-base> <new-base> ocbk    # after a rebase: what changed in each commit
```

## Consumed by

The [`amane-shell`](https://github.com/098765434/amane-shell) bar, as a git dependency pinned to a
`rev` on this branch. For local co-development the shell carries a git-ignored `.cargo/config.toml`
with `paths = ["../amane"]`, redirecting the pinned dep to a sibling checkout.

The upstream `amane` *CLI* uses its own bundled library copy in `~/.cache/amane/library`, not
this fork.

## Updating to newer upstream

The fork keeps its commits on top of the fork point, so updating is a rebase:

```sh
git fetch upstream
git log --oneline fork-base..HEAD          # the fork's delta
git rebase upstream/main                    # re-apply onto newer upstream
git range-diff fork-base upstream/main HEAD # spot commits upstream already has
git rebase --skip                           # for each one range-diff shows as upstreamed
```

Then move the `fork-base` tag to the new base, check that `docs/src` and `ARCHITECTURE.md` still
match it, and bump the consuming shell's `rev` + `outputHashes`.

## Keeping the fork light

- Put new features in their own module.
- Keep edits to existing files surgical and additive.
- Document each feature under `docs/fork/<area>/`, starting with **Usage**, then
  **Design / internals**, and add a row to the [index](README.md). Never edit `docs/src/` or
  `ARCHITECTURE.md`.
- Implemented specs move to [`design/`](README.md#design-history) with a "Status: implemented"
  header.

The goal is for the fork to shrink over time as pieces land upstream.
