use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use crate::graphics::font::{self, Face};
use crate::graphics::image::Bitmap;
use crate::graphics::letter;
use crate::graphics::{Area, BezierPath, Color, Outline, Renderer, Transform};

use super::Command;

// color letters kept at once; a window resized through many text sizes empties it now and then
const MAX_COLOR_LETTERS: usize = 1024;

// a color letter's picture, and where it sits from the pen on the baseline, in its own pixels
type ColorPicture = (Arc<Bitmap>, f32, f32);

thread_local! {
    // each letter's outline, read from the face once
    static OUTLINES: RefCell<HashMap<(u64, u16), Option<BezierPath>>> =
        RefCell::new(HashMap::new());

    // whether each font draws in color, by its key
    static COLOR_FONTS: RefCell<HashMap<u64, bool>> = RefCell::new(HashMap::new());

    // each color letter's picture at a size, drawn once
    static COLOR_LETTERS: RefCell<HashMap<(u64, u16, u32), Option<ColorPicture>>> =
        RefCell::new(HashMap::new());
}

// whether the face is a color one, like an emoji font, asked once per font
fn color_font(face: Face) -> bool {
    let key = face.font.key.value();

    if let Some(color) = COLOR_FONTS.with_borrow(|fonts| fonts.get(&key).copied()) {
        return color;
    }

    let color = letter::is_color(face);

    COLOR_FONTS.with_borrow_mut(|fonts| fonts.insert(key, color));

    color
}

// a color letter at `size` pixels, as a picture
fn color_picture(face: Face, id: u16, size: f32) -> Option<ColorPicture> {
    let key = (face.key(), id, size.to_bits());

    COLOR_LETTERS.with_borrow_mut(|letters| {
        if letters.len() >= MAX_COLOR_LETTERS && !letters.contains_key(&key) {
            letters.clear();
        }

        letters
            .entry(key)
            .or_insert_with(|| {
                let letter = letter::rasterize_color(face, id, size)?;

                let bitmap = Bitmap::from_rgba(letter.width, letter.height, letter.rgba);

                Some((Arc::new(bitmap), letter.left, letter.top))
            })
            .clone()
    })
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
        let scale = self.transform.even_scale();

        // a color face's letters, like emoji, are pictures in their own colors;
        // a line's letters mostly share a face, so each run of one is asked once
        let mut last: Option<(u64, bool)> = None;
        let mut color_of = |face: Face| {
            let key = face.font.key.value();

            match last {
                Some((last_key, color)) if last_key == key => color,
                _ => {
                    let color = color_font(face);

                    last = Some((key, color));

                    color
                }
            }
        };

        // nothing is put aside for the usual line with no color letter in it
        if letters.iter().any(|letter| color_of(letter.face)) {
            let mut colored = Vec::new();
            let mut plain = Vec::new();

            for letter in letters {
                if color_of(letter.face) {
                    colored.push(*letter);
                } else {
                    plain.push(*letter);
                }
            }

            self.color_letters(&colored, scale.unwrap_or(1.0), x, y);

            match scale {
                Some(scale) => self.text_pictures(&plain, scale, color, x, y),
                None => self.text_outlines(&plain, color, x, y),
            }

            return;
        }

        match scale {
            Some(scale) => self.text_pictures(letters, scale, color, x, y),
            None => self.text_outlines(letters, color, x, y),
        }
    }

    /*
     * color letters drawn at the size they show on screen and placed in the
     * view's own units, so they go to the gpu like any other picture
     */
    fn color_letters(&mut self, letters: &[Placed], scale: f32, x: f32, y: f32) {
        for letter in letters {
            let Some((picture, left, top)) =
                color_picture(letter.face, letter.id, letter.size * scale)
            else {
                continue;
            };

            let placement = Area::new(
                x + letter.x + left / scale,
                y + letter.y + top / scale,
                picture.width() as f32 / scale,
                picture.height() as f32 / scale,
            );

            self.image(placement, 0.0.into(), picture, placement);
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
