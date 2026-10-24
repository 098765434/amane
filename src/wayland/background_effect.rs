use smithay_client_toolkit::reexports::protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::{self, ExtBackgroundEffectManagerV1},
    ext_background_effect_surface_v1::{self, ExtBackgroundEffectSurfaceV1},
};
use wayland_client::{Connection, Dispatch, QueueHandle};

use super::WaylandState;

/*
 * a window's say over where the compositor blurs behind it (ext-background-effect):
 * made the first time a view asks for a blur region, kept for the window's life
 */
pub struct BlurEffect {
    pub surface: ExtBackgroundEffectSurfaceV1,
}

// has to go before the surface it belongs to
impl Drop for BlurEffect {
    fn drop(&mut self) {
        self.surface.destroy();
    }
}

// the manager only says which effects it has, and blur is the one asked for
impl Dispatch<ExtBackgroundEffectManagerV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ExtBackgroundEffectManagerV1,
        _: ext_background_effect_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtBackgroundEffectSurfaceV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ExtBackgroundEffectSurfaceV1,
        _: ext_background_effect_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
