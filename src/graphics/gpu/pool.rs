use std::env;
use std::sync::LazyLock;

use vello::wgpu::{
    Color, LoadOp, Operations, RenderPassColorAttachment, RenderPassDescriptor, StoreOp, Texture,
    TextureUsages,
};

use super::{Gpu, texture};

// AMANE_KEEP_SPARES=1 keeps every spare texture until the window resizes, as before
static KEEP_SPARES: LazyLock<bool> = LazyLock::new(|| env::var_os("AMANE_KEEP_SPARES").is_some());

/*
 * textures the size of the window are needed several times a frame, so
 * finished ones are kept and handed out again instead of allocated; one
 * that no frame took since it was given back is let go at the end of the
 * next frame, since a size an animation passed through never comes back,
 * while the canvas and the sizes a steady frame uses are taken every frame
 */
impl Gpu {
    // a reused canvas still holds the last drawing, so it's cleared first
    pub(super) fn take_canvas(&mut self, width: u32, height: u32) -> Texture {
        let Some(canvas) = self.take(width, height, texture::CANVAS) else {
            return texture::canvas(&self.device, width, height);
        };

        self.clear(&canvas);

        canvas
    }

    // passes only draw over their area, so the rest has to read as nothing
    pub(super) fn take_field(&mut self, width: u32, height: u32) -> Texture {
        let Some(field) = self.take(width, height, texture::FIELD) else {
            return texture::field(&self.device, width, height);
        };

        self.clear(&field);

        field
    }

    // vello overwrites every pixel of it, so no clearing needed
    pub(super) fn take_scratch(&mut self, width: u32, height: u32) -> Texture {
        match self.take(width, height, texture::SCRATCH) {
            Some(scratch) => scratch,
            None => texture::scratch(&self.device, width, height),
        }
    }

    pub(super) fn give_back(&mut self, texture: Texture) {
        self.spare.push(texture);
    }

    // the frame is drawn: what was left from the last frame goes, and this frame's spares wait for the next
    pub(super) fn end_frame(&mut self) {
        if *KEEP_SPARES {
            self.spare.append(&mut self.stale);
        }

        self.stale = std::mem::take(&mut self.spare);
    }

    fn take(&mut self, width: u32, height: u32, usage: TextureUsages) -> Option<Texture> {
        let fits = |texture: &Texture| {
            texture.width() == width && texture.height() == height && texture.usage() == usage
        };

        if let Some(index) = self.stale.iter().position(fits) {
            return Some(self.stale.swap_remove(index));
        }

        let index = self.spare.iter().position(fits)?;

        Some(self.spare.swap_remove(index))
    }

    fn clear(&self, canvas: &Texture) {
        let view = texture::view(canvas);

        let attachment = RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(Color::TRANSPARENT),
                store: StoreOp::Store,
            },
        };

        let mut encoder = self.device.create_command_encoder(&Default::default());

        encoder.begin_render_pass(&RenderPassDescriptor {
            color_attachments: &[Some(attachment)],
            ..Default::default()
        });

        self.queue.submit([encoder.finish()]);
    }
}
