# Blur (fork addition)

A layer window can tell the compositor where to blur behind it: as plain
rectangles (`blur_region`) or as rounded, fading shapes, each optionally drawn
with a liquid-glass material (`blur_shapes`). The usage summary is in
[Layer windows](../src/layer_windows.md#blur-behind-the-window); this page covers
how it works.

```rust
LayerWindow::new()
    .width(200.0)
    .height(90.0)
    .blur_shapes(vec![
        BlurShape::new(8.0, 8.0, 164.0, 74.0, 28.0)
            .alpha(open)              // fades with the card
            .material("glass")        // if the compositor offers it
            .tint(Color::rgba(255, 255, 255, 40)),
    ])
```

## Blur region — `ext-background-effect-v1`

`LayerWindow::blur_region(Vec<InputArea>)` works like `input_region`:

- `None` (not set) leaves blur to the compositor's own rules; an empty list
  blurs nowhere.
- The protocol object is created the first time a view asks for a region, and
  dropped with the window.
- Only a changed region is sent.

(`src/wayland/background_effect.rs`, `src/wayland/update.rs`.)

## Blur shapes — `niri-blur-shapes-v1`

`LayerWindow::blur_shapes(Vec<BlurShape>)` sends rounded rectangles, each with
its own corner radius, alpha, material and tint. Shapes take the place of a blur
region. The protocol is private to the niri fork
([098765434/niri](https://github.com/098765434/niri), `docs/fork/blur-shapes.md`);
it is vendored at `protocols/niri-blur-shapes-v1.xml`.

| API | Meaning |
|---|---|
| `BlurShape::new(x, y, w, h, radius)` | a rounded rectangle, logical px |
| `.alpha(a)` | how much of the effect shows, 0–1 |
| `.material(name)` / `.tint(color)` | a compositor material and the colour it lays on |
| `.scale_about(s, origin)`, `.offset(dx, dy)`, `.fade(opacity)` | follow a layer that is scaled, moved or faded |
| `amane::blur_fades()` | whether the compositor draws shapes exactly (with fade) |
| `amane::material_available(name)` | whether the compositor announced that material |

**Fallback.** On a compositor without the protocol, shapes are turned into an
`ext-background-effect` region: strips that step in at the corners, rounded
inwards. A region can't fade, so a shape less than half visible is left out.

**Materials.** niri announces its material names (and again on config reload).
When they change, every window sends its shapes again so material ids stay
correct. An unknown name draws plain blur.

**Arrives with its buffer.** A skipped frame doesn't send a new blur with the
bare commit that requests the next frame; the last shown blur is put back, so
blur never runs ahead of the pixels it sits behind (`389fc29`).

(`src/blur.rs`, `src/wayland/blur_shapes.rs`, `src/wayland/update.rs`;
example: `examples/blur_shapes.rs`.)

## Drag release hook

`Rectangle::on_release(|pos| ..)` runs once the left button is let go after a
drag that started on the rectangle, wherever the pointer is then. A drag also
ends this way when the pointer leaves the window. While a drag is running, the
dragged target keeps the cursor shape until the drop (`2e9f8e0`). Before, a
resize handle that doesn't follow the pointer flipped the cursor on every
motion, and each `set_shape` made niri redraw every output.

(`src/widgets/rectangle/targets.rs`, `src/input/pointer.rs`.)
