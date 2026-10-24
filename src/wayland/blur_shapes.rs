use wayland_client::{Connection, Dispatch, QueueHandle};

use super::WaylandState;
use crate::blur;

pub mod protocol {
    #![allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
    #![allow(non_upper_case_globals, non_snake_case, unused_imports)]
    #![allow(missing_docs, clippy::all)]

    use wayland_client;
    use wayland_client::protocol::*;

    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/niri-blur-shapes-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/niri-blur-shapes-v1.xml");
}

use protocol::niri_blur_shapes_manager_v1::{self, NiriBlurShapesManagerV1};
use protocol::niri_blur_shapes_v1::{self, NiriBlurShapesV1};

/*
 * a window's rounded blur shapes (niri-blur-shapes): made the first time a
 * view asks for shapes on a compositor that has them, kept until it stops
 */
pub struct BlurShapes {
    pub shapes: NiriBlurShapesV1,
}

// has to go before the surface it belongs to; the window's blur region applies again
impl Drop for BlurShapes {
    fn drop(&mut self) {
        self.shapes.destroy();
    }
}

// the materials announced so far, taken whole when the compositor says it's done
#[derive(Default)]
pub struct Materials(std::sync::Mutex<Vec<(u32, String)>>);

impl Dispatch<NiriBlurShapesManagerV1, Materials> for WaylandState {
    fn event(
        _: &mut Self,
        _: &NiriBlurShapesManagerV1,
        event: niri_blur_shapes_manager_v1::Event,
        pending: &Materials,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let mut pending = pending.0.lock().unwrap_or_else(|e| e.into_inner());

        match event {
            niri_blur_shapes_manager_v1::Event::Material { id, name } => pending.push((id, name)),
            niri_blur_shapes_manager_v1::Event::Done => {
                blur::set_materials(std::mem::take(&mut *pending));

                // views choose what to draw by the materials there are
                crate::mark_all();
            }
        }
    }
}

// shapes never send events, but wayland-client still needs somewhere to send them
impl Dispatch<NiriBlurShapesV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &NiriBlurShapesV1,
        _: niri_blur_shapes_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
