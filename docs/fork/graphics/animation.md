# Animation (fork additions)

What the fork adds to upstream's [Animation](../../src/animation.md) page: a damped spring
easing, springs that keep their speed when retargeted, `Animation::retime`, a public
`Easing::apply`, and animations that start at rest.

## Usage

### Spring easing

`Easing::Spring { bounce }` is a damped spring that settles within the animation's duration.
`bounce` 0 arrives without overshooting; higher overshoots and wobbles more, up to 0.9 (larger
values are clamped).

```rust,ignore
use std::time::Duration;
use amane::{Animation, Easing};

let mut open = Animation::new(0.0)
    .duration(Duration::from_millis(400))
    .easing(Easing::Spring { bounce: 0.2 });

open.to(1.0);
```

The spring is stiff enough that what's left of the swing is about 0.001 when the duration runs
out, and then it lands exactly on the target. `Easing` is no longer `Eq`, because of the `f32`.

### Retargeting keeps the speed

`to(target)` halfway through already starts from where the animation is, so it never jumps. A
spring also carries its current speed into the new way, so it never stalls either: a panel
flicked open and then closed mid-way turns around smoothly. Other easings start the new way from
rest.

This works for any `Blend` that says how far a value is along a way (`Blend::along`). `f32` does;
a `Blend` that doesn't (the default, and `Color`) starts from rest. The carried speed is capped,
so a flick across a tiny distance can't still be swinging when the duration ends.

### `retime`

`retime(duration)` goes on to the same target over a new duration, from where it is now, so one
animation can come and go at different speeds:

```rust,ignore
fn hover(inside: bool) {
    let mut lift = Lift::write();

    // in quickly, out slowly
    lift.0.retime(Duration::from_millis(if inside { 120 } else { 400 }));
    lift.0.to(if inside { 1.0 } else { 0.0 });
}
```

An animation that has already arrived only keeps the new duration for its next move. A spring
keeps its speed through a retime too.

### `Easing::apply`

`Easing::apply(progress)` turns how much time has passed into how far along the way it is, both
from 0 to 1, for easing something you move yourself (with `request_frame()`):

```rust,ignore
let eased = Easing::Spring { bounce: 0.1 }.apply(elapsed / total);
```

### A new animation is at rest

`Animation::new(value)` is at rest on `value`, and nothing moves until `to` is called. Making one,
or reading its value, draws no frames (`527c538`).

## Design / internals

- All of it is in `src/animation.rs`. The spring is a unit step on a damped spring:
  critically damped for `bounce` 0, under-damped otherwise. Its velocity is in shares of the way
  per duration.
- On `to` or `retime`, `rebase` measures how fast the value is moving now from where it was a
  moment ago, and converts that into the new way's units through `Blend::along`.
- Commits: `ba43071` (spring), `289f191` (velocity carried, `retime`, `apply`), `527c538` (at
  rest).
