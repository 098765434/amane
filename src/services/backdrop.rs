use std::collections::HashMap;

use crate::Service;

/*
 * how bright the compositor says the backdrop is behind each blur shape that
 * has an id (niri-blur-shapes version 2, for materials it reports for), from
 * 0 for black to 1 for white; empty on a compositor that doesn't say
 */
#[derive(Default)]
pub struct Backdrop {
    shapes: HashMap<u32, f32>,
}

// the readings arrive with the compositor's events, so nothing is polled
impl Service for Backdrop {
    fn new() -> Self {
        Self::default()
    }

    fn listen() {}
}

impl Backdrop {
    // the last reading for the shape with this id, none before the first
    pub fn luminance(&self, id: u32) -> Option<f32> {
        self.shapes.get(&id).copied()
    }
}

// a reading from the compositor; one that didn't change redraws nothing
pub(crate) fn report(id: u32, luminance: f32) {
    let luminance = luminance.clamp(0.0, 1.0);
    let mut backdrop = Backdrop::write();

    if backdrop.shapes.insert(id, luminance) == Some(luminance) {
        backdrop.quiet();
    }
}
