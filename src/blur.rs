use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::{Color, InputArea};

/*
 * one rounded rectangle the compositor blurs behind (niri-blur-shapes), with
 * how much of the effect shows and the compositor's material it draws with;
 * where the compositor lacks the protocol it is approximated by strips of a
 * blur region instead, which can't fade
 */
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlurShape {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) radius: f32,
    pub(crate) alpha: f32,
    pub(crate) material: Option<&'static str>,
    pub(crate) tint: Option<Color>,
    pub(crate) id: Option<u32>,
}

impl BlurShape {
    pub fn new(x: f32, y: f32, width: f32, height: f32, radius: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            radius,
            alpha: 1.0,
            material: None,
            tint: None,
            id: None,
        }
    }

    // how much of the effect shows, 0 to 1
    pub fn alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha.clamp(0.0, 1.0);

        self
    }

    // a material the compositor offers, see material_available; plain blur otherwise
    pub fn material(mut self, name: &'static str) -> Self {
        self.material = Some(name);

        self
    }

    // a colour the material lays over the blur, when it draws tints
    pub fn tint(mut self, color: Color) -> Self {
        self.tint = Some(color);

        self
    }

    /*
     * the shell's own name for the shape, the same from frame to frame, so
     * what the compositor says about the backdrop behind it can be read back
     * (Backdrop); unique across the program's windows
     */
    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);

        self
    }

    // as drawn by a layer scaled by `scale` about `origin`
    pub fn scale_about(mut self, scale: f32, origin: (f32, f32)) -> Self {
        self.x = origin.0 + (self.x - origin.0) * scale;
        self.y = origin.1 + (self.y - origin.1) * scale;
        self.width *= scale;
        self.height *= scale;
        self.radius *= scale;

        self
    }

    // as drawn by a layer moved by (`dx`, `dy`)
    pub fn offset(mut self, dx: f32, dy: f32) -> Self {
        self.x += dx;
        self.y += dy;

        self
    }

    // as drawn by a layer at `opacity`
    pub fn fade(mut self, opacity: f32) -> Self {
        self.alpha = (self.alpha * opacity).clamp(0.0, 1.0);

        self
    }

    pub fn x(&self) -> f32 {
        self.x
    }

    pub fn y(&self) -> f32 {
        self.y
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn height(&self) -> f32 {
        self.height
    }

    pub fn radius(&self) -> f32 {
        self.radius
    }

    pub fn opacity(&self) -> f32 {
        self.alpha
    }

    pub fn shape_id(&self) -> Option<u32> {
        self.id
    }
}

static SHAPES: AtomicBool = AtomicBool::new(false);

// the materials the compositor announced, by id; replaced whole when it sends them again
static MATERIALS: Mutex<Vec<(u32, String)>> = Mutex::new(Vec::new());

// counts the times the materials changed, so windows send their shapes again
static MATERIALS_CHANGED: AtomicU64 = AtomicU64::new(0);

// whether the compositor draws blur shapes as asked, with their fade; without it a shape
// shows only while at least half visible, and with square steps at its corners
pub fn blur_fades() -> bool {
    SHAPES.load(Ordering::Relaxed)
}

// whether the compositor offers a material by this name
pub fn material_available(name: &str) -> bool {
    material_id(name).is_some()
}

pub(crate) fn set_shapes_available(available: bool) {
    SHAPES.store(available, Ordering::Relaxed);
}

pub(crate) fn set_materials(materials: Vec<(u32, String)>) {
    *MATERIALS.lock().unwrap_or_else(|e| e.into_inner()) = materials;

    MATERIALS_CHANGED.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn materials_changed() -> u64 {
    MATERIALS_CHANGED.load(Ordering::Relaxed)
}

pub(crate) fn material_id(name: &str) -> Option<u32> {
    MATERIALS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|(_, n)| n == name)
        .map(|(id, _)| *id)
}

// a colour as the protocol wants it: premultiplied 0xRRGGBBAA
pub(crate) fn premultiplied(color: Color) -> u32 {
    let a = u32::from(color.alpha());
    let channel = |c: u8| (u32::from(c) * a + 127) / 255;

    (channel(color.red()) << 24) | (channel(color.green()) << 16) | (channel(color.blue()) << 8) | a
}

// how many strips stand in for one corner where the compositor lacks shapes
const STEPS: usize = 8;

// how far each corner strip reaches into the next, so rounding never leaves a hairline
const OVERLAP: f32 = 1.0;

// below this a shape is left out where blur can't fade, rather than shown at full strength
const SHOWN_FROM: f32 = 0.5;

/*
 * a blur region standing in for shapes, already in surface coordinates (each
 * shape times `factor`): horizontal strips that step in at the corners, each
 * inset as the curve is at its middle and rounded inwards
 */
pub(crate) fn strips(shapes: &[BlurShape], factor: f32) -> Vec<InputArea> {
    let mut areas = Vec::new();

    for shape in shapes.iter().filter(|s| s.alpha >= SHOWN_FROM) {
        let (x, y) = (shape.x * factor, shape.y * factor);
        let (width, height) = (shape.width * factor, shape.height * factor);

        if width <= 0.0 || height <= 0.0 {
            continue;
        }

        let radius = (shape.radius * factor)
            .min(width / 2.0)
            .min(height / 2.0)
            .max(0.0);

        if radius < 1.0 {
            areas.push(inward(x, y, width, height));
            continue;
        }

        let band = radius / STEPS as f32;

        for i in 0..STEPS {
            let outer = band * i as f32;
            let middle = outer + band / 2.0;
            let inset = radius - (radius * radius - (radius - middle).powi(2)).sqrt();

            areas.push(inward(
                x + inset,
                y + outer,
                width - inset * 2.0,
                band + OVERLAP,
            ));
            areas.push(inward(
                x + inset,
                y + height - outer - band - OVERLAP,
                width - inset * 2.0,
                band + OVERLAP,
            ));
        }

        areas.push(inward(x, y + radius, width, height - radius * 2.0));
    }

    areas.retain(|a| a.width > 0 && a.height > 0);

    areas
}

// rounded to whole pixels, shrinking rather than growing
fn inward(x: f32, y: f32, width: f32, height: f32) -> InputArea {
    let left = x.ceil();
    let top = y.ceil();
    let right = (x + width).floor();
    let bottom = (y + height).floor();

    InputArea {
        x: left as i32,
        y: top as i32,
        width: (right - left).max(0.0) as i32,
        height: (bottom - top).max(0.0) as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_stay_inside_their_shape() {
        let areas = strips(&[BlurShape::new(10.0, 20.0, 100.0, 60.0, 16.0)], 1.0);

        assert_eq!(areas.len(), STEPS * 2 + 1);

        for area in &areas {
            assert!(area.x >= 10 && area.x + area.width <= 110, "{area:?}");
            assert!(area.y >= 20 && area.y + area.height <= 80, "{area:?}");
        }

        // the outermost strip steps in, the middle one spans the whole width
        assert!(areas[0].x > 10);
        assert_eq!((areas[STEPS * 2].x, areas[STEPS * 2].width), (10, 100));
    }

    #[test]
    fn strips_follow_the_scale_factor() {
        let areas = strips(&[BlurShape::new(10.0, 10.0, 20.0, 20.0, 0.0)], 2.0);

        assert_eq!(
            areas,
            vec![InputArea {
                x: 20,
                y: 20,
                width: 40,
                height: 40
            }]
        );
    }

    #[test]
    fn a_shape_mostly_faded_gets_no_strips() {
        let shape = BlurShape::new(0.0, 0.0, 20.0, 20.0, 0.0);

        assert!(strips(&[shape.alpha(0.4)], 1.0).is_empty());
        assert_eq!(strips(&[shape.alpha(0.5)], 1.0).len(), 1);
    }

    #[test]
    fn a_shape_follows_its_layer() {
        let shape = BlurShape::new(10.0, 10.0, 20.0, 20.0, 8.0)
            .scale_about(0.5, (30.0, 30.0))
            .offset(-5.0, 0.0)
            .fade(0.5);

        assert_eq!((shape.x, shape.y), (15.0, 20.0));
        assert_eq!((shape.width, shape.radius, shape.alpha), (10.0, 4.0, 0.5));
    }

    #[test]
    fn a_tint_is_sent_premultiplied() {
        assert_eq!(premultiplied(Color::rgba(255, 128, 0, 255)), 0xff80_00ff);
        assert_eq!(premultiplied(Color::rgba(255, 255, 255, 128)), 0x8080_8080);
    }
}
