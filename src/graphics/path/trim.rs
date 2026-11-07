//! Paths read from svg, placed, and trimmed to a part of their length, through kurbo.

use vello::kurbo::{BezPath, ParamCurve, ParamCurveArclen, PathEl, PathSeg, Point};

use super::*;

// how close, in canvas pixels, lengths along a curve are worked out
pub(super) const LENGTH_ACCURACY: f64 = 0.1;

impl BezierPath {
    // read from an svg path's d attribute, none when it doesn't parse or draws nothing
    pub fn from_svg(data: &str) -> Option<Self> {
        Self::from_kurbo(&BezPath::from_svg(data).ok()?)
    }

    // every point scaled about 0,0, then moved
    pub fn placed(mut self, scale: f32, x: f32, y: f32) -> Self {
        let at = |px: f32, py: f32| (px * scale + x, py * scale + y);

        if let Some(arc) = &mut self.arc {
            (arc.center_x, arc.center_y) = at(arc.center_x, arc.center_y);
            arc.radius *= scale;
        }

        for segment in &mut self.segments {
            *segment = match *segment {
                Segment::MoveTo(px, py) => {
                    let (px, py) = at(px, py);
                    Segment::MoveTo(px, py)
                }

                Segment::LineTo(px, py) => {
                    let (px, py) = at(px, py);
                    Segment::LineTo(px, py)
                }

                Segment::QuadTo(x1, y1, px, py) => {
                    let ((x1, y1), (px, py)) = (at(x1, y1), at(px, py));
                    Segment::QuadTo(x1, y1, px, py)
                }

                Segment::CubicTo(x1, y1, x2, y2, px, py) => {
                    let ((x1, y1), (x2, y2), (px, py)) = (at(x1, y1), at(x2, y2), at(px, py));
                    Segment::CubicTo(x1, y1, x2, y2, px, py)
                }

                Segment::Close => Segment::Close,
            };
        }

        self
    }

    /*
     * only the part between two fractions of the path's length, 0 its start and
     * 1 its end, as one open line; none when that part has no length
     */
    pub fn trimmed(&self, start: f32, end: f32) -> Option<Self> {
        let start = f64::from(start.clamp(0.0, 1.0));
        let end = f64::from(end.clamp(0.0, 1.0));

        if start <= 0.0 && end >= 1.0 {
            return Some(self.clone());
        }

        // part of an arc is an arc, so it stays one quad for the gpu
        if let Some(arc) = self.arc {
            let sweep = arc.sweep * (end - start) as f32;

            if f64::from(sweep.abs().to_radians() * arc.radius) <= LENGTH_ACCURACY {
                return None;
            }

            let mut builder = PathBuilder::new();

            builder.arc(
                arc.center_x,
                arc.center_y,
                arc.radius,
                arc.start + arc.sweep * start as f32,
                sweep,
            );

            return builder.finish();
        }

        let path = self.to_kurbo();

        let pieces: Vec<(PathSeg, f64)> = path
            .segments()
            .map(|piece| (piece, piece.arclen(LENGTH_ACCURACY)))
            .collect();

        let total: f64 = pieces.iter().map(|(_, length)| length).sum();

        let (from, to) = (start * total, end * total);

        // shorter than the accuracy would leave a dot where a round cap sits
        if to - from <= LENGTH_ACCURACY {
            return None;
        }

        let mut trimmed = BezPath::new();
        let mut walked = 0.0;

        for (piece, length) in pieces {
            let (piece_start, piece_end) = (walked, walked + length);

            walked = piece_end;

            if length <= 0.0 || piece_end <= from || piece_start >= to {
                continue;
            }

            let first = if from > piece_start {
                piece.inv_arclen(from - piece_start, LENGTH_ACCURACY)
            } else {
                0.0
            };

            let last = if to < piece_end {
                piece.inv_arclen(to - piece_start, LENGTH_ACCURACY)
            } else {
                1.0
            };

            let part = piece.subsegment(first..last);

            // a gap between pieces, like the start of another outline, lifts the pen
            if trimmed.elements().last().and_then(|el| el.end_point()) != Some(part.start()) {
                trimmed.move_to(part.start());
            }

            trimmed.push(part.as_path_el());
        }

        Self::from_kurbo(&trimmed)
    }

    pub(crate) fn to_kurbo(&self) -> BezPath {
        let mut bezier = BezPath::new();

        for segment in &self.segments {
            match *segment {
                Segment::MoveTo(x, y) => bezier.move_to(point(x, y)),

                Segment::LineTo(x, y) => bezier.line_to(point(x, y)),

                Segment::QuadTo(x1, y1, x, y) => bezier.quad_to(point(x1, y1), point(x, y)),

                Segment::CubicTo(x1, y1, x2, y2, x, y) => {
                    bezier.curve_to(point(x1, y1), point(x2, y2), point(x, y))
                }

                Segment::Close => bezier.close_path(),
            }
        }

        bezier
    }

    fn from_kurbo(bezier: &BezPath) -> Option<Self> {
        let mut builder = PathBuilder::new();

        let at = |point: Point| (point.x as f32, point.y as f32);

        for element in bezier.elements() {
            match *element {
                PathEl::MoveTo(p) => {
                    let (x, y) = at(p);
                    builder.move_to(x, y);
                }

                PathEl::LineTo(p) => {
                    let (x, y) = at(p);
                    builder.line_to(x, y);
                }

                PathEl::QuadTo(p1, p) => {
                    let ((x1, y1), (x, y)) = (at(p1), at(p));
                    builder.quad_to(x1, y1, x, y);
                }

                PathEl::CurveTo(p1, p2, p) => {
                    let ((x1, y1), (x2, y2), (x, y)) = (at(p1), at(p2), at(p));
                    builder.cubic_to(x1, y1, x2, y2, x, y);
                }

                PathEl::ClosePath => builder.close(),
            }
        }

        builder.finish()
    }
}

impl From<BezierPath> for PathBuilder {
    fn from(path: BezierPath) -> Self {
        Self {
            segments: path.segments,
            arc: path.arc,
        }
    }
}

fn point(x: f32, y: f32) -> Point {
    Point::new(f64::from(x), f64::from(y))
}
