use std::cell::RefCell;
use std::collections::HashMap;

use crate::graphics::font::{self, Face};
use crate::graphics::{BezierPath, Color, Outline, Renderer, Transform};

use super::Command;

thread_local! {
    // each letter's outline, read from the face once
    static OUTLINES: RefCell<HashMap<(u64, u16), Option<BezierPath>>> =
        RefCell::new(HashMap::new());
}

// one shaped letter of a line: what it is, and where its pen sits from the line's start on its baseline
#[derive(Clone, Copy)]
pub struct Placed {
    pub face: Face,
    pub id: u16,
    pub size: f32,
    pub x: f32,
    pub y: f32,
}

impl Renderer {
    /*
     * a line of letters whose baseline starts at x, y; letters are drawn as
     * small cached pictures, which the gpu handles far faster than outlines,
     * so only rotated or stretched text needs the outlines
     */
    pub fn text(&mut self, letters: &[Placed], color: Color, x: f32, y: f32) {
        match self.transform.even_scale() {
            Some(scale) => self.text_pictures(letters, scale, color, x, y),
            None => self.text_outlines(letters, color, x, y),
        }
    }

    /*
     * the line starts on a whole pixel and every letter keeps its rounded
     * distance from that start, so a moving line never shimmers apart
     */
    fn text_pictures(&mut self, letters: &[Placed], scale: f32, color: Color, x: f32, y: f32) {
        let start = self.transform.map(x, y);

        let line_x = start.0.round();
        let baseline = start.1.round();

        for letter in letters {
            self.commands.push(Command::Glyph {
                face: letter.face,
                id: letter.id,
                size: letter.size * scale,
                color,
                x: line_x + f32::round(letter.x * scale),
                y: baseline + f32::round(letter.y * scale),
            });
        }
    }

    fn text_outlines(&mut self, letters: &[Placed], color: Color, x: f32, y: f32) {
        for letter in letters {
            // a space has no outline
            let Some(path) = outline(letter.face, letter.id) else {
                continue;
            };

            let units_per_em = letter.face.font.metrics(letter.face.coords).units_per_em;

            let face_units = letter.size / f32::from(units_per_em);

            // a faked italic leans the letter forward, by how far it rises
            let lean = letter
                .face
                .skew
                .map_or(0.0, |degrees| degrees.to_radians().tan());

            // fonts point y upward, the screen points it downward
            let letter_transform = Transform::from_row(
                face_units,
                0.0,
                face_units * lean,
                -face_units,
                x + letter.x,
                y + letter.y,
            );
            let transform = letter_transform.post_concat(self.transform);

            self.commands.push(Command::Fill {
                path,
                transform,
                color,
            });
        }
    }
}

fn outline(face: Face, id: u16) -> Option<BezierPath> {
    let key = (face.key(), id);

    OUTLINES.with_borrow_mut(|outlines| {
        let path = outlines
            .entry(key)
            .or_insert_with(|| read_outline(face, id));

        path.clone()
    })
}

fn read_outline(face: Face, id: u16) -> Option<BezierPath> {
    font::with_context(|context| {
        // no size keeps the outline in the font's own units
        let mut scaler = context
            .builder(face.font)
            .normalized_coords(face.coords)
            .build();

        let shape = scaler.scale_outline(id)?;

        Outline::new().read(shape.path()).finish()
    })
}
