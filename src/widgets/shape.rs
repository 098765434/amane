mod arc;
mod circle;
mod line;
mod path;
mod round;

use crate::graphics::{self, Cap, Color, Gradient, Join};

pub use arc::Arc;
pub use circle::Circle;
pub use line::Line;
pub use path::Path;

#[derive(Clone)]
pub struct Style {
    pub(crate) fill: Color,

    // a fill that blends across the canvas, in place of the colour
    pub(crate) gradient: Option<Gradient>,

    pub(crate) stroke_thickness: f32,
    pub(crate) stroke_color: Color,
    pub(crate) cap: Cap,
    pub(crate) join: Join,

    pub(crate) opacity: f32,
}

// something a canvas draws, every shape gets the same builder methods for how it looks
pub trait Shape {
    // the outline in the canvas's own coordinates, for a canvas of this size
    fn trace(&self, width: f32, height: f32) -> Option<graphics::BezierPath>;

    fn style(&self) -> &Style;

    fn style_mut(&mut self) -> &mut Style;

    fn fill(mut self, color: impl Into<Color>) -> Self
    where
        Self: Sized,
    {
        self.style_mut().fill = color.into();

        self
    }

    /*
     * fills the shape with a gradient laid over the whole canvas, as a
     * rectangle's gradient is laid over the rectangle; a linear one of two
     * stops on a small shape is drawn with the quads
     */
    fn gradient(mut self, gradient: Gradient) -> Self
    where
        Self: Sized,
    {
        self.style_mut().gradient = Some(gradient);

        self
    }

    fn stroke(mut self, thickness: f32, color: impl Into<Color>) -> Self
    where
        Self: Sized,
    {
        self.style_mut().stroke_thickness = thickness;
        self.style_mut().stroke_color = color.into();

        self
    }

    fn cap(mut self, cap: Cap) -> Self
    where
        Self: Sized,
    {
        self.style_mut().cap = cap;

        self
    }

    fn join(mut self, join: Join) -> Self
    where
        Self: Sized,
    {
        self.style_mut().join = join;

        self
    }

    fn opacity(mut self, opacity: f32) -> Self
    where
        Self: Sized,
    {
        self.style_mut().opacity = opacity;

        self
    }
}

// a new shape draws nothing until it gets a fill or a stroke
impl Default for Style {
    fn default() -> Self {
        Self {
            fill: Color::TRANSPARENT,
            gradient: None,

            stroke_thickness: 0.0,
            stroke_color: Color::TRANSPARENT,
            cap: Cap::Butt,
            join: Join::Miter,

            opacity: 1.0,
        }
    }
}
