# Text styling (fork addition)

Upstream's [Text](../../src/text.md) laid text out a letter at a time from swash advances: no
shaping, kerning or ligatures, and only a font's named weights. The fork lays it out with
[parley](https://github.com/linebender/parley), which does font matching, fallback, shaping, line
breaking, bidi and alignment, and adds the typography a design tool such as Penpot sets: italic,
line height, letter spacing, case, decorations, alignment, direction and variable font axes
(`06ead81`).

## Usage

```rust,ignore
use amane::{Case, Decoration, Text, TextAlign, TextDirection, Weight};

Text::new("Now Playing")
    .size(13.0)
    .weight(650)                        // any weight, exact on a variable font
    .letter_spacing(-0.2)               // pixels after every letter
    .case(Case::Upper)
    .decoration(Decoration::Underline)

Text::new(long)
    .wrap()
    .line_height(1.4)                   // a multiple of the size
    .align(TextAlign::Justify)
    .variation(b"wdth", 87.5)           // a variable font's axis
```

New methods on `Text`, next to upstream's `size`, `color`, `font`, `weight`, `wrap`, `elide`,
`max_lines` and `tight`:

| Method | Does | Default |
|---|---|---|
| `.italic(true)` | the font's italic, or a faked slant when it has none | `false` |
| `.line_height(multiple)` | the distance between lines as a multiple of the size, like CSS's unitless `line-height` | the font's own spacing |
| `.letter_spacing(pixels)` | extra room after every letter; negative draws letters closer | `0.0` |
| `.case(Case)` | `AsWritten`, `Upper`, `Lower`, or `Capitalize` (the first letter of every word, the rest left alone) | `AsWritten` |
| `.decoration(Decoration)` | `None`, `Underline` or `StrikeThrough`, drawn in the text's colour, measured from the font | `None` |
| `.align(TextAlign)` | `Start`, `Center`, `End`, or `Justify` (every line but a paragraph's last stretched to the full width); start and end follow the direction | `Start` |
| `.direction(TextDirection)` | `Auto` (from the first strong letter), `LeftToRight` or `RightToLeft` | `Auto` |
| `.variation(tag, value)` | a variable font's axis by its four-letter tag, like `b"wdth"` or `b"opsz"`; call it again for more axes | none |

`Weight` gains `Weight::Number(u16)`, any weight from 1 to 1000; `.weight(650)` takes a number
too. A variable font draws it exactly on its `wght` axis, other fonts pick the closest file.

Alignment only shows where text has spare width, which is text that wraps or elides inside a
wider area.

Text without a `line_height` measures as before: lines hug ascent to descent and step by the
font's own spacing. With one, the whole line boxes count, as in CSS and design tools.

`examples/text.rs` shows every style.

## Design / internals

- `src/widgets/text/style.rs` holds the `Style` (size, font, weight, italic, line height,
  letter spacing, case, decoration, align, direction, variations) and the public enums.
  `Style` hashes its floats by their bits, so it can key a cache.
- `src/widgets/text/layout.rs` lays text out with parley and turns each line into letters,
  decorations and a baseline. Elided lines are found by halving, since each try shapes the line
  again; the cut line ends in `…`.
- Layouts are cached per thread by content, style, rules and width, so text measured and drawn
  every frame is shaped once. The cache is emptied at 2048 layouts, so text that changes every
  frame, like a clock, can't grow it without end.
- amane still draws the letters through its own swash atlas. A glyph's face carries its
  variation coordinates and faked bold or italic, so the atlas and outline caches tell weights of
  one variable font apart.
- fontconfig is no longer a dependency; fontique finds fonts.
