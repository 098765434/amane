use vello::kurbo::{Affine, BezPath};
use vello::peniko;

use crate::graphics::{BezierPath, Color, Transform};

pub(super) fn bezier(path: &BezierPath) -> BezPath {
    path.to_kurbo()
}

pub(super) fn affine(transform: Transform) -> Affine {
    Affine::new([
        f64::from(transform.sx),
        f64::from(transform.ky),
        f64::from(transform.kx),
        f64::from(transform.sy),
        f64::from(transform.tx),
        f64::from(transform.ty),
    ])
}

pub(super) fn paint(color: Color) -> peniko::Color {
    peniko::Color::from_rgba8(color.r, color.g, color.b, color.a)
}
