# Shader effects (fork addition)

Upstream's [Shaders](../../src/shaders.md) run one fragment shader over a rectangle, and that
shader can't read any pixels. An **effect** is a chain of WGSL passes over what the rectangle
drew. Each pass reads the drawing or an earlier pass's result as a texture, and the last pass is
shown in the drawing's place. This is how a blur, glow, bevel or chrome finish is built from a
shape, text or an icon. The fork also reloads shader files when they're saved and reports compile
errors to the app.

## Usage

```rust,ignore
use amane::{Effect, Input, Rectangle};

Rectangle::new()
    .width(360.0)
    .height(110.0)
    .effect(
        Effect::new()
            .pass("shaders/blur_x.wgsl", [Input::Content])           // pass 0
            .pass("shaders/blur_y.wgsl", [Input::Pass(0)])           // pass 1
            .pass("shaders/glow.wgsl", [Input::Content, Input::Pass(1)]) // shown
            .values(vec![[8.0, 0.0, 0.0, 0.0], [0.54, 0.71, 0.98, 1.0]])
            .margin(24.0),
    )
    .child(text)
```

| Method | Does |
|---|---|
| `Effect::pass(path, inputs)` | adds a pass; `inputs` become `input0`..`input3` (see below) |
| `.values(rows)` | up to 64 rows of four numbers, read by every pass as `values` |
| `.margin(px)` | logical pixels the passes reach past the rectangle, for blurs and glows that spill out |
| `Rectangle::effect(effect)` | runs the rectangle's fill and children through it; `opacity` fades the result |
| `amane::shader_errors()` | each shader file that failed to load or compile, with naga's message pointing into that file |

A pass can read four kinds of input:

| Input | Is |
|---|---|
| `Input::Content` | the rectangle's fill and children, drawn on their own |
| `Input::Pass(n)` | what an earlier pass wrote |
| `Input::Backdrop` | what the window drew before the rectangle, under it and around it, so a surface can reflect or bend its surroundings |
| `Input::image(path)`, `Input::blurred_image(path, px)` | an image file at its own size, blurred once when it's decoded; clear until then |

The backdrop is copied before the result is laid down, which costs one canvas copy, and only
when a pass reads it. It holds only what *this window* drew. Inside a faded group or a clip that
holds the effect, it is just what that group drew, so keep the backdrop's fill inside the same
clip as the effect. Other windows and the wallpaper are never readable from a Wayland client.
For the wallpaper, read its file as an image and place it with the window's position on the
output.

`examples/effect.rs` blurs text and a pill into a glow. Run it from the project folder, because
the shader paths are relative to it.

## What a pass gets

The same inputs as a rectangle shader (`uv`, `size`, `time`, `values`, `placement`), plus:

```wgsl
@group(0) @binding(4) var<uniform> scale: f32;   // real pixels per logical pixel
@group(1) @binding(0) var input_sampler: sampler; // linear, clamped
@group(1) @binding(1) var input0: texture_2d<f32>; // .. input3
```

Declare `@location(1) canvas_uv: vec2<f32>` in `main` to get where the pixel is on the whole
canvas (0 to 1), and read inputs there with
`textureSampleLevel(input0, input_sampler, canvas_uv, 0.0)`. One real pixel is
`1.0 / placement.canvas.xy`, so a blur of `r` logical pixels reaches `r * scale` of them.

- **Content, backdrop and passes hold premultiplied color; images hold plain color.** Content is what amane drew. Passes before the last one
  are stored exactly as written, in `Rgba16Float`, so they can hand on fine steps or values past
  0 and 1.
- **The last pass returns plain, not premultiplied, color,** like a rectangle shader.
- `uv` and `size` cover the rectangle plus its margin.
- Unused inputs read as clear. Effect passes are WGSL only.
- As with rectangle shaders, a pass whose source contains the word `time` keeps the window
  drawing every frame. Drive slow changes from Rust through `values` and `request_frame`
  instead.

## Reloading and errors

The first time a shader file is drawn, amane watches its folder. Saving the file recompiles every
shader in every window and redraws. If the new source fails, amane keeps drawing the last good
version and puts the error in `shader_errors()`. A file that never compiled draws nothing; in an
effect, the drawing is shown without the effect instead. The error is also printed to stderr. A
missing file is an error, not a panic, for rectangle shaders too.

## The shader clock

Every shader's `time` is `amane::shader_time()`. `amane::pause_shader_time(Some(t))` holds it at
`t` seconds: shaders that read `time` stop redrawing every frame, and moving the held time
redraws once. `pause_shader_time(None)` lets the clock run on from where it was held.
`shader_time_paused()` says which. Use it for still screenshots and for scrubbing an animation.

## Frame stats

`amane::frame_stats(window) -> Option<FrameStats>` gives a window's last frames, averaged over up
to 30: `fps`, `view_ms`, `draw_ms` and `present_ms`. `present_ms` is the time to hand the frame to
the GPU and show it, not the GPU's own work. Pass a layer window's namespace, or a normal window's
name as `current_window()` gives it. Stats are kept whether `AMANE_FRAMES` is set or not.

## Shadow spread

`Shadow::spread(px)` works like CSS's spread. A drop shadow grows by it on every side, and its
corners round out by as much. An inner shadow's hole shrinks by it, so the shadow reaches further
in.

## How it works

`Command::Effect` is routed like a faded group (`gpu/paint.rs`): its commands draw onto a canvas
of their own. `gpu/effect.rs` then runs the passes, each into a pooled texture (`take_field` for
`Rgba16Float`, `take_canvas` for the last pass), and lays the result over the canvas. Image
inputs are loaded by the renderer, so the window waiting on a decode is the one woken. The GPU
uploads each image once and drops it after a frame that didn't read it. A clip that
holds an effect draws on its own canvas too, so the clip trims the result.

Shaders are cached per file and per kind (`Plain`, `Pass`, `Last`) in `gpu/shade.rs`. A global
generation in `graphics/shader_files.rs`, counted up by an inotify thread, marks them stale, and
`Gpu::shows` won't skip a frame that would draw a saved shader. Compiling runs inside a wgpu
validation error scope, so errors are caught, not printed by the uncaught-error handler.

Cost: one extra full-canvas texture per pass, plus the passes themselves, drawn only over the
area and its margin. Keep effects on a few elements, not on every list item (see the
glass-module measurements in the shell).
