use std::sync::Arc;

use crate::graphics::image::Bitmap;
use crate::graphics::{Area, BezierPath, Cap, Color, Corners, Gradient, Join, Renderer, Transform};

use super::Command;
use crate::video::Picture;

impl Renderer {
    pub fn rectangle(&mut self, area: Area, color: Color, radius: Corners) {
        /*
         * an invisible fill still counts as drawing, and one before a shader
         * makes the gpu run a whole extra drawing pass each frame
         */
        if color.a == 0 {
            return;
        }

        if area.width <= 0.0 || area.height <= 0.0 {
            return;
        }

        self.commands.push(Command::Rectangle {
            area,
            radius,
            transform: self.transform,
            color,
        });
    }

    pub fn border(&mut self, area: Area, radius: Corners, thickness: f32, color: Color) {
        // a zero width line would still draw as a hairline
        if thickness == 0.0 || color.a == 0 {
            return;
        }

        if area.width <= 0.0 || area.height <= 0.0 {
            return;
        }

        self.commands.push(Command::Border {
            area,
            radius,
            thickness,
            transform: self.transform,
            color,
        });
    }

    // the gradient spreads across the rectangle, not the whole window
    pub fn gradient(&mut self, area: Area, gradient: &Gradient, radius: Corners) {
        let Some(path) = area.trace(radius) else {
            return;
        };

        self.commands.push(Command::Gradient {
            path,
            area,
            transform: self.transform,
            gradient: gradient.clone(),
        });
    }

    pub fn image(&mut self, area: Area, radius: Corners, image: Arc<Bitmap>, placement: Area) {
        // an empty rectangle shows none of the image
        if area.width <= 0.0 || area.height <= 0.0 {
            return;
        }

        let horizontal_scale = placement.width / image.width() as f32;
        let vertical_scale = placement.height / image.height() as f32;

        let image_transform = Transform::from_row(
            horizontal_scale,
            0.0,
            0.0,
            vertical_scale,
            placement.x,
            placement.y,
        );

        let transform = image_transform.post_concat(self.transform);

        // the image can reach past the rectangle, the clip keeps it inside the rounded shape
        self.commands.push(Command::Image {
            image,
            transform,
            area,
            radius,
            clip_transform: self.transform,
        });
    }

    // like an image, with the frame's own size as its pixels
    pub(crate) fn video(&mut self, area: Area, radius: Corners, picture: Picture, placement: Area) {
        if area.width <= 0.0 || area.height <= 0.0 {
            return;
        }

        let frame_transform = Transform::from_row(
            placement.width / picture.frame.width as f32,
            0.0,
            0.0,
            placement.height / picture.frame.height as f32,
            placement.x,
            placement.y,
        );

        self.commands.push(Command::Video {
            transform: frame_transform.post_concat(self.transform),
            picture,
            area,
            radius,
            clip_transform: self.transform,
        });
    }

    // the path is in the area's own coordinates, with 0,0 at its top left corner
    pub fn fill_path(&mut self, path: &BezierPath, area: Area, color: Color) {
        self.commands.push(Command::Fill {
            path: path.clone(),
            transform: self.inside(area),
            color,
        });
    }

    // the gradient is laid over the area, the path in the area's own coordinates
    pub fn gradient_path(&mut self, path: &BezierPath, area: Area, gradient: &Gradient) {
        self.commands.push(Command::Gradient {
            path: path.clone(),
            area: Area::new(0.0, 0.0, area.width, area.height),
            transform: self.inside(area),
            gradient: gradient.clone(),
        });
    }

    pub fn stroke_path(
        &mut self,
        path: &BezierPath,
        area: Area,
        thickness: f32,
        color: Color,
        cap: Cap,
        join: Join,
    ) {
        self.commands.push(Command::Stroke {
            path: path.clone(),
            transform: self.inside(area),
            thickness,
            color,
            cap,
            join,
        });
    }

    // moves the path to where the area sits before scaling it like everything else
    fn inside(&self, area: Area) -> Transform {
        let offset = Transform::from_row(1.0, 0.0, 0.0, 1.0, area.x, area.y);

        offset.post_concat(self.transform)
    }
}
