use vello::wgpu::{CurrentSurfaceTexture, PollType, SurfaceTexture};

use crate::graphics::renderer::Command;
use crate::graphics::shader_files;

use super::{Gpu, texture};

impl Gpu {
    /*
     * whether these commands at this size are the frame on screen already, so
     * drawing them again would show nothing new; a frame that draws something
     * on time (a shader) isn't, and the caller says so
     */
    pub fn shows(&self, commands: &[Command], width: u32, height: u32) -> bool {
        // a saved shader file draws differently with the same commands
        let shaders_saved = self.shaders_seen != shader_files::generation();

        // a held clock moved draws the same commands at another time
        let clock_moved = self.clock_seen != shader_files::clock_changes();

        !shaders_saved
            && !clock_moved
            && self
                .showing
                .as_ref()
                .is_some_and(|(shown, w, h)| (*w, *h) == (width, height) && shown == commands)
    }

    // the window's buffer was taken away, so nothing it showed is on screen now
    pub fn forget_shown(&mut self) {
        self.showing = None;
    }

    // false when the frame was skipped, so nothing was committed
    pub fn draw(&mut self, commands: Vec<Command>, width: u32, height: u32) -> bool {
        if self.config.width != width || self.config.height != height {
            self.config.width = width;
            self.config.height = height;

            self.surface.configure(&self.device, &self.config);

            // spares and kept results of the old size would never fit again
            self.spare.clear();
            self.stale.clear();
            self.painted.clear();
        }

        let Some(frame) = self.next_frame() else {
            return false;
        };

        self.shaders_seen = shader_files::generation();
        self.clock_seen = shader_files::clock_changes();

        // kept to tell whether the next frame changes anything
        let kept = commands.clone();

        self.painting = 0;
        self.runs = 0;

        self.quads.start_frame();

        let canvas = self.take_canvas(width, height);

        self.run(commands, &canvas);

        let target = frame.texture.create_view(&Default::default());

        let mut encoder = self.device.create_command_encoder(&Default::default());

        self.present.run(
            &self.device,
            &mut encoder,
            &texture::view(&canvas),
            &target,
            [1.0, 0.0, 0.0, 0.0],
        );

        self.queue.submit([encoder.finish()]);

        self.give_back(canvas);

        self.forget_unshown();

        self.forget_unpainted();

        self.forget_unread_inputs();

        self.end_frame();

        // presenting attaches the frame to the wayland surface and commits it
        self.queue.present(frame);

        /*
         * what the last submit finished with (blur temporaries, upload
         * staging) is only given back when the device is polled, which
         * nothing else does; without waiting, so it costs the frame nothing
         */
        let _ = self.device.poll(PollType::Poll);

        self.showing = Some((kept, width, height));

        // what wgpu holds for the process after this frame: alive, and dropped but not yet freed
        #[cfg(feature = "vello-stats")]
        if crate::timing::enabled()
            && let Some(report) = super::shared::instance(self.display).generate_report()
        {
            let hub = report.hub_report();

            eprintln!(
                "wgpu buffers {}+{} textures {}+{} views {}+{} bind_groups {}+{}",
                hub.buffers.num_kept_from_user,
                hub.buffers.num_released_from_user,
                hub.textures.num_kept_from_user,
                hub.textures.num_released_from_user,
                hub.texture_views.num_kept_from_user,
                hub.texture_views.num_released_from_user,
                hub.bind_groups.num_kept_from_user,
                hub.bind_groups.num_released_from_user,
            );
        }

        true
    }

    fn next_frame(&mut self) -> Option<SurfaceTexture> {
        match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) => Some(frame),

            CurrentSurfaceTexture::Suboptimal(frame) => Some(frame),

            // the swapchain no longer fits the surface, a fresh one takes its place
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);

                match self.surface.get_current_texture() {
                    CurrentSurfaceTexture::Success(frame) => Some(frame),

                    _ => None,
                }
            }

            // a hidden or busy window skips this frame, the next redraw catches up
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => None,

            // already reported to the gpu error log, and the next redraw tries again
            CurrentSurfaceTexture::Validation => None,
        }
    }
}
