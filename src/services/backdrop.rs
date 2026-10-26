use std::collections::HashMap;

use crate::Service;
use crate::changes::{self, Source};
use crate::timing;

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
    /*
     * the last reading for the shape with this id, none before the first;
     * a later reading for it draws again only the windows that asked
     */
    pub fn luminance(&self, id: u32) -> Option<f32> {
        changes::note_source(Source::Backdrop(id));

        self.shapes.get(&id).copied()
    }
}

/*
 * a reading from the compositor; one that didn't change redraws nothing,
 * and one that did only the windows that read that shape's
 */
pub(crate) fn report(id: u32, luminance: f32) {
    let luminance = luminance.clamp(0.0, 1.0);
    let mut backdrop = Backdrop::write();

    // not a change to every reader of the service
    backdrop.quiet();

    if backdrop.shapes.insert(id, luminance) != Some(luminance) {
        drop(backdrop);

        if timing::enabled() {
            eprintln!("change Backdrop {id}");
        }

        changes::mark_source(Source::Backdrop(id));
    }
}
