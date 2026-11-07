# Timers (fork addition)

`Timer` runs a job on the event loop at a time, instead of a thread that sleeps until then. A
job runs on the UI thread, like an input handler; one that changes what windows show marks it
the usual way (a service write, `amane::mark_all`). `redraw_after` does only that.

## Usage

```rust,ignore
use std::time::Duration;
use amane::{Timer, redraw_after};

// once, half a second from now
let hide = Timer::after(Duration::from_millis(500), || HOVER.set(false));

// changed its mind
hide.cancel();

// a countdown that only changes with time: draw again when the next second starts
redraw_after(Duration::from_secs(1));
```

| API | Does |
|---|---|
| `Timer::after(delay, job) -> TimerHandle` | runs `job` on the event loop once `delay` has passed |
| `Timer::at(instant, job) -> TimerHandle` | the same at an `Instant`; one already past runs at the loop's next turn |
| `TimerHandle::cancel()`, `is_cancelled()` | stops a job that hasn't run; dropping the handle doesn't |
| `redraw_after(delay) -> TimerHandle` | draws every window again then |

Both are callable from any thread, and before `App::run`: those timers start with the event
loop.

## Design / internals

- `src/timer.rs`. A calloop channel on the event loop takes each request and inserts a calloop
  `Timer` source for it, which runs the job and drops itself. Requests made before the loop
  exists wait in a list that is emptied when the channel is attached (`WaylandApp::new`).
- Tests run a bare calloop `EventLoop` without a compositor: time order, cancelling, and a job
  that asks for another.
- The internal timers (the GPU idle release, the clipboard clear in `copy_secret`) were already
  calloop timers and stay as they are. The Bluetooth and KDE Connect pauses that sleep are in
  the middle of a blocking D-Bus sequence on the worker thread, and stay too.
