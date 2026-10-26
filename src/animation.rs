use std::time::{Duration, Instant};

use crate::Color;

pub struct Animation<T: Blend = f32> {
    pub(crate) from: T,
    pub(crate) target: T,

    pub(crate) started: Instant,
    pub(crate) duration: Duration,
    pub(crate) easing: Easing,

    // how fast a spring sets off, in shares of the way per duration: what it
    // carries from the move it interrupted
    pub(crate) velocity: f32,
}

// anything an animation can move between, `amount` goes from 0 at `from` to 1 at `to`
pub trait Blend: Copy + PartialEq + Send + Sync + 'static {
    fn blend(from: Self, to: Self, amount: f32) -> Self;

    /*
     * how far `value` is along the way from `from` to `to`, as `amount` would
     * put it; a spring retargeted halfway carries its speed into the new way
     * through this, and `None` (the default) starts it from rest
     */
    fn along(from: Self, to: Self, value: Self) -> Option<f32> {
        let _ = (from, to, value);

        None
    }
}

// how the speed changes between the start and the target
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Easing {
    // the same speed the whole way
    Linear,

    // starts fast and slows down as it arrives
    #[default]
    Out,

    // starts slow, speeds up in the middle and slows down again
    InOut,

    /*
     * a damped spring that settles within the duration; `bounce` 0 arrives
     * without overshooting, higher overshoots and wobbles more (up to 0.9)
     */
    Spring {
        bounce: f32,
    },
}

impl<T: Blend> Animation<T> {
    // starts at rest on `value`, nothing moves until `to` is called
    pub fn new(value: T) -> Self {
        Self {
            from: value,
            target: value,

            started: Instant::now(),
            duration: Duration::from_millis(200),
            easing: Easing::default(),
            velocity: 0.0,
        }
    }

    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration;

        self
    }

    pub fn easing(mut self, easing: Easing) -> Self {
        self.easing = easing;

        self
    }

    // a change halfway through starts from where it is now, so it never jumps;
    // a spring keeps the speed it had, so it never stalls either
    pub fn to(&mut self, target: T) {
        if target == self.target {
            return;
        }

        self.rebase(target, self.duration);
    }

    // goes on to the same target over `duration` from where it is now, so
    // one animation can come and go at different speeds
    pub fn retime(&mut self, duration: Duration) {
        if duration == self.duration {
            return;
        }

        self.rebase(self.target, duration);
    }

    fn rebase(&mut self, target: T, duration: Duration) {
        let now = self.current();
        let velocity = match self.easing {
            Easing::Spring { .. } => self.carried(now, target, duration),
            _ => 0.0,
        };

        self.from = now;
        self.target = target;
        self.duration = duration;
        self.started = Instant::now();
        self.velocity = velocity;
    }

    /*
     * the speed it's moving at, as a share of the way from `now` to `target`
     * per `duration`: where it was a moment ago tells how fast it's going
     */
    fn carried(&self, now: T, target: T, duration: Duration) -> f32 {
        let progress = self.progress();

        if progress >= 1.0 {
            return 0.0;
        }

        let total = self.duration.as_secs_f32();
        let earlier = (progress - 0.004 / total).max(0.0);
        let elapsed = (progress - earlier) * total;

        if elapsed <= 0.0 {
            return 0.0;
        }

        let Some(back) = T::along(now, target, self.at(earlier)) else {
            return 0.0;
        };

        (-back / elapsed * duration.as_secs_f32()).clamp(-MAX_VELOCITY, MAX_VELOCITY)
    }

    // read in the view, and while it is still moving the window keeps drawing frames
    pub fn value(&self) -> T {
        if self.progress() < 1.0 {
            moving::set();
        }

        self.current()
    }

    fn current(&self) -> T {
        self.at(self.progress())
    }

    fn at(&self, progress: f32) -> T {
        let eased = self.easing.launch(progress, self.velocity);

        T::blend(self.from, self.target, eased)
    }

    // 0 when it has just started, 1 once it has arrived
    fn progress(&self) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }

        let elapsed = self.started.elapsed().as_secs_f32();
        let total = self.duration.as_secs_f32();

        f32::min(elapsed / total, 1.0)
    }
}

// a spring that inherits more than this (a flick across a tiny way) would
// still be swinging when its duration runs out
const MAX_VELOCITY: f32 = 12.0;

impl Blend for f32 {
    fn blend(from: Self, to: Self, amount: f32) -> Self {
        from + (to - from) * amount
    }

    fn along(from: Self, to: Self, value: Self) -> Option<f32> {
        let way = to - from;

        (way.abs() > f32::EPSILON).then(|| (value - from) / way)
    }
}

impl Blend for Color {
    fn blend(from: Self, to: Self, amount: f32) -> Self {
        let r = blend_channel(from.r, to.r, amount);
        let g = blend_channel(from.g, to.g, amount);
        let b = blend_channel(from.b, to.b, amount);
        let a = blend_channel(from.a, to.a, amount);

        Color::rgba(r, g, b, a)
    }
}

impl Easing {
    // turns how much time has passed into how far along the way it is, both from 0 to 1
    pub fn apply(self, progress: f32) -> f32 {
        self.launch(progress, 0.0)
    }

    // the same, setting off at `velocity` (only a spring carries it)
    fn launch(self, progress: f32, velocity: f32) -> f32 {
        match self {
            Easing::Linear => progress,
            Easing::Out => Self::out(progress),
            Easing::InOut => Self::in_out(progress),
            Easing::Spring { bounce } => Self::spring(progress, bounce, velocity),
        }
    }

    fn out(progress: f32) -> f32 {
        let remaining = 1.0 - progress;

        1.0 - remaining * remaining * remaining
    }

    fn in_out(progress: f32) -> f32 {
        if progress < 0.5 {
            return 4.0 * progress * progress * progress;
        }

        let remaining = 2.0 - 2.0 * progress;

        1.0 - remaining * remaining * remaining / 2.0
    }

    /*
     * a unit step on a damped spring, stiff enough that what's left of the
     * swing is about 0.001 when the duration runs out, and then exactly 1
     * so it always lands on the target
     */
    fn spring(progress: f32, bounce: f32, velocity: f32) -> f32 {
        if progress >= 1.0 {
            return 1.0;
        }

        let damping = 1.0 - bounce.clamp(0.0, 0.9);

        if damping >= 1.0 {
            let stiffness = 9.2;
            let decay = (-stiffness * progress).exp();

            return 1.0 - decay * (1.0 + (stiffness - velocity) * progress);
        }

        let stiffness = 6.9 / damping;
        let wobble = stiffness * (1.0 - damping * damping).sqrt();
        let decay = (-damping * stiffness * progress).exp();
        let swing = (wobble * progress).cos()
            + (damping * stiffness - velocity) / wobble * (wobble * progress).sin();

        1.0 - decay * swing
    }
}

/*
 * asks for one more frame after this one; call it from the view while
 * something you move yourself has not arrived, and stop calling it once
 * it has, so the window can rest
 */
pub fn request_frame() {
    moving::set();
}

fn blend_channel(from: u8, to: u8, amount: f32) -> u8 {
    let from = f32::from(from);
    let to = f32::from(to);

    let blended = f32::blend(from, to, amount);

    blended.round() as u8
}

pub mod moving {
    use std::sync::atomic::{AtomicBool, Ordering};

    // set by any animation the view read that has not arrived yet
    static MOVING: AtomicBool = AtomicBool::new(false);

    pub fn set() {
        MOVING.store(true, Ordering::Relaxed);
    }

    // asked once after each view, so the flag only ever describes that view
    pub fn take() -> bool {
        MOVING.swap(false, Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{Animation, Easing};

    fn samples(easing: Easing) -> impl Iterator<Item = f32> {
        (0..=100).map(move |step| easing.apply(step as f32 / 100.0))
    }

    #[test]
    fn spring_starts_at_zero_and_lands_on_one() {
        for bounce in [0.0, 0.15, 0.3, 0.9] {
            let spring = Easing::Spring { bounce };

            assert!(spring.apply(0.0).abs() < 1e-6);
            assert_eq!(spring.apply(1.0), 1.0);
            assert!((spring.apply(0.99) - 1.0).abs() < 0.01);
        }
    }

    #[test]
    fn spring_without_bounce_never_overshoots() {
        assert!(samples(Easing::Spring { bounce: 0.0 }).all(|x| x <= 1.0));
    }

    #[test]
    fn spring_with_bounce_overshoots() {
        let peak = samples(Easing::Spring { bounce: 0.3 }).fold(0.0, f32::max);

        assert!(peak > 1.0);
    }

    // a spring halfway along, as if it had started `ago` back
    fn halfway(ago: Duration) -> Animation {
        let mut spring = Animation::new(0.0)
            .duration(Duration::from_millis(500))
            .easing(Easing::Spring { bounce: 0.1 });

        spring.to(100.0);
        spring.started = Instant::now() - ago;

        spring
    }

    // how fast it moves now, in units a second
    fn speed(animation: &Animation) -> f32 {
        let total = animation.duration.as_secs_f32();
        let progress = animation.progress();
        let step = 0.001;

        (animation.at(progress + step / total) - animation.at(progress)) / step
    }

    #[test]
    fn a_retargeted_spring_keeps_its_place_and_speed() {
        let mut spring = halfway(Duration::from_millis(80));
        let before = (spring.current(), speed(&spring));

        assert!(before.1 > 100.0, "it's moving towards 100");

        spring.to(0.0);
        let after = (spring.current(), speed(&spring));

        assert!((after.0 - before.0).abs() < 1.0);
        assert!(
            (after.1 - before.1).abs() < before.1 * 0.1,
            "{before:?} then {after:?}"
        );
        assert_eq!(spring.at(1.0), 0.0);
    }

    #[test]
    fn retiming_keeps_the_speed_too() {
        let mut spring = halfway(Duration::from_millis(80));
        let before = speed(&spring);

        spring.retime(Duration::from_millis(200));

        assert!((speed(&spring) - before).abs() < before * 0.1);
        assert_eq!(spring.at(1.0), 100.0);
    }

    #[test]
    fn a_spring_at_rest_starts_from_rest() {
        let mut spring = halfway(Duration::from_secs(1));

        spring.to(0.0);

        assert_eq!(spring.velocity, 0.0);
    }

    #[test]
    fn what_cannot_say_how_far_along_it_is_starts_from_rest() {
        use crate::Color;

        let mut color = Animation::new(Color::rgba(0, 0, 0, 255))
            .duration(Duration::from_millis(500))
            .easing(Easing::Spring { bounce: 0.1 });

        color.to(Color::rgba(255, 255, 255, 255));
        color.started = Instant::now() - Duration::from_millis(80);
        color.to(Color::rgba(0, 0, 0, 255));

        assert_eq!(color.velocity, 0.0);
    }
}
