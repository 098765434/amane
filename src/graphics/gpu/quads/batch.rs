use vello::wgpu::{CommandEncoder, Device, Queue, Texture};

use crate::graphics::font::Face;
use crate::graphics::gpu::picture;
use crate::graphics::image::Bitmap;
use crate::graphics::{Area, Color, Corners};

use super::clip::NO_CLIP;
use super::pipeline::picture_group;
use super::{Bound, Clip, Clips, LETTER, PICTURE, Paint, QUAD_SIZE, Quads, Run, SHAPE};

impl Quads {
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
     * every frame, like a card swelling open, fills it quickly. The frame's
     * work so far is submitted before the atlas is written again, since a
     * queue's texture writes land ahead of any encoder submitted after them
     */
    #[allow(clippy::too_many_arguments)]
    pub fn letter(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
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
            self.draw(device, encoder, canvas);

            let drawn =
                std::mem::replace(encoder, device.create_command_encoder(&Default::default()));

            queue.submit([drawn.finish()]);

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

            let group = picture_group(
                device,
                &self.picture_inputs,
                &texture,
                &self.blank_colour,
                &self.sampler,
            );

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
            Some(Bound::Picture(key)),
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
        picture: Option<Bound>,
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
    pub(super) fn push_with(
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
        picture: Option<Bound>,
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
