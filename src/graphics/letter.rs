use swash::FontRef;
use swash::scale::{Render, Source};
use swash::zeno::Format;

use crate::graphics::font;

// one letter turned into pixels: how much of each pixel it covers, 0 to 255
pub struct Letter {
    pub width: u32,
    pub height: u32,

    // where the pixels sit from the pen on the baseline
    pub left: f32,
    pub top: f32,

    pub coverage: Vec<u8>,
}

// none for a letter without pixels, like a space
pub fn rasterize(font: FontRef<'static>, id: u16, size: f32) -> Option<Letter> {
    font::with_context(|context| {
        let mut scaler = context
            .builder(font)
            .normalized_coords(font::coords(font))
            .size(size)
            .hint(true)
            .build();

        let image = Render::new(&[Source::Outline])
            .format(Format::Alpha)
            .render(&mut scaler, id)?;

        let placement = image.placement;

        if placement.width == 0 || placement.height == 0 {
            return None;
        }

        // swash counts top upward from the baseline, amane counts it downward
        Some(Letter {
            width: placement.width,
            height: placement.height,
            left: placement.left as f32,
            top: -placement.top as f32,
            coverage: image.data,
        })
    })
}
