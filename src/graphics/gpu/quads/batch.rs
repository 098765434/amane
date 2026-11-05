use vello::wgpu::{Device, Queue, Texture};

use crate::graphics::font::Face;
use crate::graphics::gpu::picture;
use crate::graphics::image::Bitmap;
use crate::graphics::{Area, Cap, Color, Corners};

use super::clip::NO_CLIP;
use super::pipeline::picture_group;
use super::{
    ARC, Clip, Clips, LETTER, OUTLINE, PICTURE, QUAD_SIZE, Quads, Run, SEGMENT, SHADOW,
    SHADOW_REACH, SHAPE, TRAPEZOID,
};

/*
 * what a quad is painted with: one colour, or a two-stop linear gradient
 * from color to color2, in canvas pixels
 */
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paint {
    color: [f32; 4],
    color2: [f32; 4],

    // the direction it runs (x, y), where along it color is, and how far on color2 is
    gradient: [f32; 4],
}

impl Paint {
    pub fn solid(color: Color) -> Self {
        rgba(color).into()
    }

    /*
     * from `from` at `start` to `to` at `end`, canvas points; before and
     * after them the end colours carry on
     */
    pub fn linear(from: Color, to: Color, start: (f32, f32), end: (f32, f32)) -> Self {
        let (dx, dy) = (end.0 - start.0, end.1 - start.1);
        let length = dx.hypot(dy);

        if length <= f32::EPSILON {
            return Self::solid(to);
        }

        let direction = (dx / length, dy / length);

        Self {
            color: rgba(from),
            color2: rgba(to),
            gradient: [
                direction.0,
                direction.1,
                start.0 * direction.0 + start.1 * direction.1,
                length,
            ],
        }
    }
}

#[cfg(test)]
impl Paint {
    // the gradient row: direction x and y, where color is along it, and how far on color2 is
    pub fn line(&self) -> [f32; 4] {
        self.gradient
    }
}

impl From<[f32; 4]> for Paint {
    fn from(color: [f32; 4]) -> Self {
        Self {
            color,
            color2: color,
            gradient: [0.0; 4],
        }
    }
}

// how quads.wgsl tells the ends of a line apart
fn end(cap: Cap) -> f32 {
    match cap {
        Cap::Butt => 0.0,
        Cap::Round => 1.0,
        Cap::Square => 2.0,
    }
}

impl Quads {
    /*
     * part of a circle's edge as a line of the given thickness, in canvas
     * pixels; angles in degrees, 0 straight up, growing clockwise, and the
     * same cap at both ends
     */
    #[allow(clippy::too_many_arguments)]
    pub fn arc(
        &mut self,
        (center_x, center_y): (f32, f32),
        radius: f32,
        (start, sweep): (f32, f32),
        thickness: f32,
        cap: Cap,
        color: Color,
        clips: Clips,
    ) {
        let half = thickness / 2.0;

        // the line's outer edge, a pixel of soft edge, and a square cap's corner
        let reach = radius + half * std::f32::consts::SQRT_2 + 1.0;

        let covered = Area::new(center_x - reach, center_y - reach, reach * 2.0, reach * 2.0);

        // a negative sweep is the same arc from its other end
        let (start, sweep) = if sweep < 0.0 {
            (start + sweep, -sweep)
        } else {
            (start, sweep)
        };

        self.push(
            covered,
            ([radius, end(cap), 0.0, 0.0], half),
            rgba(color),
            clips,
            ARC,
            [
                center_x,
                center_y,
                start.to_radians(),
                sweep.min(360.0).to_radians(),
            ],
            None,
        );
    }

    /*
     * a straight line between two points, in canvas pixels, with a cap at
     * each end; where it meets another line of the same stroke, the cut is
     * the two lines' directions added, and the pixels on the other line's
     * side of the corner's bisector are left to it, so the pieces of a
     * translucent line don't darken where they meet
     */
    #[allow(clippy::too_many_arguments)]
    pub fn segment(
        &mut self,
        (x0, y0): (f32, f32),
        (x1, y1): (f32, f32),
        thickness: f32,
        (start, finish): (Cap, Cap),
        (cut_start, cut_finish): ((f32, f32), (f32, f32)),
        color: Color,
        clips: Clips,
    ) {
        let half = thickness / 2.0;
        let reach = half * std::f32::consts::SQRT_2 + 1.0;

        let covered = Area::new(
            x0.min(x1) - reach,
            y0.min(y1) - reach,
            (x1 - x0).abs() + reach * 2.0,
            (y1 - y0).abs() + reach * 2.0,
        );

        self.push_with(
            covered,
            ([end(start), end(finish), cut_finish.0, cut_finish.1], half),
            [cut_start.0, cut_start.1],
            rgba(color),
            clips,
            SEGMENT,
            [x0, y0, x1, y1],
            None,
        );
    }

    /*
     * the area between a straight line and a horizontal baseline below it, in
     * canvas pixels, cut hard at the line's two x's so the pieces of a longer
     * line sit next to each other without a seam
     */
    pub fn trapezoid(
        &mut self,
        (x0, y0): (f32, f32),
        (x1, y1): (f32, f32),
        baseline: f32,
        color: Color,
        clips: Clips,
    ) {
        let top = y0.min(y1) - 1.0;

        let covered = Area::new(x0.min(x1), top, (x1 - x0).abs(), baseline + 1.0 - top);

        self.push(
            covered,
            ([baseline, 0.0, 0.0, 0.0], 0.0),
            rgba(color),
            clips,
            TRAPEZOID,
            [x0, y0, x1, y1],
            None,
        );
    }

    /*
     * the inside of a closed outline of straight edges, in canvas pixels, by
     * the nonzero rule; the shader walks every edge for every pixel, so it's
     * for small shapes, which paint.rs keeps to a few hundred corners
     */
    pub fn outline(&mut self, corners: &[(f32, f32)], paint: Paint, clips: Clips) {
        if corners.len() < 3 {
            return;
        }

        let (mut left, mut top) = (f32::MAX, f32::MAX);
        let (mut right, mut bottom) = (f32::MIN, f32::MIN);

        for &(x, y) in corners {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
        }

        let first = (self.points.len() / 2) as f32;

        for &(x, y) in corners {
            self.points.extend([x, y]);
        }

        // a pixel of soft edge all round
        let covered = Area::new(
            left - 1.0,
            top - 1.0,
            right - left + 2.0,
            bottom - top + 2.0,
        );

        self.push(
            covered,
            ([0.0; 4], 0.0),
            paint,
            clips,
            OUTLINE,
            [first, corners.len() as f32, 0.0, 0.0],
            None,
        );
    }

    // area and radius in canvas pixels
    pub fn rectangle(&mut self, area: Area, radius: Corners, color: Color, clips: Clips) {
        self.push(
            area,
            (radius.to_array(), 0.0),
            rgba(color),
            clips,
            SHAPE,
            [0.0; 4],
            None,
        );
    }

    /*
     * a drop shadow: the rectangle blurred by a gaussian of the given
     * deviation, in canvas pixels, every corner rounded by radius (as vello
     * draws it: the largest corner); the quad covers how far the blur reaches
     */
    pub fn shadow(&mut self, area: Area, radius: f32, deviation: f32, color: Color, clips: Clips) {
        let reach = SHADOW_REACH * deviation;

        let covered = Area::new(
            area.x - reach,
            area.y - reach,
            area.width + reach * 2.0,
            area.height + reach * 2.0,
        );

        self.push(
            covered,
            ([radius; 4], deviation),
            rgba(color),
            clips,
            SHADOW,
            [area.x, area.y, area.width, area.height],
            None,
        );
    }

    pub(super) fn mask(&mut self, shape: Clip, strength: f32) {
        self.push(
            shape.area,
            (shape.radius.to_array(), 0.0),
            [1.0, 1.0, 1.0, strength],
            Clips::default(),
            SHAPE,
            [0.0; 4],
            None,
        );
    }

    pub fn border(
        &mut self,
        area: Area,
        radius: Corners,
        thickness: f32,
        color: Color,
        clips: Clips,
    ) {
        self.push(
            area,
            (radius.to_array(), thickness),
            rgba(color),
            clips,
            SHAPE,
            [0.0; 4],
            None,
        );
    }

    /*
     * x and y are where the letter's baseline starts, on a whole pixel; a
     * letter that finds the atlas full draws what's queued onto `canvas` first,
     * so the atlas can start over within the frame: text that changes size
     * every frame, like a card swelling open, fills it quickly
     */
    #[allow(clippy::too_many_arguments)]
    pub fn letter(
        &mut self,
        device: &Device,
        queue: &Queue,
        canvas: &Texture,
        face: Face,
        id: u16,
        size: f32,
        color: Color,
        x: f32,
        y: f32,
        clips: Clips,
    ) {
        let mut placed = self.atlas.letter(device, queue, face, id, size);

        if placed.is_none() && self.atlas.is_full() {
            self.draw(device, queue, canvas);

            self.atlas.empty();

            placed = self.atlas.letter(device, queue, face, id, size);
        }

        let Some(letter) = placed else {
            return;
        };

        let area = Area::new(
            x + letter.left,
            y + letter.top,
            letter.width as f32,
            letter.height as f32,
        );

        let texels = [
            letter.x as f32,
            letter.y as f32,
            (letter.x + letter.width) as f32,
            (letter.y + letter.height) as f32,
        ];

        self.push(
            area,
            ([0.0; 4], 0.0),
            rgba(color),
            clips,
            LETTER,
            texels,
            None,
        );
    }

    pub fn forget(&mut self, picture: usize) {
        self.pictures.remove(&picture);
    }

    // the whole image stretched over area, which the clip usually trims
    pub fn picture(
        &mut self,
        device: &Device,
        queue: &Queue,
        image: &Bitmap,
        area: Area,
        clips: Clips,
    ) {
        let key = std::ptr::from_ref(image) as usize;

        if !self.pictures.contains_key(&key) {
            let texture = picture::upload(device, queue, image);

            let group = picture_group(device, &self.picture_inputs, &texture, &self.sampler);

            self.pictures.insert(key, group);
        }

        let corners = [0.0, 0.0, 1.0, 1.0];

        self.push(
            area,
            ([0.0; 4], 0.0),
            [1.0; 4],
            clips,
            PICTURE,
            corners,
            Some(key),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        area: Area,
        (radius, thickness): ([f32; 4], f32),
        color: impl Into<Paint>,
        clips: Clips,
        kind: f32,
        source: [f32; 4],
        picture: Option<usize>,
    ) {
        self.push_with(
            area,
            (radius, thickness),
            [0.0; 2],
            color,
            clips,
            kind,
            source,
            picture,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn push_with(
        &mut self,
        area: Area,
        // the radius row: a shape's corners, or what the kind keeps there
        (radius, thickness): ([f32; 4], f32),
        // the two spare numbers of the shape row
        spare: [f32; 2],
        paint: impl Into<Paint>,
        clips: Clips,
        kind: f32,
        source: [f32; 4],
        picture: Option<usize>,
    ) {
        let outer = clips.outer.unwrap_or(NO_CLIP);
        let inner = clips.inner.unwrap_or(NO_CLIP);

        let index = (self.waiting.len() / QUAD_SIZE) as u32;

        // the fields of Quad in quads.wgsl, in the same order
        let area_row = [area.x, area.y, area.width, area.height];
        let clip_row = [
            outer.area.x,
            outer.area.y,
            outer.area.width,
            outer.area.height,
        ];
        let paint = paint.into();
        let color_row = paint.color;
        let shape_row = [thickness, spare[0], spare[1], kind];
        let source_row = source;
        let inner_clip_row = [
            inner.area.x,
            inner.area.y,
            inner.area.width,
            inner.area.height,
        ];
        let radius_row = radius;
        let clip_radius_row = outer.radius.to_array();
        let inner_radius_row = inner.radius.to_array();

        let rows = [
            area_row,
            clip_row,
            color_row,
            shape_row,
            source_row,
            inner_clip_row,
            radius_row,
            clip_radius_row,
            inner_radius_row,
            paint.color2,
            paint.gradient,
        ];

        for row in rows {
            self.waiting.extend(row);
        }

        // a quad reading the same image as the one before joins its run
        if let Some(run) = self.runs.last_mut()
            && run.picture == picture
        {
            run.count += 1;

            return;
        }

        self.runs.push(Run {
            picture,
            first: index,
            count: 1,
        });
    }
}

fn rgba(color: Color) -> [f32; 4] {
    [color.r, color.g, color.b, color.a].map(|channel| f32::from(channel) / 255.0)
}
