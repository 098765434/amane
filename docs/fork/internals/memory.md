# Memory

What an amane process holds in GPU memory and RAM at idle, what each part costs, and what the
fork does about it. Each optimisation says what it cost, why, what changed, how to switch it
off, and how to measure it. The numbers are from a Ryzen AI 7 laptop with a Radeon 860M, a
2880×1800 panel at scale 1.75, and the amane-shell desktop (2026-10-26).

On an integrated GPU, "VRAM" is ordinary RAM that the firmware reserves at boot (here 4 GiB of
32), so it is already missing from what Linux sees. Lowering it pays off twice: usage past the
carve-out spills into GTT, which is regular RAM, and once the desktop fits well under 1 GiB the
BIOS carve-out can be lowered, handing the rest back.

## Measuring

A process's own GPU memory is in its `fdinfo` (readable for your own child processes, root for
others under `ptrace_scope 1`):

```sh
gpu() { awk '/drm-client-id/{id=$2} /drm-total-(vram|gtt):/{m[id SUBSEP $1]=$2*($3=="MiB"?1024:1)}
  END{for(k in m){split(k,a,SUBSEP); t[a[2]]+=m[k]} for(x in t) printf "  %s %.0f MiB\n", x, t[x]/1024}' /proc/$1/fdinfo/*; }
gpu $(pgrep -f examples/gradient)
```

Run an example beside the live shell with its own runtime dir, so it does not hand over to the
running instance:

```sh
T=$(mktemp -d /tmp/amXXXX)
XDG_RUNTIME_DIR=$T WAYLAND_DISPLAY=/run/user/1000/wayland-1 cargo run --example gradient
```

`/proc/PID/status` gives the CPU side: `RssAnon` is the process's own heap, `RssFile` is mostly
shared libraries (libLLVM alone is 29 MB, shared by every GPU process) and is cheap.

What a bare amane process costs on this machine, by what it draws:

| Example | RssAnon | VRAM | GTT | Goes through vello? |
|---|---|---|---|---|
| `rectangle` | 16 MB | 22 MB | 9 MB | no, quads only |
| `workspaces`, `players` | 15 MB | 38 MB | 15–18 MB | no |
| `gradient`, `shader`, `canvas` | 16–20 MB | **65 MB** (was 193 before the fork's vello) | 25–48 MB | yes |

## vello's scratch buffers (fork of vello)

**What it cost.** The first vello run in a process allocated about 130 MB of GPU scratch on this
machine, and up to 360 MB on a busy shell: `vello_encoding`'s `BufferSizes::new` hard-codes the
bump-allocated buffers (`tiles`, `lines`, `seg_counts`, `segments`, `ptcl`, `bin_data`,
`blend_spill`) at sizes "hand picked to accommodate the vello test scenes as well as paris-30k",
a street map of Paris with 30,000 paths. That is about 165 MB a set, and `vello::Renderer`'s
pool rounds every buffer up to a power-of-two size class and keeps it for the renderer's
lifetime. A shell draws a few hundred paths a frame. amane already shares one renderer per
process (`src/graphics/gpu/shared.rs`), so this was paid once per process, but it was still the
largest single item on the desktop. `RendererOptions` has no setting for it.

**What changed.** amane's `vello_encoding` comes from a fork,
[098765434/vello](https://github.com/098765434/vello) branch `ocbk`, one commit on top of tag
`v0.11.0` (the tree crates.io's 0.11.0 was published from). It sizes the buffers for a shell:

| Buffer | Upstream | Fork |
|---|---|---|
| `tiles`, `seg_counts`, `segments`, `lines` | 2²¹ each | 2¹⁸ each |
| `ptcl` | 2²³ | 2²¹ |
| `bin_data`, `blend_spill` | 2¹⁸, 2²⁰ | unchanged |

About 29 MB a set instead of 165. The fork's own
[docs/fork/README.md](https://github.com/098765434/vello/blob/ocbk/docs/fork/README.md) has the
bytes and the measurements. The patch is wired in `Cargo.toml`'s `[patch.crates-io]`; a patch
only applies from the root manifest, so a program that uses amane as a git dependency (the
shell does) repeats the same entry in its own `Cargo.toml`, and a nix build needs the fork's hash
in `cargoLock.outputHashes` (`flake.nix`).

**Why these sizes are safe.** An overflow is silent: the shaders set `BumpAllocators::failed`,
coarse and fine rasterisation exit early, and `render_to_texture` still returns `Ok`, so the
run comes out blank. The sizes were checked against what real scenes take, read back from the
GPU (below). The heaviest example, `canvas` (a stroked clock face and spirograph at 1216×320),
takes 36k lines, 37k segments and 940 tiles; `blur_shapes` at a full 2881×1769 output takes
11.7k tiles and 3.1k segments. Every limit is 3–7× above the largest value seen.

**Switching it off.** `VELLO_SCRATCH=upstream` in the environment restores upstream's sizes
without a rebuild. If a scene that came out blank fills in, it overflowed: report it with the
`scratch` line below.

**Measuring a scene.** The `vello-stats` cargo feature turns on vello's `debug_layers` and
draws every vello run the slow way, reading the bump counters back from the GPU. With
`AMANE_FRAMES=1` each run then prints what it took:

```sh
cargo run --features vello-stats --example canvas 2>&1 | grep ^scratch
scratch 1216x320 failed 0 tiles 940 lines 35954 seg_counts 37483 segments 37483 ptcl 0 bin 14 blend 0
```

`failed 1` is an overflow. Every run waits for the GPU, so frame times are not representative
with the feature on. The same build also prints a `wgpu buffers A+B textures A+B …` line after
each frame and a `released: …` line after each hide: `A` is what is alive, `B` the free slots in
wgpu's tables (cumulative, so a rising `B` with a steady `A` is churn, not a leak), and vello's
own log (the image atlas size) on stderr. (vello's `bump_estimate` feature gives a CPU-side bound instead; it
overshoots stroked scenes by 10×, so the readback is the one to trust.)

**Measured.** `gradient` beside the live shell: 193 MB of VRAM with upstream's sizes, 65 MB with
the fork's, stable over 25 s of animation. RSS unchanged. The shell itself, run beside the live
one with the desktop showing, the launcher and Quick Settings opened: 231 vello runs, none
failed. The largest took 38.7k tiles and 104k ptcl entries (Quick Settings, 686×1386), and 3.5k
segments and 1.1k lines (the widgets layer at full output). Every limit is 7× or more above.

**Later.** Upstream has moved to the sparse-strips renderers (`vello_cpu`, `vello_gpu`), which
have no such buffers. Moving amane to `vello_gpu` once it has feature parity and a stable API
retires the fork.

## A hidden window holds nothing on the GPU

**What it cost.** `hide()` (`src/wayland/update.rs`) took the buffer away and nothing else. The
swapchain, the canvas, the spare textures, last frame's kept vello results, the picture
textures, both glyph caches with the 1024² letter atlas, the shader inputs and the two vibrant
layers (each a window-sized surface with its own swapchain and canvas) all stayed. A full-output
window that had been shown once, like the launcher, kept 80–100 MB for the rest of the session,
and a window reopened kept more each time (the spares, below).

**What changed.** `Gpu::release()` (`src/graphics/gpu.rs`), called from `hide()`: the wgpu
surface is made again from the same wayland surface (an unconfigured surface holds no
swapchain; the next `draw()` configures it, since the kept size is 0), the spares, kept results,
images, glyphs, shader inputs and the quads' pictures and atlas are let go, and the window's
vibrant layers are dropped. The pipelines stay; they are small and showing again would make the
same. The letter atlas is now made on the first letter (`atlas.rs`), so a window that never
shows text never has one.

**Switching it off.** `AMANE_KEEP_HIDDEN=1`.

**Measured** on a second shell beside the live one (launcher opened and closed twice, then
Quick Settings), VRAM of the shell process:

| Step | before | after |
|---|---|---|
| start | 58 MB | 58 MB |
| launcher open | 317 | 317 |
| launcher closed | 235 | 154 |
| launcher open again | 349 | 285 |
| launcher closed again | 267 | 186 |
| Quick Settings open, then closed | 371, 354 | 225, 128 |

`release()` ends by waiting for the GPU once (`device.poll(wait)`): wgpu frees what was dropped
only when the device is polled, and a desktop that hid a window may draw nothing for a long
while, so without it the window's textures stayed until the next frame of any window.

What stays after the launcher closes (about 96 MB over the start) is partly vello's: its scratch
set (29 MB) and the pool buffers for the scene. **Open:** Quick Settings costs about 190 MB of
VRAM, 110 MB of GTT and 30 MB of heap to open, and about 120 MB of VRAM and all of the GTT
stay after it hides. With upstream's scratch sizes it costs 128 MB more, so the fork's sizing
holds; `AMANE_PRESENT=mailbox` makes no difference; vello's image atlas stays at 1×1; and
`vello-stats` reports 43 vello buffers and 7 more textures than before alive afterwards, none of
them a surface, spare, picture or atlas (those went). Whatever holds it is below wgpu's
accounting (its Vulkan allocator's blocks, or Mesa's swapchain images, which it gives FIFO four
of). The next step is a driver-level view (`VK_EXT_device_memory_report`, or RADV's
`RADV_DEBUG=info`) rather than more guessing.

## Spare textures that outlive their frame are dropped

**What it cost.** Finished window-sized textures (`spare` in `Gpu`) were kept for reuse and
only cleared on a resize, and reuse needs an exact size match. Partial vello runs round their
box to 64 px steps, so every animation left one spare per size it passed through, and a
reopened window grew by a few tens of MB each time (the table above).

**What changed.** `pool.rs` keeps two lists: what was given back this frame, and what was left
from the last. A spare no frame took since it was given back is dropped at the end of the next
frame; the canvas and the sizes a steady frame uses are taken every frame, so they stay.

**Switching it off.** `AMANE_KEEP_SPARES=1`.

## Fifo presentation, one frame of latency

**What it cost.** The swapchain preferred Mailbox with a latency of 2, which Mesa's Wayland
backend serves with four images, 20.7 MB each at full output. amane draws one frame per frame
callback and never one for a hidden or frozen window, so no frame ever waits on another.

**What changed.** `target.rs` asks for Fifo with `desired_maximum_frame_latency: 1`.

**Switching it off.** `AMANE_PRESENT=mailbox` restores the old choice, for a compositor that
stalls Fifo presents. On niri no stall was seen (launcher, Quick Settings and toast animations
run at their usual frame counts).

## An empty vibrant layer holds nothing

A vibrant window draws its translucent whites and blacks on two subsurfaces of its own
(`src/wayland/vibrancy.rs`), each with its own `Gpu`: a swapchain, a canvas and spares the size
of the window. A layer with nothing on it (a window with no translucent black, say) drew an
empty buffer and kept all that. It is now unmapped instead (a null buffer attached, synced with
the window's commit) and its `Gpu` released, until it shows something again. The layers also go
with the window when it hides (above).

Sizing a layer to its content rather than the window was considered and left: the surfaces that
are vibrant are the bar, the dock and the widgets, and the widgets surface is now the size of
its cards (the shell's `widgets/mod.rs`), so a content-sized layer would save a few tens of MB
for a translate-every-command pass. The frame log's `vello N` field (`AMANE_FRAMES=1`) says how
many times a frame went through vello; an idle frame should say 0.

## Small things

- `device.poll` is called without waiting after every present (`present.rs`), so what the last
  submit finished with (blur temporaries, upload staging, a released window's textures) is
  given back then, not at some later submit.
- vello compiles its shaders on one thread (`num_init_threads`), not one per CPU, at start.
- The fork's vello also retires atlas images unused for 120 passes and shrinks the atlas (the
  fork's docs). The shell's own surfaces don't reach the atlas (niri frosts them), but amane's
  blur regions and `draw_image` pictures do: `blur_shapes` at full output sits at 210 MB, most
  of it the 4096² atlas its two blur copies need.

## Planned

The rest of the idle-memory work, in order of payoff: content-sized vibrant layers, dropping the
vello renderer after an idle period (what the launcher leaves behind, above), sharing pipelines
and picture textures across windows, decoding images at their drawn size with a byte-capped
cache, and event-driven services in place of polls. Each lands with its own section here.
