//! The fork's shapes on the quads: drop shadows, arcs, straight lines, the area under a line,
//! small filled outlines, two-stop gradients and video frames, with the helpers that decide
//! which of them the quads can draw. `AMANE_VELLO_SHADOWS` and `AMANE_VELLO_SHAPES` send them
//! back to vello.

use std::sync::LazyLock;

use crate::graphics::path::Segment;
use crate::graphics::renderer::Command;
use crate::graphics::{Area, BezierPath, Cap, Color, Corners, Gradient, Join};

use super::super::Gpu;
use super::super::convert::bezier;
use super::super::quads::{Clips, Paint};
use super::{Route, device_area, device_clip, straight};

impl Gpu {
    // the commands only the fork sends to the quads
    pub(super) fn add_shape_to_quads(&mut self, command: Command, clips: Clips) {
        match command {
            Command::Shadow {
                area,
                radius,
                transform,
                color,
                blur,
            } => {
                let scale = transform.even_scale().unwrap_or(1.0);

                let area = device_area(area, transform);

                self.quads.shadow(
                    area,
                    radius.largest() * scale,
                    deviation(blur) * scale,
                    color,
                    clips,
                );
            }

            Command::Stroke {
                path,
                transform,
                thickness,
                color,
                cap,
                ..
            } => {
                let scale = transform.even_scale().unwrap_or(1.0);
                let thickness = thickness * scale;

                if let Some(arc) = path.arc {
                    self.quads.arc(
                        transform.map(arc.center_x, arc.center_y),
                        arc.radius * scale,
                        (arc.start, arc.sweep),
                        thickness,
                        cap,
                        color,
                        clips,
                    );

                    return;
                }

                for line in polylines(&path).unwrap_or_default() {
                    let points: Vec<(f32, f32)> =
                        line.iter().map(|(x, y)| transform.map(*x, *y)).collect();

                    let directions: Vec<(f32, f32)> = points
                        .windows(2)
                        .map(|pair| direction(pair[0], pair[1]))
                        .collect();

                    let last = directions.len() - 1;

                    for (i, pair) in points.windows(2).enumerate() {
                        // the ends get the line's caps; where pieces meet they are rounded into
                        // a join and cut at its bisector, so neither draws the other's pixels
                        let (start, cut_start) = if i == 0 {
                            (cap, (0.0, 0.0))
                        } else {
                            (Cap::Round, added(directions[i - 1], directions[i]))
                        };

                        let (finish, cut_finish) = if i == last {
                            (cap, (0.0, 0.0))
                        } else {
                            (Cap::Round, added(directions[i], directions[i + 1]))
                        };

                        self.quads.segment(
                            pair[0],
                            pair[1],
                            thickness,
                            (start, finish),
                            (cut_start, cut_finish),
                            color,
                            clips,
                        );
                    }
                }
            }

            Command::Fill {
                path,
                transform,
                color,
            } => {
                let scale = transform.even_scale().unwrap_or(1.0);

                // a whole circle is a rounded rectangle with the corners meeting in the middle
                if let Some(arc) = path.arc {
                    let (x, y) = transform.map(arc.center_x, arc.center_y);
                    let radius = arc.radius * scale;

                    let area = Area::new(x - radius, y - radius, radius * 2.0, radius * 2.0);

                    self.quads
                        .rectangle(area, Corners::from(radius), color, clips);

                    return;
                }

                let Some((points, baseline)) = under_polyline(&path) else {
                    // any other small closed outline, curves flattened
                    if let Some(corners) = outline_corners(&path, scale) {
                        let corners: Vec<(f32, f32)> =
                            corners.iter().map(|(x, y)| transform.map(*x, *y)).collect();

                        self.quads.outline(&corners, Paint::solid(color), clips);
                    }

                    return;
                };

                let (_, baseline) = transform.map(0.0, baseline);

                for pair in points.windows(2) {
                    let (x0, y0) = pair[0];
                    let (x1, y1) = pair[1];

                    self.quads.trapezoid(
                        transform.map(x0, y0),
                        transform.map(x1, y1),
                        baseline,
                        color,
                        clips,
                    );
                }
            }

            Command::Gradient {
                path,
                area,
                transform,
                gradient,
            } => {
                let scale = transform.even_scale().unwrap_or(1.0);

                let (Some(corners), Some(paint)) = (
                    outline_corners(&path, scale),
                    two_stops(&gradient, device_area(area, transform)),
                ) else {
                    return;
                };

                let corners: Vec<(f32, f32)> =
                    corners.iter().map(|(x, y)| transform.map(*x, *y)).collect();

                self.quads.outline(&corners, paint, clips);
            }

            Command::Video {
                picture,
                transform,
                area,
                radius,
                clip_transform,
            } => {
                // a turned or flipped frame isn't drawn
                if !straight(transform) {
                    return;
                }

                let size = Area::new(
                    0.0,
                    0.0,
                    picture.frame.width as f32,
                    picture.frame.height as f32,
                );

                let placement = device_area(size, transform);

                let clips = match device_clip(area, radius, clip_transform) {
                    Some(shape) => clips.within(shape),
                    None => clips,
                };

                self.quads
                    .video(&self.device, &self.queue, &picture, placement, clips);
            }

            _ => unreachable!(
                "only rectangles, borders, shadows, shapes, gradients, letters and images go to the quads"
            ),
        }
    }
}

// where the fork's shapes go: the commands upstream sends to vello
pub(super) fn route(command: &Command) -> Route {
    match command {
        Command::Shadow { transform, .. } => {
            if transform.even_scale().is_some() && !*VELLO_SHADOWS {
                Route::Quads
            } else {
                Route::Vello
            }
        }

        // an arc, or straight lines; their joins come out round, so a thick line that
        // asks for another join stays with vello
        Command::Stroke {
            path,
            transform,
            thickness,
            join,
            ..
        } => {
            let scale = transform.even_scale();
            let straight = scale.is_some() && !*VELLO_SHAPES;

            let rounded = *join == Join::Round || thickness * scale.unwrap_or(1.0) <= THIN;

            let simple = path.arc.is_some() || (rounded && polylines(path).is_some());

            if straight && simple {
                Route::Quads
            } else {
                Route::Vello
            }
        }

        // a whole circle, or the area under a line down to a baseline
        Command::Fill {
            path, transform, ..
        } => {
            let straight = transform.even_scale().is_some() && !*VELLO_SHAPES;

            let simple = path.arc.is_some_and(|arc| arc.is_whole())
                || under_polyline(path).is_some()
                || outline_corners(path, transform.even_scale().unwrap_or(1.0)).is_some();

            if straight && simple {
                Route::Quads
            } else {
                Route::Vello
            }
        }

        // two colours along a line, over a small outline
        Command::Gradient {
            path,
            area,
            transform,
            gradient,
        } => {
            let scale = transform.even_scale();

            let simple = scale.is_some_and(|scale| outline_corners(path, scale).is_some())
                && two_stops(gradient, device_area(*area, *transform)).is_some();

            if simple && !*VELLO_SHAPES {
                Route::Quads
            } else {
                Route::Vello
            }
        }

        // vello has no video frames: turned ones aren't shown
        Command::Video { .. } => Route::Quads,

        Command::InnerShadow { .. } => Route::Vello,

        _ => Route::Vello,
    }
}

/*
 * AMANE_VELLO_SHADOWS=1 draws drop shadows with vello, as before the quads
 * drew them; the two use the same formula, so this is for telling them apart
 */
pub(super) static VELLO_SHADOWS: LazyLock<bool> =
    LazyLock::new(|| std::env::var_os("AMANE_VELLO_SHADOWS").is_some());

// a command in a few words, for the vello-stats log
#[cfg(feature = "vello-stats")]
pub(super) fn describe(command: &Command) -> String {
    match command {
        Command::Fill { path, color, .. } => {
            format!(
                "fill of {} steps, alpha {}",
                path.segments.len(),
                color.alpha()
            )
        }
        Command::Stroke {
            path,
            color,
            thickness,
            ..
        } => format!(
            "stroke of {} steps, {thickness} thick, alpha {}",
            path.segments.len(),
            color.alpha()
        ),
        Command::Gradient { .. } => "gradient".into(),
        Command::Shadow { .. } => "turned shadow".into(),
        Command::InnerShadow { .. } => "inner shadow".into(),
        Command::Rectangle { .. } | Command::Border { .. } => "turned rectangle".into(),
        Command::Image { .. } => "turned image".into(),
        Command::Clip { .. } => "turned clip".into(),
        _ => "other".into(),
    }
}

// a line this thin, in canvas pixels, shows no difference between one join and another
pub(super) const THIN: f32 = 3.0;

// the unit direction from one point to the next, none for the same point twice
pub(super) fn direction((x0, y0): (f32, f32), (x1, y1): (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let length = dx.hypot(dy);

    if length <= 0.0 {
        return (0.0, 0.0);
    }

    (dx / length, dy / length)
}

// the two directions added: across the bisector of the corner they make
pub(super) fn added((ax, ay): (f32, f32), (bx, by): (f32, f32)) -> (f32, f32) {
    (ax + bx, ay + by)
}

// AMANE_VELLO_SHAPES=1 draws arcs, lines and the areas under them with vello, as before
pub(super) static VELLO_SHAPES: LazyLock<bool> =
    LazyLock::new(|| std::env::var_os("AMANE_VELLO_SHAPES").is_some());

// like css, a shadow's blur reaches twice as far as the deviation of the gaussian behind it
pub(in crate::graphics::gpu) fn deviation(blur: f32) -> f32 {
    blur / 2.0
}

// the most corners a filled outline has to go to the quads; the shader walks them all per pixel
pub(super) const OUTLINE_CORNERS: usize = 256;

// how far a flattened curve strays from the real one, in device pixels
pub(super) const FLATNESS: f64 = 0.2;

/*
 * a path of one closed outline as its corners, curves flattened finely
 * enough for `scale` device pixels per unit, when it has few enough of them
 * for the quads; the last corner isn't the first again
 */
pub(in crate::graphics::gpu) fn outline_corners(
    path: &BezierPath,
    scale: f32,
) -> Option<Vec<(f32, f32)>> {
    use vello::kurbo::{PathEl, flatten};

    let bezier = bezier(path);

    let mut corners: Vec<(f32, f32)> = Vec::new();
    let mut outlines = 0;

    let tolerance = FLATNESS / f64::from(scale.max(0.01));

    flatten(
        bezier.elements().iter().copied(),
        tolerance,
        |element| match element {
            PathEl::MoveTo(point) => {
                outlines += 1;
                corners.push((point.x as f32, point.y as f32));
            }
            PathEl::LineTo(point) => corners.push((point.x as f32, point.y as f32)),
            _ => {}
        },
    );

    if outlines != 1 || corners.len() < 3 || corners.len() > OUTLINE_CORNERS {
        return None;
    }

    // closing back onto the start adds nothing
    if corners.first() == corners.last() {
        corners.pop();
    }

    Some(corners)
}

/*
 * a linear gradient of two stops as quads paint it, its line laid over
 * `area` in device pixels as vello lays it (see gradient.rs)
 */
pub(super) fn two_stops(gradient: &Gradient, area: Area) -> Option<Paint> {
    let Gradient::Linear { angle, stops } = gradient else {
        return None;
    };

    let [(from_at, from), (to_at, to)]: [(f32, Color); 2] = stops.as_slice().try_into().ok()?;

    let (sin, cos) = angle.to_radians().sin_cos();

    let half_length = (area.width * sin.abs() + area.height * cos.abs()) / 2.0;

    let center = (area.x + area.width / 2.0, area.y + area.height / 2.0);

    // 0 degrees runs bottom to top, and y grows downward
    let step = (sin * half_length, -cos * half_length);

    let start = (center.0 - step.0, center.1 - step.1);
    let end = (center.0 + step.0, center.1 + step.1);

    let at = |offset: f32| {
        (
            start.0 + (end.0 - start.0) * offset,
            start.1 + (end.1 - start.1) * offset,
        )
    };

    Some(Paint::linear(from, to, at(from_at), at(to_at)))
}

/*
 * the path as runs of straight lines, each at least two points long, when it
 * has no curves; a closed run ends where it began
 */
pub(super) fn polylines(path: &BezierPath) -> Option<Vec<Vec<(f32, f32)>>> {
    let mut lines: Vec<Vec<(f32, f32)>> = Vec::new();

    for segment in &path.segments {
        match *segment {
            Segment::MoveTo(x, y) => lines.push(vec![(x, y)]),

            Segment::LineTo(x, y) => lines.last_mut()?.push((x, y)),

            Segment::Close => {
                let line = lines.last_mut()?;
                let first = *line.first()?;

                if line.last() != Some(&first) {
                    line.push(first);
                }
            }

            Segment::QuadTo(..) | Segment::CubicTo(..) => return None,
        }
    }

    lines.retain(|line| line.len() >= 2);

    if lines.is_empty() {
        return None;
    }

    Some(lines)
}

/*
 * a closed outline that starts on a baseline, goes along a line of points
 * from left to right, and comes back down to the baseline: the area under
 * the line, as the points along it from one end of the baseline to the other
 */
pub(in crate::graphics::gpu) fn under_polyline(
    path: &BezierPath,
) -> Option<(Vec<(f32, f32)>, f32)> {
    let mut lines = polylines(path)?;

    if lines.len() != 1 {
        return None;
    }

    let mut points = lines.pop()?;

    // closed, so the last point is the first again
    if points.len() < 4 || points.first() != points.last() {
        return None;
    }

    points.pop();

    let (_, baseline) = points[0];
    let (_, last_y) = points[points.len() - 1];

    if last_y != baseline {
        return None;
    }

    let left_to_right = points.windows(2).all(|pair| pair[0].0 <= pair[1].0);
    let above = points.iter().all(|(_, y)| *y <= baseline);

    if !left_to_right || !above {
        return None;
    }

    Some((points, baseline))
}
