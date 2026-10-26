use vello::wgpu::{
    AddressMode, Device, Extent3d, FilterMode, Queue, Sampler, SamplerDescriptor,
    TexelCopyBufferLayout, Texture, TextureDescriptor, TextureDimension, TextureFormat,
    TextureUsages,
};

use std::sync::Arc;

use crate::graphics::image::Bitmap;
use crate::graphics::{Area, Transform};
use crate::style::{Effect, Input};

use super::Gpu;
use super::shader::Kind;

impl Gpu {
    /*
     * each pass draws over the area and its margin into a texture of its own,
     * reading the content or earlier passes; the last pass is laid on the canvas
     */
    #[allow(clippy::too_many_arguments)]
    pub(super) fn run_effect(
        &mut self,
        content: &Texture,
        effect: &Effect,
        images: &[Option<Arc<Bitmap>>],
        area: Area,
        transform: Transform,
        canvas: &Texture,
    ) {
        let (width, height) = (canvas.width(), canvas.height());

        let last = effect.passes.len() - 1;

        let kind = |index: usize| {
            if index == last {
                Kind::Last
            } else {
                Kind::Pass
            }
        };

        // every pass is loaded, so each broken one is reported, not just the first
        let loaded: Vec<bool> = effect
            .passes
            .iter()
            .enumerate()
            .map(|(index, pass)| self.load_shader(&pass.shader, kind(index)))
            .collect();

        // a broken chain shows the drawing as it is, so there's still something to look at
        if loaded.contains(&false) {
            self.lay(content, canvas, 1.0, false);

            return;
        }

        // copied before the result is laid over it, which would change what's read
        let backdrop = effect.reads_backdrop().then(|| {
            let backdrop = self.take_canvas(width, height);

            self.lay(canvas, &backdrop, 1.0, false);

            backdrop
        });

        let images: Vec<Option<Texture>> = images
            .iter()
            .map(|image| image.as_ref().map(|image| self.input_image(image)))
            .collect();

        let blank = self.blank.clone();

        let margin = effect.margin;

        let reach = Area::new(
            area.x - margin,
            area.y - margin,
            area.width + margin * 2.0,
            area.height + margin * 2.0,
        );

        let mut results: Vec<Texture> = Vec::with_capacity(effect.passes.len());

        // images are counted across every pass, in the order the effect lists them
        let mut next_image = images.iter();

        for (index, pass) in effect.passes.iter().enumerate() {
            let kind = kind(index);

            let target = match kind {
                Kind::Last => self.take_canvas(width, height),
                _ => self.take_field(width, height),
            };

            let mut inputs: Vec<&Texture> = Vec::with_capacity(pass.inputs.len());

            for input in &pass.inputs {
                let texture = match input {
                    Input::Content => content,
                    Input::Pass(earlier) => &results[*earlier],
                    Input::Backdrop => backdrop.as_ref().unwrap_or(&blank),
                    Input::Image { .. } => {
                        next_image.next().and_then(Option::as_ref).unwrap_or(&blank)
                    }
                };

                inputs.push(texture);
            }

            self.draw_shader(
                &target,
                &pass.shader,
                kind,
                &effect.values,
                reach,
                transform,
                &inputs,
            );

            results.push(target);
        }

        if let Some(shown) = results.pop() {
            self.lay(&shown, canvas, 1.0, false);

            self.give_back(shown);
        }

        for result in results {
            self.give_back(result);
        }

        if let Some(backdrop) = backdrop {
            self.give_back(backdrop);
        }
    }

    // an image input's pixels, sent to the gpu the first frame it's read and kept while it is
    fn input_image(&mut self, image: &Arc<Bitmap>) -> Texture {
        let key = Arc::as_ptr(image) as usize;

        self.input_images_read.insert(key);

        if let Some((_, texture)) = self.input_images.get(&key) {
            return texture.clone();
        }

        let size = Extent3d {
            width: image.width(),
            height: image.height(),
            depth_or_array_layers: 1,
        };

        let texture = self.device.create_texture(&TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });

        self.queue.write_texture(
            texture.as_image_copy(),
            &image.pixels,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image.width() * 4),
                rows_per_image: None,
            },
            size,
        );

        self.input_images
            .insert(key, (image.clone(), texture.clone()));

        texture
    }

    // image inputs no effect read this frame are let go
    pub(super) fn forget_unread_inputs(&mut self) {
        let read = std::mem::take(&mut self.input_images_read);

        self.input_images.retain(|key, _| read.contains(key));
    }
}

// inputs are read between pixels and past their edge as the edge itself
pub fn sampler(device: &Device) -> Sampler {
    device.create_sampler(&SamplerDescriptor {
        label: None,
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..Default::default()
    })
}

// one clear pixel, read by a pass in place of an input it wasn't given
pub fn blank(device: &Device, queue: &Queue) -> Texture {
    let size = Extent3d {
        width: 1,
        height: 1,
        depth_or_array_layers: 1,
    };

    let texture = device.create_texture(&TextureDescriptor {
        label: None,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        texture.as_image_copy(),
        &[0, 0, 0, 0],
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: None,
        },
        size,
    );

    texture
}
