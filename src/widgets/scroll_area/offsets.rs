/*
 * how far each scroll area has moved, kept apart from the widgets that are
 * rebuilt each redraw; fingers move it one to one and it coasts on after
 * they lift, slowing to a stop, and a wheel's step eases in
 */

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::input::PIXELS_PER_LINE;
use crate::{Animation, Easing, Scroll, ScrollSource};

// how quickly a fling's momentum dies out: it travels its speed times this, as iOS's does
const DECAY: f32 = 0.325;

// how long a fling coasts for
const COAST: Duration = Duration::from_millis(850);

// how long a wheel's step takes
const STEP: Duration = Duration::from_millis(160);

// slower than this, in pixels a second, lifting the fingers just stops
const MIN_FLING: f32 = 60.0;

// fingers resting this long before they lift leave no momentum behind
const HELD: Duration = Duration::from_millis(80);

#[derive(Default)]
struct Motion {
    // where it is while nothing moves it
    offset: f32,

    // coasting after a flick, or easing after a wheel
    moving: Option<Animation>,

    // fingers on it: how fast they're going (pixels a second, smoothed) and when they last moved
    fingers: Option<(f32, Instant)>,

    // how far it could move when last drawn
    farthest: f32,

    // content added above is to leave the view where it is: the next growth moves it as far
    keep_bottom: bool,
}

impl Motion {
    fn now(&self) -> f32 {
        match &self.moving {
            Some(moving) => moving.value(),
            None => self.offset,
        }
    }

    // where it comes to rest
    fn target(&self) -> f32 {
        match &self.moving {
            Some(moving) => moving.target,
            None => self.offset,
        }
    }

    fn ease(&mut self, from: f32, to: f32, duration: Duration) {
        let mut moving = Animation::new(from).duration(duration).easing(Easing::Out);

        moving.to(to);

        self.offset = to;
        self.moving = Some(moving);
    }
}

thread_local! {
    static OFFSETS: RefCell<HashMap<&'static str, Motion>> = RefCell::new(HashMap::new());
}

// where it is now, asking for frames while it moves
pub fn get(id: &'static str) -> f32 {
    OFFSETS.with_borrow(|offsets| offsets.get(id).map_or(0.0, Motion::now))
}

// puts it at `offset` at once
pub fn set(id: &'static str, offset: f32) {
    OFFSETS.with_borrow_mut(|offsets| {
        let motion = offsets.entry(id).or_default();

        motion.offset = offset;
        motion.moving = None;
        motion.fingers = None;
    });
}

// whether it rests within `slack` of its end, as last drawn
pub fn at_end(id: &'static str, slack: f32) -> bool {
    OFFSETS.with_borrow(|offsets| {
        offsets
            .get(id)
            .is_none_or(|motion| motion.target().min(motion.farthest) >= motion.farthest - slack)
    })
}

// the next time the content grows, it grew above: move down as far, so the view stays
pub fn keep_bottom(id: &'static str) {
    OFFSETS.with_borrow_mut(|offsets| {
        let motion = offsets.entry(id).or_default();

        motion.offset = motion.now().clamp(0.0, motion.farthest);
        motion.moving = None;
        motion.keep_bottom = true;
    });
}

// how far it can move now, drawn; a growth asked to keep the bottom moves it with it
pub fn drawn(id: &'static str, farthest: f32) {
    OFFSETS.with_borrow_mut(|offsets| {
        let motion = offsets.entry(id).or_default();

        let grown = farthest - motion.farthest;

        if motion.keep_bottom && grown > 0.5 {
            motion.offset = motion.offset.clamp(0.0, motion.farthest) + grown;
            motion.moving = None;
            motion.keep_bottom = false;
        }

        motion.farthest = farthest;
    });
}

// one scroll event over the area, which can move as far as `farthest`
pub fn scroll(id: &'static str, scroll: &Scroll, farthest: f32) {
    OFFSETS.with_borrow_mut(|offsets| {
        let motion = offsets.entry(id).or_default();

        let now = Instant::now();

        match scroll.source {
            ScrollSource::Finger | ScrollSource::Continuous => {
                let (_, moved) = scroll.pixels();

                follow(motion, moved, scroll.stop, farthest, now);
            }

            ScrollSource::Wheel => {
                let from = motion.now().clamp(0.0, farthest);

                // a step taken while one is still easing goes on from where that one is heading
                let to = (motion.target().clamp(0.0, farthest) + scroll.y * PIXELS_PER_LINE)
                    .clamp(0.0, farthest);

                motion.fingers = None;
                motion.ease(from, to, STEP);
            }

            ScrollSource::Tilt => {}
        }
    });
}

fn follow(motion: &mut Motion, moved: f32, stop: bool, farthest: f32, now: Instant) {
    // fingers landing stop a coast where it is
    if motion.fingers.is_none() {
        motion.offset = motion.now().clamp(0.0, farthest);
        motion.moving = None;
        motion.fingers = Some((0.0, now));
    }

    if moved != 0.0 {
        motion.offset = (motion.offset + moved).clamp(0.0, farthest);

        if let Some((velocity, last)) = &mut motion.fingers {
            let gap = now.duration_since(*last).as_secs_f32().max(0.004);

            // the latest movement counts most: a flick is how a swipe ends
            *velocity = *velocity * 0.5 + moved / gap * 0.5;
            *last = now;
        }
    }

    if !stop {
        return;
    }

    let Some((velocity, last)) = motion.fingers.take() else {
        return;
    };

    let velocity = if now.duration_since(last) >= HELD {
        0.0
    } else {
        velocity
    };

    if velocity.abs() >= MIN_FLING {
        let from = motion.offset;
        let to = (from + velocity * DECAY).clamp(0.0, farthest);

        motion.ease(from, to, COAST);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_added_above_keeps_the_view() {
        let id = "test.keep_bottom";

        drawn(id, 300.0);
        set(id, 120.0);
        keep_bottom(id);

        // a redraw before the content comes changes nothing
        drawn(id, 300.0);
        assert_eq!(get(id), 120.0);

        drawn(id, 500.0);
        assert_eq!(get(id), 320.0);

        // and only the first growth counts
        drawn(id, 600.0);
        assert_eq!(get(id), 320.0);
    }

    #[test]
    fn at_the_end_is_within_the_slack() {
        let id = "test.at_end";

        assert!(at_end(id, 4.0));

        drawn(id, 300.0);
        set(id, f32::MAX);
        assert!(at_end(id, 4.0));

        set(id, 290.0);
        assert!(!at_end(id, 4.0));
        assert!(at_end(id, 16.0));
    }

    fn motion() -> Motion {
        Motion::default()
    }

    #[test]
    fn fingers_move_it_one_to_one() {
        let mut motion = motion();
        let start = Instant::now();

        follow(&mut motion, 10.0, false, 500.0, start);
        follow(
            &mut motion,
            15.0,
            false,
            500.0,
            start + Duration::from_millis(10),
        );

        assert_eq!(motion.now(), 25.0);
        assert!(motion.moving.is_none());
    }

    #[test]
    fn a_flick_coasts_on_past_where_the_fingers_lift() {
        let mut motion = motion();
        let start = Instant::now();

        for i in 0..5 {
            follow(
                &mut motion,
                20.0,
                false,
                2000.0,
                start + Duration::from_millis(i * 10),
            );
        }

        follow(
            &mut motion,
            0.0,
            true,
            2000.0,
            start + Duration::from_millis(45),
        );

        assert!(motion.offset > 100.0);
        assert!(motion.moving.is_some());
        assert!(
            motion.target() > 100.0 + 200.0,
            "coasts to {}",
            motion.target()
        );
        assert!(motion.target() <= 2000.0);
    }

    #[test]
    fn fingers_that_rested_before_lifting_just_stop() {
        let mut motion = motion();
        let start = Instant::now();

        follow(&mut motion, 20.0, false, 2000.0, start);
        follow(
            &mut motion,
            20.0,
            false,
            2000.0,
            start + Duration::from_millis(10),
        );
        follow(
            &mut motion,
            0.0,
            true,
            2000.0,
            start + Duration::from_millis(300),
        );

        assert!(motion.moving.is_none());
        assert_eq!(motion.now(), 40.0);
    }

    #[test]
    fn a_coast_stops_at_the_end() {
        let mut motion = motion();
        let start = Instant::now();

        for i in 0..5 {
            follow(
                &mut motion,
                40.0,
                false,
                250.0,
                start + Duration::from_millis(i * 10),
            );
        }

        follow(
            &mut motion,
            0.0,
            true,
            250.0,
            start + Duration::from_millis(45),
        );

        assert!(motion.target() <= 250.0);
    }
}
