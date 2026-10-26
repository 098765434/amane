# Maintaining the fork

How the fork is kept on top of upstream, which commits make up each feature, and the housekeeping
rules. What each feature does is in the [index](README.md).

## Provenance

| | |
|---|---|
| Upstream | `https://github.com/MystiaFin/amane` (remote: `upstream`) |
| Fork point | upstream commit `72e23eb` (tag `fork-base`), the real upstream tree the fork was authored against |
| Fork branch | `ocbk` (upstream `main` is kept in sync for PRs) |
| The delta | `git log --oneline fork-base..ocbk` |

The fork point is a real upstream commit, so `git log fork-base..HEAD` and `git range-diff` line
up against genuine upstream history.

Branches: `ocbk` is the only work branch. The local `power` branch is already merged into `ocbk`
and can be deleted. There is no `app-windows` branch; that work was done on `ocbk`.

## Upstream files stay as they were

All fork documentation lives in `docs/fork/`. Upstream's mdBook (`docs/src/`) and
`ARCHITECTURE.md` are byte-identical to `fork-base`; the fork's additions to them are in
[`docs/fork/`](README.md) instead. Only two upstream-adjacent files change: a one-line banner in
`README.md`, and the root `FORK.md` stub. Check it with:

```sh
git diff --stat fork-base HEAD -- docs/src ARCHITECTURE.md   # prints nothing
```

So a rebase only ever conflicts on `FORK.md` and the README banner line, plus the fork's code.
The crate doc at the top of `src/lib.rs` is a few fork lines linking here.

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
| vello's scratch buffers sized for a shell (fork of vello), `vello-stats` | `6fd7bd5` | `Cargo.toml` (`[patch.crates-io]`), `flake.nix`, `src/graphics/gpu/flush.rs`; [memory](internals/memory.md) | the sizing belongs upstream in vello, not amane |
| Fork docs and housekeeping | `be25bb3`, `fe941a9`, `fe45a04`, `6bfdba8`, `033b216`, `81aa6c4`, and the `docs(fork)` commits | `FORK.md`, `docs/fork/` | no |

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
