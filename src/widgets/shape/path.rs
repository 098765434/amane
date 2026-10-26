use crate::graphics::{self, BezierPath, PathBuilder};

use super::{Shape, Style};

// a free-form outline drawn step by step in canvas coordinates
#[derive(Clone)]
pub struct Path {
    pub(crate) steps: PathBuilder,
    pub(crate) style: Style,

    // scaled about 0,0 and then moved, when it's drawn
    placement: (f32, f32, f32),

    // the part of its length drawn, from 0 at its start to 1 at its end
    trim: (f32, f32),
}

impl Path {
    pub fn new() -> Self {
        Self {
            steps: PathBuilder::new(),
            style: Style::default(),
            placement: (1.0, 0.0, 0.0),
            trim: (0.0, 1.0),
        }
    }

    /*
     * the steps of an svg path's d attribute, arcs included; none when it
     * doesn't parse. Parse once and clone it, rather than parsing each frame
     */
    pub fn svg(data: &str) -> Option<Self> {
        let path = BezierPath::from_svg(data)?;

        Some(Self {
            steps: PathBuilder::from(path),
            ..Self::new()
        })
    }

    // drawn scaled about 0,0 by scale and then moved by x and y, so one outline fits any size
    pub fn place(mut self, x: f32, y: f32, scale: f32) -> Self {
        self.placement = (scale, x, y);

        self
    }

    /*
     * draws only the part of the line between two fractions of its length,
     * 0 its start and 1 its end, so animating them draws the line on like a pen;
     * nothing at all when the part has no length
     */
    pub fn trim(mut self, start: f32, end: f32) -> Self {
        self.trim = (start, end);

        self
    }

    // lifts the pen and puts it down here, without drawing
    pub fn move_to(mut self, x: f32, y: f32) -> Self {
        self.steps.move_to(x, y);

        self
    }

    pub fn line_to(mut self, x: f32, y: f32) -> Self {
        self.steps.line_to(x, y);

        self
    }

    // a curve bent toward the handle point
    pub fn quad_to(mut self, handle_x: f32, handle_y: f32, x: f32, y: f32) -> Self {
        self.steps.quad_to(handle_x, handle_y, x, y);

        self
    }

    // a curve bent by two handle points, the first near the start and the second near the end
    pub fn cubic_to(
        mut self,
        first_x: f32,
        first_y: f32,
        second_x: f32,
        second_y: f32,
        x: f32,
        y: f32,
    ) -> Self {
        self.steps
            .cubic_to(first_x, first_y, second_x, second_y, x, y);

        self
    }

    // same angles as Arc: degrees, 0 straight up, growing clockwise
    pub fn arc(
        mut self,
        center_x: f32,
        center_y: f32,
        radius: f32,
        start: f32,
        sweep: f32,
    ) -> Self {
        self.steps.arc(center_x, center_y, radius, start, sweep);

        self
    }

    // a straight line back to where this piece started
    pub fn close(mut self) -> Self {
        self.steps.close();

        self
    }
}

impl Default for Path {
    fn default() -> Self {
        Self::new()
    }
}

impl Shape for Path {
    fn trace(&self, _: f32, _: f32) -> Option<graphics::BezierPath> {
        let mut path = self.steps.clone().finish()?;

        let (scale, x, y) = self.placement;

        if (scale, x, y) != (1.0, 0.0, 0.0) {
            path = path.placed(scale, x, y);
        }

        match self.trim {
            (start, end) if start <= 0.0 && end >= 1.0 => Some(path),
            (start, end) => path.trimmed(start, end),
        }
    }

    fn style(&self) -> &Style {
        &self.style
    }

    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}
