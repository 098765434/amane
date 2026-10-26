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

`failed 1` is an overflow. `ptcl` is not counted by vello 0.11, so it reads 0. Every run waits
for the GPU, so frame times are not representative with the feature on. (vello's `bump_estimate`
feature gives a CPU-side bound instead; it overshoots stroked scenes by 10×, so the readback is
the one to trust.)

**Measured.** `gradient` beside the live shell: 193 MB of VRAM with upstream's sizes, 65 MB with
the fork's, stable over 25 s of animation. RSS unchanged.

**Later.** Upstream has moved to the sparse-strips renderers (`vello_cpu`, `vello_gpu`), which
have no such buffers. Moving amane to `vello_gpu` once it has feature parity and a stable API
retires the fork.

## Planned

The rest of the idle-memory work, in order of payoff: releasing a hidden window's GPU resources
(`hide()` keeps its swapchain, canvas, spare textures and caches), dropping spare textures that
outlive their frame, Fifo presentation with one frame of latency, content-sized vibrant layers,
dropping the vello renderer after an idle period, sharing pipelines and picture textures across
windows, decoding images at their drawn size with a byte-capped cache, and event-driven
services in place of polls. Each lands with its own section here.
