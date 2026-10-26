use vello::kurbo::{Rect, Shape};

use crate::graphics::renderer::Command;
use crate::graphics::{Area, Cap, Join, Transform};

use super::convert::{affine, bezier};

/*
 * vello costs as much as the texture it draws into, so a run that only
 * reaches a small part of the window, like a ring or a clock's digits,
 * draws into a texture the size of that part and is laid down there
 */
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Reach {
    #[default]
    Nothing,

    Within(Rect),

    // images, gradients, shadows and clips vello draws, whose edges aren't worked out
    Everywhere,
}

// where a run's texture goes on the canvas, in real pixels
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

// textures come in steps of this many pixels, so a box that grows a little reuses a spare
const STEP: u32 = 64;

impl Reach {
    pub fn add(&mut self, command: &Command) {
        *self = match (*self, reach(command)) {
            (Self::Everywhere, _) | (_, None) => Self::Everywhere,

            (Self::Nothing, Some(rect)) => Self::Within(rect),

            (Self::Within(sum), Some(rect)) => Self::Within(sum.union(rect)),
        };
    }

    // the canvas part to draw, none when nothing of the run lands on the canvas
    pub fn place(self, width: u32, height: u32) -> Option<Place> {
        let whole = Place {
            x: 0,
            y: 0,
            width,
            height,
        };

        let rect = match self {
            Self::Nothing => return None,
            Self::Everywhere => return Some(whole),
            Self::Within(rect) => rect,
        };

        let left = rect.x0.floor().max(0.0);
        let top = rect.y0.floor().max(0.0);
        let right = rect.x1.ceil().min(f64::from(width));
        let bottom = rect.y1.ceil().min(f64::from(height));

        if right <= left || bottom <= top {
            return None;
        }

        let place = Place {
            x: left as u32,
            y: top as u32,
            width: (right - left) as u32,
            height: (bottom - top) as u32,
        };

        // a box near the window's size saves little, and the window-sized spares fit it
        let covered = u64::from(place.width) * u64::from(place.height) * 4;

        if covered >= u64::from(width) * u64::from(height) * 3 {
            return Some(whole);
        }

        Some(place)
    }
}

impl Place {
    pub fn is_whole(self, width: u32, height: u32) -> bool {
        self.x == 0 && self.y == 0 && self.width == width && self.height == height
    }

    // the texture size the part is drawn into, never past the canvas
    pub fn texture(self, width: u32, height: u32) -> (u32, u32) {
        let step = |size: u32, limit: u32| size.div_ceil(STEP).saturating_mul(STEP).min(limit);

        (step(self.width, width), step(self.height, height))
    }
}

// what one command covers on the canvas, none when it isn't worked out
fn reach(command: &Command) -> Option<Rect> {
    let rect = match command {
        Command::Fill {
            path, transform, ..
        } => (affine(*transform) * bezier(path)).bounding_box(),

        Command::Stroke {
            path,
            transform,
            thickness,
            cap,
            join,
            ..
        } => {
            let transform = affine(*transform);

            let [a, b, c, d, ..] = transform.as_coeffs();

            let scale = f64::max(a.hypot(b), c.hypot(d));

            let half = f64::from(*thickness) / 2.0 * scale;

            // a miter corner reaches out up to its limit, kurbo's is 4 widths
            let corner = match join {
                Join::Miter => half * 4.0,
                Join::Round | Join::Bevel => half,
            };

            let end = match cap {
                Cap::Square => half * std::f64::consts::SQRT_2,
                Cap::Butt | Cap::Round => half,
            };

            (transform * bezier(path))
                .bounding_box()
                .inflate(corner.max(end), corner.max(end))
        }

        Command::Rectangle {
            area, transform, ..
        } => outline(*area, *transform, 0.0)?,

        Command::Border {
            area,
            transform,
            thickness,
            ..
        } => outline(*area, *transform, *thickness)?,

        _ => return None,
    };

    // antialiasing reaches a pixel past the edge
    Some(rect.inflate(1.0, 1.0))
}

fn outline(area: Area, transform: Transform, thickness: f32) -> Option<Rect> {
    let corners = Rect::new(
        f64::from(area.x),
        f64::from(area.y),
        f64::from(area.x + area.width),
        f64::from(area.y + area.height),
    );

    let rect = affine(transform).transform_rect_bbox(corners);

    let [a, b, c, d, ..] = affine(transform).as_coeffs();

    let reach = f64::from(thickness) * f64::max(a.hypot(b), c.hypot(d));

    Some(rect.inflate(reach, reach))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::BezierPath;
    use crate::graphics::{Color, PathBuilder};

    fn line() -> BezierPath {
        let mut builder = PathBuilder::new();

        builder.move_to(10.0, 10.0);
        builder.line_to(50.0, 10.0);

        builder.finish().unwrap()
    }

    #[test]
    fn a_fill_reaches_its_bounds_and_a_pixel() {
        let mut reach = Reach::default();

        reach.add(&Command::Fill {
            path: line(),
            transform: Transform::IDENTITY,
            color: Color::WHITE,
        });

        assert_eq!(reach, Reach::Within(Rect::new(9.0, 9.0, 51.0, 11.0)));
    }

    #[test]
    fn a_stroke_reaches_out_by_its_miter_and_scale() {
        let mut reach = Reach::default();

        reach.add(&Command::Stroke {
            path: line(),
            transform: Transform::from_scale(2.0, 2.0),
            thickness: 2.0,
            color: Color::WHITE,
            cap: Cap::Round,
            join: Join::Miter,
        });

        // the line spans 20..100 at twice the size, a 4 pixel line's miter reaches 8
        assert_eq!(reach, Reach::Within(Rect::new(11.0, 11.0, 109.0, 29.0)));
    }

    #[test]
    fn anything_unmeasured_reaches_everywhere() {
        let mut reach = Reach::default();

        reach.add(&Command::Fill {
            path: line(),
            transform: Transform::IDENTITY,
            color: Color::WHITE,
        });

        reach.add(&Command::Group {
            commands: Vec::new(),
            opacity: 0.5,
        });

        assert_eq!(reach, Reach::Everywhere);
        assert_eq!(
            reach.place(800, 600).map(|p| p.is_whole(800, 600)),
            Some(true)
        );
    }

    #[test]
    fn places_round_out_and_stay_on_the_canvas() {
        let reach = Reach::Within(Rect::new(-5.5, 10.2, 40.4, 30.0));

        let place = reach.place(800, 600).unwrap();

        assert_eq!(
            place,
            Place {
                x: 0,
                y: 10,
                width: 41,
                height: 20
            }
        );
        assert_eq!(place.texture(800, 600), (64, 64));
        assert_eq!(
            Reach::Within(Rect::new(900.0, 0.0, 950.0, 10.0)).place(800, 600),
            None
        );
        assert_eq!(Reach::Nothing.place(800, 600), None);
    }

    #[test]
    fn a_box_near_the_window_size_draws_whole() {
        let place = Reach::Within(Rect::new(0.0, 0.0, 790.0, 590.0))
            .place(800, 600)
            .unwrap();

        assert!(place.is_whole(800, 600));
    }
}
