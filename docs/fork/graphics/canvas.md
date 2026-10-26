# Canvas paths and shaders (fork additions)

What the fork adds to upstream's [Canvas](../../src/canvas.md) and [Shaders](../../src/shaders.md)
pages: SVG path data, placing and trimming a path, line joins, and 64 shader value rows instead
of 16.

## Usage

### Line joins

Every shape gets a fifth method from the `Shape` trait, next to `fill`, `stroke`, `cap` and
`opacity`:

```rust,ignore
.join(Join::Round)        // how corners look: Miter (the default), Round, or Bevel
```

### `Path::svg`, `place` and `trim`

| Method | Does | Default |
|---|---|---|
| `Path::svg(d)` | reads the `d` attribute of an SVG path, arcs included; `None` if it doesn't parse | — |
| `.place(x, y, scale)` | draws it scaled about `(0, 0)` and then moved, so one outline fits any size | in place |
| `.trim(start, end)` | draws only the part of the line between two fractions of its length: `0` is where it starts and `1` where it ends | all of it |

Animate `trim`'s two ends and the line draws itself on, or erases itself from the start, the way a
stroke clock changes its digits. An empty part draws nothing, not even a dot from a round cap.

```rust,ignore
// parse once and clone it each frame, parsing isn't free
let one = Path::svg("M18 24 L34 12 L34 88").unwrap();

one.clone()
    .place(20.0, 10.0, 1.5)
    .trim(0.0, drawn.value())
    .stroke(18.0, "#E8C99A")
    .cap(Cap::Round)
    .join(Join::Round)
```

`examples/stroke_clock.rs` is a counter that ticks like that.

### 64 shader value rows

`shader_values` passes up to 64 rows of 4 numbers from your view to the shader (upstream: 16), so
`values` is `array<vec4<f32>, 64>` in WGSL and `vec4 values[64]` in GLSL. Rows you don't pass are
0; more than 64 rows panics. That is 1 KiB of uniform, enough to light many shapes in one pass,
e.g. the shell's glass rims over a grid of modules.

## Design / internals

- `Path::svg`, `place` and `trim` are in `src/widgets/shape/path.rs`; the trimmed geometry is
  `Path::trimmed` in `src/graphics/path.rs`, and `Join` is `src/graphics/join.rs` (`17fca64`).
- A vello run draws into a texture the size of what it reaches, not the whole window (`6d2123b`),
  so an animated ring or clock digit costs its own size. See
  [Architecture](../internals/architecture.md#gpu-bounds).
- The row count is `VALUE_ROWS` (`src/graphics/renderer/effect.rs`), and the declarations are in `src/graphics/gpu/shader.rs` (`16480b2`).
