use std::ffi::c_void;
use std::ptr::NonNull;

use vello::wgpu::rwh::{
    DisplayHandle, HandleError, HasDisplayHandle, RawDisplayHandle, RawWindowHandle,
    WaylandDisplayHandle, WaylandWindowHandle,
};
use vello::wgpu::{
    Adapter, CompositeAlphaMode, Instance, PresentMode, Surface, SurfaceColorSpace,
    SurfaceConfiguration, SurfaceTargetUnsafe, TextureFormat, TextureUsages,
};

// libwayland's display, which the gl backend needs before it can present to any window
#[derive(Debug)]
pub(super) struct Display(NonNull<c_void>);

// only touches the display on the main thread, and the connection outlives the gpu
unsafe impl Send for Display {}
unsafe impl Sync for Display {}

impl Display {
    pub(super) fn new(display: *mut c_void) -> Self {
        let display = NonNull::new(display).expect("failed to read the wayland display");

        Self(display)
    }

    fn raw(&self) -> RawDisplayHandle {
        RawDisplayHandle::Wayland(WaylandDisplayHandle::new(self.0))
    }
}

impl HasDisplayHandle for Display {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        // the connection stays open for as long as the gpu uses it
        let handle = unsafe { DisplayHandle::borrow_raw(self.raw()) };

        Ok(handle)
    }
}

// the pointers are libwayland's display and surface, which have to outlive the gpu
pub(super) fn create(
    instance: &Instance,
    display: *mut c_void,
    surface: *mut c_void,
) -> Surface<'static> {
    let display = Display::new(display);
    let surface = NonNull::new(surface).expect("failed to read the wayland surface");

    let target = SurfaceTargetUnsafe::RawHandle {
        raw_display_handle: Some(display.raw()),
        raw_window_handle: RawWindowHandle::Wayland(WaylandWindowHandle::new(surface)),
    };

    // the caller keeps the wayland objects alive for longer than the gpu
    unsafe { instance.create_surface_unsafe(target) }.expect("failed to create gpu surface")
}

// the size stays 0 until the compositor says how big the window is
pub(super) fn configure(surface: &Surface, adapter: &Adapter) -> SurfaceConfiguration {
    let capabilities = surface.get_capabilities(adapter);

    // the canvas already holds display ready values, an srgb format would convert them twice
    let format = capabilities
        .formats
        .iter()
        .copied()
        .find(|format| {
            matches!(
                format,
                TextureFormat::Bgra8Unorm | TextureFormat::Rgba8Unorm
            )
        })
        .expect("failed to find a surface format without srgb conversion");

    // premultiplied lets the compositor show the desktop through transparent pixels
    let alpha_mode = if capabilities
        .alpha_modes
        .contains(&CompositeAlphaMode::PreMultiplied)
    {
        CompositeAlphaMode::PreMultiplied
    } else {
        capabilities.alpha_modes[0]
    };

    // amane waits for frame callbacks itself, so presenting should not wait again
    let present_mode = if capabilities.present_modes.contains(&PresentMode::Mailbox) {
        PresentMode::Mailbox
    } else {
        PresentMode::Fifo
    };

    SurfaceConfiguration {
        usage: TextureUsages::RENDER_ATTACHMENT,
        format,
        color_space: SurfaceColorSpace::Auto,
        width: 0,
        height: 0,
        present_mode,
        desired_maximum_frame_latency: 2,
        alpha_mode,
        view_formats: Vec::new(),
    }
}
