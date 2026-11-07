# Video (fork addition)

Moving pictures handed in from another thread: a decoder publishes each
frame to a `VideoFeed`, and a `Video` widget shows the newest one. Only the
windows showing that feed are drawn again for it.

## Usage

```rust
let feed = VideoFeed::new();

// on the decoder's thread, as each frame comes
feed.publish(VideoFrame {
    width,
    height,
    y,      // width × height bytes of brightness
    uv,     // (width / 2) × (height / 2) byte pairs of colour, rounded up
    colors: VideoColors::Bt709,
    full_range: false,
});

// in view(): the newest frame, whole and in the middle, with rounded corners
Video::new(&feed).radius(12.0)

// a pointer at `point` on a contained video `width` by `height`, as the frame's pixel
Video::frame_point(width, height, feed.size()?, point)
```

| | |
| --- | --- |
| `VideoFeed::new()`, `publish(frame)`, `clear()` | clones share one feed; any thread can publish; a frame whose planes don't match its size is dropped |
| `size()`, `frames()` | the newest frame's size, and how many came |
| `Video::new(&feed)` | contained by default; `.cover()`, `.radius(r)`, `.width(..)`, `.height(..)` |
| `Video::frame_point(..)` | maps a point on the widget to the frame, none over the bars |

## How it works

- Frames are **NV12**: a plane of brightness and a half-size plane of
  interleaved color. That's what hardware decoders hand out, and it's half the
  size of RGBA.
- Each feed keeps two textures on the GPU (`R8` and `Rg8`, no mipmaps). Each new
  frame is written over them in place with `write_texture`, and they're made
  again only when the size changes. A feed no window drew in a frame lets go of
  its textures.
- They're drawn as quad kind 8. The fragment shader turns BT.709 (or BT.601),
  video or full range, into RGB. It's always the quad pipeline, never vello. A
  turned or flipped video isn't drawn.
- `VideoFeed::publish` marks `Source::Video(id)`. The `Video` widget notes that
  source as it draws, so only those windows redraw.

## Limits

- A frame is copied once onto the GPU (about 3 MB for 1080p). Zero-copy dmabuf
  import from a hardware decoder isn't done.
- No HDR transfer functions; BT.2020 is drawn as BT.709.
