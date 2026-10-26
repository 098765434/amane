use smithay_client_toolkit::reexports::calloop::{
    LoopHandle,
    timer::{TimeoutAction, Timer},
};

use crate::graphics::{VELLO_IDLE, release_idle_scratch};

use super::WaylandState;

// a timer that lets vello's scratch go once nothing drew through it for the idle time
pub fn insert(handle: &LoopHandle<'static, WaylandState>) {
    let Some(idle) = *VELLO_IDLE else {
        return;
    };

    // checked every idle period: at most that long after the last run, at least one period
    handle
        .insert_source(Timer::from_duration(idle), move |_, _, _| {
            release_idle_scratch();

            TimeoutAction::ToDuration(idle)
        })
        .expect("failed to insert the gpu idle timer");
}
