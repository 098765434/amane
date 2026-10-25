use smithay_client_toolkit::reexports::calloop::{LoopHandle, ping::make_ping};

use crate::changes;

use super::WaylandState;

pub fn insert(handle: &LoopHandle<'static, WaylandState>) {
    let (ping, source) = make_ping().expect("failed to create wake ping");

    handle
        .insert_source(source, |_, _, state| {
            state.end_lock_if_unlocked();

            state.start_lock_if_asked();

            state.update_idle_if_asked();

            state.handle_window_requests();

            state.handle_popup_requests();

            state.handle_clipboard_requests();

            state.handle_spawn_requests();

            state.request_changed_frames();
        })
        .expect("failed to insert wake ping");

    changes::set_waker(move || ping.ping());
}
