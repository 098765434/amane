use vello::kurbo::{BezPath, ParamCurve, ParamCurveArclen, PathEl, PathSeg, Point};

// one step of a path, in the path's own coordinates
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    MoveTo(f32, f32),

    LineTo(f32, f32),

    QuadTo(f32, f32, f32, f32),

    CubicTo(f32, f32, f32, f32, f32, f32),

    Close,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BezierPath {
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, Default)]
pub struct PathBuilder {
    segments: Vec<Segment>,
}

impl PathBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn move_to(&mut self, x: f32, y: f32) {
        self.segments.push(Segment::MoveTo(x, y));
    }

    pub fn line_to(&mut self, x: f32, y: f32) {
        self.segments.push(Segment::LineTo(x, y));
    }

    pub fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.segments.push(Segment::QuadTo(x1, y1, x, y));
    }

    pub fn cubic_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.segments.push(Segment::CubicTo(x1, y1, x2, y2, x, y));
    }

    pub fn close(&mut self) {
        self.segments.push(Segment::Close);
    }

    // a path with no steps draws nothing, so there is no path to hand back
    pub fn finish(self) -> Option<BezierPath> {
        if self.segments.is_empty() {
            return None;
        }

        Some(BezierPath {
            segments: self.segments,
        })
    }

    /*
     * angles are in degrees, 0 is straight up and they grow clockwise,
     * a negative sweep goes counterclockwise
     */
    pub fn arc(&mut self, center_x: f32, center_y: f32, radius: f32, start: f32, sweep: f32) {
        let (start_x, start_y) = point_at(center_x, center_y, radius, start);

        // an arc in the middle of a path joins on to it instead of starting a new piece
        if self.segments.is_empty() {
            self.move_to(start_x, start_y);
        } else {
            self.line_to(start_x, start_y);
        }

        // one curve can only follow a circle closely for up to a quarter turn
        let quarters = f32::ceil(f32::abs(sweep) / 90.0);
        let pieces = f32::max(quarters, 1.0);

        let piece_sweep = sweep / pieces;

        for piece in 0..pieces as u32 {
            let from = start + piece_sweep * piece as f32;

            self.arc_piece(center_x, center_y, radius, from, piece_sweep);
        }
    }

    fn arc_piece(&mut self, center_x: f32, center_y: f32, radius: f32, from: f32, sweep: f32) {
        let to = from + sweep;

        let (from_x, from_y) = point_at(center_x, center_y, radius, from);
        let (to_x, to_y) = point_at(center_x, center_y, radius, to);

        // how far the handles reach along the circle's direction to bend the curve onto it
        let handle = radius * 4.0 / 3.0 * f32::tan(f32::to_radians(sweep) / 4.0);

        let (from_direction_x, from_direction_y) = direction_at(from);
        let (to_direction_x, to_direction_y) = direction_at(to);

        self.cubic_to(
            from_x + handle * from_direction_x,
            from_y + handle * from_direction_y,
            to_x - handle * to_direction_x,
            to_y - handle * to_direction_y,
            to_x,
            to_y,
        );
    }
}

// how close, in canvas pixels, lengths along a curve are worked out
const LENGTH_ACCURACY: f64 = 0.1;

impl BezierPath {
    // read from an svg path's d attribute, none when it doesn't parse or draws nothing
    pub fn from_svg(data: &str) -> Option<Self> {
        Self::from_kurbo(&BezPath::from_svg(data).ok()?)
    }

    // every point scaled about 0,0, then moved
    pub fn placed(mut self, scale: f32, x: f32, y: f32) -> Self {
        let at = |px: f32, py: f32| (px * scale + x, py * scale + y);

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
        }
    }
}

fn point(x: f32, y: f32) -> Point {
    Point::new(f64::from(x), f64::from(y))
}

fn point_at(center_x: f32, center_y: f32, radius: f32, angle: f32) -> (f32, f32) {
    let radians = f32::to_radians(angle);

    // y grows downward on screen, so up is minus
    let x = center_x + radius * f32::sin(radians);
    let y = center_y - radius * f32::cos(radians);

    (x, y)
}

// which way a point moving clockwise around the circle heads at this angle
fn direction_at(angle: f32) -> (f32, f32) {
    let radians = f32::to_radians(angle);

    (f32::cos(radians), f32::sin(radians))
}

#[cfg(test)]
mod trim_tests {
    use super::*;

    // the digits of a clock drawn in one stroke each, in a 60 by 100 box
    const DIGITS: [&str; 10] = [
        "M12 34 A18 18 0 0 1 48 34 L48 66 A18 18 0 0 1 12 66 Z",
        "M18 24 L34 12 L34 88",
        "M13 30 A17 17 0 0 1 47 30 C47 46 36 56 13 88 L48 88",
        "M14 24 C18 10 46 10 46 30 C46 42 38 48 28 48 C40 48 48 56 48 68 C48 92 16 94 12 78",
        "M40 88 L40 12 L12 62 L50 62",
        "M46 12 L17 12 L14 46 C20 40 26 38 31 38 C42 38 48 48 48 62 C48 82 36 88 28 88 C20 88 14 84 12 78",
        "M42 14 C30 26 14 44 14 64 A17 17 0 1 0 48 64 A17 17 0 1 0 14 64",
        "M12 12 L48 12 L24 88",
        "M30 48 C18 48 14 40 14 30 A16 18 0 0 1 46 30 C46 40 42 48 30 48 C16 48 12 58 12 68 A18 20 0 0 0 48 68 C48 58 44 48 30 48 Z",
        "M18 86 C30 74 46 56 46 36 A17 17 0 1 0 12 36 A17 17 0 1 0 46 36",
    ];

    fn length(path: &BezierPath) -> f64 {
        path.to_kurbo()
            .segments()
            .map(|piece| piece.arclen(LENGTH_ACCURACY))
            .sum()
    }

    fn square() -> BezierPath {
        BezierPath::from_svg("M0 0 L10 0 L10 10 L0 10 Z").unwrap()
    }

    #[test]
    fn every_digit_parses_and_ends_where_it_ends() {
        for digit in DIGITS {
            let path = BezierPath::from_svg(digit).unwrap();

            let end = path.to_kurbo().segments().last().unwrap().end();

            let parsed_end = BezPath::from_svg(digit)
                .unwrap()
                .segments()
                .last()
                .unwrap()
                .end();

            assert!((end - parsed_end).hypot() < 1e-3, "{digit}");
        }

        assert!(BezierPath::from_svg("M0 0 Q").is_none());
    }

    #[test]
    fn a_trimmed_part_is_its_share_of_the_length() {
        // the closing side counts too
        let square = square();

        assert!((length(&square) - 40.0).abs() < 0.01);

        let half = square.trimmed(0.25, 0.75).unwrap();

        assert!((length(&half) - 20.0).abs() < 0.2);
        assert_eq!(half.segments[0], Segment::MoveTo(10.0, 0.0));

        for digit in DIGITS {
            let path = BezierPath::from_svg(digit).unwrap();

            let total = length(&path);

            let first = path.trimmed(0.0, 0.4).unwrap();
            let rest = path.trimmed(0.4, 1.0).unwrap();

            assert!(
                (length(&first) + length(&rest) - total).abs() < 0.5,
                "{digit}"
            );
        }
    }

    #[test]
    fn the_whole_length_is_the_path_and_none_is_nothing() {
        let square = square();

        assert_eq!(square.trimmed(0.0, 1.0), Some(square.clone()));
        assert_eq!(square.trimmed(-1.0, 2.0), Some(square.clone()));
        assert_eq!(square.trimmed(0.5, 0.5), None);
        assert_eq!(square.trimmed(0.8, 0.2), None);
    }

    #[test]
    fn placing_scales_then_moves() {
        let placed = square().placed(2.0, 5.0, 1.0);

        assert_eq!(placed.segments[2], Segment::LineTo(25.0, 21.0));
    }
}
