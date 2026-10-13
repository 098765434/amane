use std::cell::RefCell;
use std::collections::HashMap;

use swash::FontRef;

use crate::graphics::{Area, BezierPath, Color, Outline, Renderer, Transform, font};

use super::Command;

thread_local! {
    // each letter's outline, read from the font once; swash gives every font its own key
    static OUTLINES: RefCell<HashMap<(u64, u16), Option<BezierPath>>> =
        RefCell::new(HashMap::new());
}

impl Renderer {
    /*
     * letters are drawn as small cached pictures, which the gpu handles far
     * faster than outlines; only rotated or stretched text needs the outlines
     */
    pub fn text(
        &mut self,
        content: &str,
        font: FontRef<'static>,
        size: f32,
        color: Color,
        area: Area,
    ) {
        match self.transform.even_scale() {
            Some(scale) => self.text_pictures(content, font, size * scale, color, area),
            None => self.text_outlines(content, font, size, color, area),
        }
    }

    /*
     * the line starts on a whole pixel and every letter keeps its rounded
     * distance from that start, so a moving line never shimmers apart
     */
    fn text_pictures(
        &mut self,
        content: &str,
        font: FontRef<'static>,
        pixel_size: f32,
        color: Color,
        area: Area,
    ) {
        let ascent = font.metrics(font::coords(font)).scale(pixel_size).ascent;

        let start = self.transform.map(area.x, area.y);

        let line_x = start.0.round();
        let baseline = (start.1 + ascent).round();

        let mut pen = 0.0;

        for letter in content.chars() {
            let measured = font::measure_letter(font, letter, pixel_size);

            let letter_x = line_x + f32::round(pen);

            pen += measured.advance;

            self.commands.push(Command::Glyph {
                font: measured.font,
                id: measured.id,
                size: pixel_size,
                color,
                x: letter_x,
                y: baseline,
            });
        }
    }

    fn text_outlines(
        &mut self,
        content: &str,
        font: FontRef<'static>,
        size: f32,
        color: Color,
        area: Area,
    ) {
        let ascent = font.metrics(font::coords(font)).scale(size).ascent;

        let baseline = area.y + ascent;

        let mut pen_x = area.x;

        for letter in content.chars() {
            let measured = font::measure_letter(font, letter, size);

            let letter_x = pen_x;

            pen_x += measured.advance;

            // a space has no outline but still moves the pen
            let Some(path) = outline(measured.font, measured.id) else {
                continue;
            };

            let face_units = measured.units;

            // fonts point y upward, the screen points it downward
            let letter_transform =
                Transform::from_row(face_units, 0.0, 0.0, -face_units, letter_x, baseline);
            let transform = letter_transform.post_concat(self.transform);

            self.commands.push(Command::Fill {
                path,
                transform,
                color,
            });
        }
    }
}

fn outline(font: FontRef<'static>, id: u16) -> Option<BezierPath> {
    let key = (font.key.value(), id);

    OUTLINES.with_borrow_mut(|outlines| {
        let path = outlines
            .entry(key)
            .or_insert_with(|| read_outline(font, id));

        path.clone()
    })
}

fn read_outline(font: FontRef<'static>, id: u16) -> Option<BezierPath> {
    font::with_context(|context| {
        // no size keeps the outline in the font's own units
        let mut scaler = context
            .builder(font)
            .normalized_coords(font::coords(font))
            .build();

        let shape = scaler.scale_outline(id)?;

        Outline::new().read(shape.path()).finish()
    })
}
