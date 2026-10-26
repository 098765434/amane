use std::env;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::LazyLock;

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

    /*
     * amane draws one frame per frame callback and never one for a hidden
     * window, so fifo never has a frame waiting on another: it gets the
     * fewest buffers (mesa gives mailbox four), and one frame of latency is
     * all a frame drawn on the callback needs. AMANE_PRESENT=mailbox is the
     * old choice, for a compositor that stalls fifo presents
     */
    let (present_mode, desired_maximum_frame_latency) =
        if *MAILBOX && capabilities.present_modes.contains(&PresentMode::Mailbox) {
            (PresentMode::Mailbox, 2)
        } else {
            (PresentMode::Fifo, 1)
        };

    SurfaceConfiguration {
        usage: TextureUsages::RENDER_ATTACHMENT,
        format,
        color_space: SurfaceColorSpace::Auto,
        width: 0,
        height: 0,
        present_mode,
        desired_maximum_frame_latency,
        alpha_mode,
        view_formats: Vec::new(),
    }
}

static MAILBOX: LazyLock<bool> =
    LazyLock::new(|| env::var_os("AMANE_PRESENT").is_some_and(|mode| mode == "mailbox"));
