use std::fs;
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;

use smithay_client_toolkit::reexports::calloop::{
    Interest, LoopHandle, Mode, PostAction, generic::Generic,
};

use crate::ipc::{ACTIVATE, IpcCall, IpcHandlers, ipc_socket_for};

use super::WaylandState;

pub fn insert(handle: &LoopHandle<'static, WaylandState>, handlers: IpcHandlers, app_id: &str) {
    let source = Generic::new(listen(app_id), Interest::READ, Mode::Level);

    handle
        .insert_source(source, move |_, listener, state| {
            // a failed call only loses that call, the shell keeps running
            if let Ok((stream, _)) = listener.accept() {
                answer(stream, &handlers, state);

                state.request_frames();
            }

            Ok(PostAction::Continue)
        })
        .expect("failed to insert ipc socket");
}

fn listen(app_id: &str) -> UnixListener {
    let path = ipc_socket_for(app_id);

    // a socket that still answers belongs to another running copy, which took this launch
    if UnixStream::connect(&path).is_ok() {
        panic!("failed to listen: {app_id} is already running");
    }

    // what is left is a leftover file from a shell that did not exit cleanly
    let _ = fs::remove_file(&path);

    let listener = UnixListener::bind(&path).expect("failed to create ipc socket");

    // the event loop only wakes this up when a client is waiting, so accept must never block
    listener
        .set_nonblocking(true)
        .expect("failed to make ipc socket non-blocking");

    listener
}

fn answer(mut stream: UnixStream, handlers: &IpcHandlers, state: &mut WaylandState) {
    // a client that never finishes sending must not freeze the shell
    let timeout = Some(Duration::from_secs(1));

    if stream.set_read_timeout(timeout).is_err() {
        return;
    }

    let Ok(call) = IpcCall::read(&stream) else {
        return;
    };

    // amane answers a second launch itself, the app only hears its arguments
    if call.name == ACTIVATE {
        state.activate(&call.arguments, handlers.on_activate);

        let _ = stream.write_all(b"activated");

        return;
    }

    let reply = handlers.run(&call);

    let _ = stream.write_all(reply.as_bytes());
}
