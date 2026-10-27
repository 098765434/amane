use std::sync::LazyLock;

use vello::Scene;
use vello::kurbo::Affine;
use vello::peniko::{Fill, ImageData, Mix};
use vello::wgpu::Texture;

use crate::graphics::path::Segment;
use crate::graphics::renderer::Command;
use crate::graphics::{Area, BezierPath, Cap, Corners, Join, Transform};

use super::Gpu;
use super::bounds::Reach;
use super::convert::bezier;
use super::quads::{Clip, Clips};

/*
 * rectangles, borders, drop shadows and letters go to the quads, anything else to vello;
 * whichever was gathering is drawn first when the other takes over, so
 * everything still lands on the canvas in order
 */
impl Gpu {
    // draws each command over what the commands before it left on the canvas
    pub(super) fn run(&mut self, commands: Vec<Command>, canvas: &Texture) {
        self.run_clipped(commands, canvas, Clips::default());
    }

    fn run_clipped(&mut self, commands: Vec<Command>, canvas: &Texture, clips: Clips) {
        let mut scene = Scene::new();

        // blurred copies vello reads from, dropped once the scene using them is drawn
        let mut borrowed = Vec::new();

        self.gather(commands, canvas, clips, &mut scene, &mut borrowed);

        self.flush(&mut scene, canvas, &mut borrowed);
    }

    fn gather(
        &mut self,
        commands: Vec<Command>,
        canvas: &Texture,
        clips: Clips,
        scene: &mut Scene,
        borrowed: &mut Vec<ImageData>,
    ) {
        for command in commands {
            match route(&command) {
                Route::Canvas => {
                    self.flush(scene, canvas, borrowed);

                    self.change_canvas(command, canvas, scene, borrowed);
                }

                Route::OwnCanvas => {
                    self.flush(scene, canvas, borrowed);

                    self.draw_on_own_canvas(command, canvas, clips);
                }

                Route::QuadClip(clip) => {
                    let Command::Clip { commands, .. } = command else {
                        unreachable!("only clips are routed as quad clips");
                    };

                    self.gather(commands, canvas, clips.within(clip), scene, borrowed);
                }

                Route::Quads => {
                    self.paint_pending_vello(scene, canvas, borrowed);

                    self.add_to_quads(command, canvas, clips);
                }

                Route::Vello => self.add_to_vello(scene, command, canvas, clips),
            }
        }
    }

    // runs a blur, cut or shader on what the canvas holds so far
    fn change_canvas(
        &mut self,
        command: Command,
        canvas: &Texture,
        scene: &mut Scene,
        borrowed: &mut Vec<ImageData>,
    ) {
        match command {
            Command::Blur {
                path,
                transform,
                amount,
            } => {
                let blurred = self.blur(canvas, scene, &path, transform, amount);

                self.reach = Reach::Everywhere;

                borrowed.extend(blurred);
            }

            Command::Cut {
                area,
                radius,
                transform,
                strength,
            } => {
                if let Some(shape) = device_mask(area, radius, transform) {
                    self.quads
                        .erase(&self.device, &self.queue, canvas, shape, strength);
                } else if let Some(path) = area.trace(radius) {
                    self.cut(canvas, &path, transform, strength);
                }
            }

            Command::Shader {
                shader,
                values,
                area,
                path,
                transform,
            } => self.shade(canvas, &shader, &values, area, path.as_ref(), transform),

            _ => unreachable!("only blurs, cuts and shaders change the canvas"),
        }
    }

    /*
     * a faded group is drawn on its own canvas and laid down faded as one, so
     * where its parts overlap they don't show through each other; a clip holding
     * a blur, cut or shader is drawn apart too, so those only see the clip's drawing
     */
    fn draw_on_own_canvas(&mut self, command: Command, canvas: &Texture, clips: Clips) {
        // an effect's passes read what its commands drew on their own canvas
        if let Command::Effect {
            commands,
            effect,
            images,
            area,
            transform,
        } = command
        {
            let own = self.take_canvas(canvas.width(), canvas.height());

            self.run_clipped(commands, &own, clips);

            self.run_effect(&own, &effect, &images, area, transform, canvas);

            self.give_back(own);

            return;
        }

        let (commands, opacity, clipped_to) = match command {
            Command::Group { commands, opacity } => (commands, opacity, None),

            Command::Clip {
                area,
                radius,
                transform,
                commands,
            } => (commands, 1.0, Some((area, radius, transform))),

            _ => unreachable!("only groups and clips are drawn on their own canvas"),
        };

        let own = self.take_canvas(canvas.width(), canvas.height());

        self.run_clipped(commands, &own, clips);

        if let Some((area, radius, transform)) = clipped_to
            && let Some(path) = area.trace(radius)
        {
            self.trim(&own, &path, transform);
        }

        self.lay(&own, canvas, opacity, false);

        self.give_back(own);
    }

    // route only sends commands here that move and scale without turning
    fn add_to_quads(&mut self, command: Command, canvas: &Texture, clips: Clips) {
        match command {
            Command::Rectangle {
                area,
                radius,
                transform,
                color,
            } => {
                let scale = transform.even_scale().unwrap_or(1.0);

                let area = device_area(area, transform);

                self.quads
                    .rectangle(area, radius.map(|corner| corner * scale), color, clips);
            }

            Command::Border {
                area,
                radius,
                thickness,
                transform,
                color,
            } => {
                let scale = transform.even_scale().unwrap_or(1.0);

                let area = device_area(area, transform);

                self.quads.border(
                    area,
                    radius.map(|corner| corner * scale),
                    thickness * scale,
                    color,
                    clips,
                );
            }

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

            Command::Glyph {
                face,
                id,
                size,
                color,
                x,
                y,
            } => self.quads.letter(
                &self.device,
                &self.queue,
                canvas,
                face,
                id,
                size,
                color,
                x,
                y,
                clips,
            ),

            Command::Image {
                image,
                transform,
                area,
                radius,
                clip_transform,
                ..
            } => {
                self.note_shown(&image);

                // the image's own pixels, stretched by its transform onto the canvas
                let size = Area::new(0.0, 0.0, image.width() as f32, image.height() as f32);

                let placement = device_area(size, transform);

                let shape = device_clip(area, radius, clip_transform);

                let clips = match shape {
                    Some(shape) => clips.within(shape),
                    None => clips,
                };

                self.quads
                    .picture(&self.device, &self.queue, &image, placement, clips);
            }

            _ => unreachable!(
                "only rectangles, borders, shadows, letters and images go to the quads"
            ),
        }
    }

    // vello's drawing so far goes down first, so the quads land on top of it
    fn paint_pending_vello(
        &mut self,
        scene: &mut Scene,
        canvas: &Texture,
        borrowed: &mut Vec<ImageData>,
    ) {
        if !scene.encoding().is_empty() {
            self.paint(scene, canvas, borrowed);
        }
    }

    // and the other way round, with the quads' clip handed to vello
    fn add_to_vello(
        &mut self,
        scene: &mut Scene,
        command: Command,
        canvas: &Texture,
        clips: Clips,
    ) {
        self.quads.draw(&self.device, &self.queue, canvas);

        let mut paths = Vec::new();

        for clip in [clips.outer, clips.inner].into_iter().flatten() {
            // an empty clip shows nothing of what is inside it
            let Some(path) = clip.area.trace(clip.radius) else {
                return;
            };

            paths.push(path);
        }

        for path in &paths {
            scene.push_layer(
                Fill::NonZero,
                Mix::Normal,
                1.0,
                Affine::IDENTITY,
                &bezier(path),
            );
        }

        self.reach.add(&command);

        // under AMANE_FRAMES=1, which commands the quads couldn't take, to see what keeps vello busy
        #[cfg(feature = "vello-stats")]
        if crate::timing::enabled() {
            eprintln!("to vello: {}", describe(&command));
        }

        self.add(scene, command, canvas);

        for _ in &paths {
            scene.pop_layer();
        }
    }

    fn flush(&mut self, scene: &mut Scene, canvas: &Texture, borrowed: &mut Vec<ImageData>) {
        self.quads.draw(&self.device, &self.queue, canvas);

        self.paint(scene, canvas, borrowed);
    }
}

// where a command is drawn, decided before anything is drawn
enum Route {
    // blurs, cuts and shaders work on what the canvas already holds
    Canvas,

    // a faded group, an effect, or a clip holding a blur, cut, shader or effect
    OwnCanvas,

    // a clip the quads apply themselves, its commands routed like any others
    QuadClip(Clip),

    // rectangles, borders, drop shadows, letters and images that move and scale without turning
    Quads,

    // paths, gradients, inner shadows, and anything turned or stretched
    Vello,
}

/*
 * AMANE_VELLO_SHADOWS=1 draws drop shadows with vello, as before the quads
 * drew them; the two use the same formula, so this is for telling them apart
 */
static VELLO_SHADOWS: LazyLock<bool> =
    LazyLock::new(|| std::env::var_os("AMANE_VELLO_SHADOWS").is_some());

// a command in a few words, for the vello-stats log
#[cfg(feature = "vello-stats")]
fn describe(command: &Command) -> String {
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
const THIN: f32 = 3.0;

// the unit direction from one point to the next, none for the same point twice
fn direction((x0, y0): (f32, f32), (x1, y1): (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let length = dx.hypot(dy);

    if length <= 0.0 {
        return (0.0, 0.0);
    }

    (dx / length, dy / length)
}

// the two directions added: across the bisector of the corner they make
fn added((ax, ay): (f32, f32), (bx, by): (f32, f32)) -> (f32, f32) {
    (ax + bx, ay + by)
}

// AMANE_VELLO_SHAPES=1 draws arcs, lines and the areas under them with vello, as before
static VELLO_SHAPES: LazyLock<bool> =
    LazyLock::new(|| std::env::var_os("AMANE_VELLO_SHAPES").is_some());

// like css, a shadow's blur reaches twice as far as the deviation of the gaussian behind it
pub(super) fn deviation(blur: f32) -> f32 {
    blur / 2.0
}

/*
 * the path as runs of straight lines, each at least two points long, when it
 * has no curves; a closed run ends where it began
 */
fn polylines(path: &BezierPath) -> Option<Vec<Vec<(f32, f32)>>> {
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
pub(super) fn under_polyline(path: &BezierPath) -> Option<(Vec<(f32, f32)>, f32)> {
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

fn route(command: &Command) -> Route {
    match command {
        Command::Blur { .. } | Command::Cut { .. } | Command::Shader { .. } => Route::Canvas,

        Command::Group { .. } | Command::Effect { .. } => Route::OwnCanvas,

        Command::Clip { commands, .. } if needs_own_canvas(commands) => Route::OwnCanvas,

        Command::Clip {
            area,
            radius,
            transform,
            ..
        } => match device_clip(*area, *radius, *transform) {
            Some(clip) => Route::QuadClip(clip),

            // a rotated clip is left to vello, with everything inside it
            None => Route::Vello,
        },

        Command::Rectangle { transform, .. } | Command::Border { transform, .. } => {
            if transform.even_scale().is_some() {
                Route::Quads
            } else {
                Route::Vello
            }
        }

        Command::Glyph { .. } => Route::Quads,

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

            let simple =
                path.arc.is_some_and(|arc| arc.is_whole()) || under_polyline(path).is_some();

            if straight && simple {
                Route::Quads
            } else {
                Route::Vello
            }
        }

        Command::Image {
            transform,
            clip_transform,
            ..
        } => {
            if straight(*transform) && clip_transform.even_scale().is_some() {
                Route::Quads
            } else {
                Route::Vello
            }
        }

        Command::Gradient { .. } | Command::InnerShadow { .. } => Route::Vello,
    }
}

// blurs, cuts and shaders work on the canvas itself, so a clip holding one draws on a canvas of its own
fn needs_own_canvas(commands: &[Command]) -> bool {
    commands.iter().any(|command| match command {
        Command::Blur { .. }
        | Command::Cut { .. }
        | Command::Shader { .. }
        | Command::Effect { .. } => true,
        Command::Group { commands, .. } | Command::Clip { commands, .. } => {
            needs_own_canvas(commands)
        }
        _ => false,
    })
}

// moves and scales without turning or flipping, each side may scale differently
fn straight(transform: Transform) -> bool {
    let turned = transform.kx != 0.0 || transform.ky != 0.0;
    let flipped = transform.sx <= 0.0 || transform.sy <= 0.0;

    !turned && !flipped
}

fn device_area(area: Area, transform: Transform) -> Area {
    Area::new(
        area.x * transform.sx + transform.tx,
        area.y * transform.sy + transform.ty,
        area.width * transform.sx,
        area.height * transform.sy,
    )
}

fn device_clip(area: Area, radius: Corners, transform: Transform) -> Option<Clip> {
    let scale = transform.even_scale()?;

    Some(Clip {
        area: device_area(area, transform),
        radius: radius.map(|corner| corner * scale),
    })
}

fn device_mask(area: Area, radius: Corners, transform: Transform) -> Option<Clip> {
    let shape = device_clip(area, radius, transform)?;

    // subpixel masks need area coverage; distance-based edges would erase too much
    if shape.area.width <= 1.0 || shape.area.height <= 1.0 {
        return None;
    }

    Some(shape)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_masks_in_device_pixels_and_keeps_subpixel_masks_in_vello() {
        let area = Area::new(10.0, 20.0, 80.0, 80.0);
        let radius = Corners::from(40.0);
        let mask = device_mask(area, radius, Transform::from_scale(1.25, 1.25)).unwrap();

        assert_eq!(mask.area, Area::new(12.5, 25.0, 100.0, 100.0));
        assert_eq!(mask.radius, Corners::from(50.0));
        assert!(device_mask(area, radius, Transform::from_scale(0.01, 0.01)).is_none());
        assert!(
            device_mask(
                Area::new(4.25, 4.25, 0.5, 0.5),
                Corners::from(0.25),
                Transform::IDENTITY,
            )
            .is_none()
        );
        assert!(device_mask(area, radius, Transform::from_rotate(45.0)).is_none());
        assert!(device_mask(area, radius, Transform::from_scale(1.0, 2.0)).is_none());
    }
}

#[cfg(test)]
mod shape_tests {
    use crate::graphics::PathBuilder;

    use super::*;

    fn path(build: impl FnOnce(&mut PathBuilder)) -> BezierPath {
        let mut builder = PathBuilder::new();

        build(&mut builder);

        builder.finish().unwrap()
    }

    #[test]
    fn straight_lines_are_polylines_and_curves_are_not() {
        let zigzag = path(|p| {
            p.move_to(0.0, 0.0);
            p.line_to(10.0, 5.0);
            p.line_to(20.0, 0.0);
            p.move_to(30.0, 30.0);
            p.line_to(40.0, 40.0);
            p.move_to(50.0, 50.0);
        });

        assert_eq!(
            polylines(&zigzag),
            Some(vec![
                vec![(0.0, 0.0), (10.0, 5.0), (20.0, 0.0)],
                vec![(30.0, 30.0), (40.0, 40.0)],
            ])
        );

        let triangle = path(|p| {
            p.move_to(0.0, 0.0);
            p.line_to(10.0, 0.0);
            p.line_to(5.0, 8.0);
            p.close();
        });

        assert_eq!(polylines(&triangle).unwrap()[0].len(), 4);

        let curved = path(|p| {
            p.move_to(0.0, 0.0);
            p.quad_to(5.0, 5.0, 10.0, 0.0);
        });

        assert_eq!(polylines(&curved), None);
        assert_eq!(polylines(&path(|p| p.move_to(1.0, 1.0))), None);
    }

    #[test]
    fn the_area_under_a_line_is_known_and_other_outlines_are_not() {
        let under = path(|p| {
            p.move_to(0.0, 20.0);
            p.line_to(0.0, 10.0);
            p.line_to(10.0, 5.0);
            p.line_to(20.0, 12.0);
            p.line_to(30.0, 20.0);
            p.close();
        });

        assert_eq!(
            under_polyline(&under),
            Some((
                vec![
                    (0.0, 20.0),
                    (0.0, 10.0),
                    (10.0, 5.0),
                    (20.0, 12.0),
                    (30.0, 20.0)
                ],
                20.0
            ))
        );

        // going back left, or below the baseline, or not closed, or not ending on it
        let back = path(|p| {
            p.move_to(0.0, 20.0);
            p.line_to(10.0, 5.0);
            p.line_to(5.0, 8.0);
            p.line_to(30.0, 20.0);
            p.close();
        });
        assert_eq!(under_polyline(&back), None);

        let below = path(|p| {
            p.move_to(0.0, 20.0);
            p.line_to(10.0, 25.0);
            p.line_to(30.0, 20.0);
            p.close();
        });
        assert_eq!(under_polyline(&below), None);

        let open = path(|p| {
            p.move_to(0.0, 20.0);
            p.line_to(10.0, 5.0);
            p.line_to(30.0, 20.0);
        });
        assert_eq!(under_polyline(&open), None);

        let hanging = path(|p| {
            p.move_to(0.0, 20.0);
            p.line_to(10.0, 5.0);
            p.line_to(30.0, 15.0);
            p.close();
        });
        assert_eq!(under_polyline(&hanging), None);
    }
}
