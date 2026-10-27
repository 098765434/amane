use vello::wgpu::util::{BufferInitDescriptor, DeviceExt};
use vello::wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindingResource, BufferUsages, Device, LoadOp, Operations,
    Queue, RenderPassColorAttachment, RenderPassDescriptor, StoreOp, Texture,
};

use crate::graphics::gpu::texture;

use super::{Clip, Quads};

impl Quads {
    // before any letter of the frame is placed
    pub fn start_frame(&mut self) {
        self.atlas.start_frame();
    }

    // draws everything gathered onto the canvas, over what is already there
    pub fn draw(&mut self, device: &Device, queue: &Queue, canvas: &Texture) {
        self.draw_with(device, queue, canvas, false);
    }

    pub fn erase(
        &mut self,
        device: &Device,
        queue: &Queue,
        canvas: &Texture,
        shape: Clip,
        strength: f32,
    ) {
        self.mask(shape, strength);

        self.draw_with(device, queue, canvas, true);
    }

    fn draw_with(&mut self, device: &Device, queue: &Queue, canvas: &Texture, erase: bool) {
        if self.waiting.is_empty() {
            return;
        }

        let numbers: Vec<u8> = self
            .waiting
            .drain(..)
            .flat_map(|value| value.to_ne_bytes())
            .collect();

        let quads = device.create_buffer_init(&BufferInitDescriptor {
            label: None,
            contents: &numbers,
            usage: BufferUsages::STORAGE,
        });

        let size: Vec<u8> = [canvas.width() as f32, canvas.height() as f32, 0.0, 0.0]
            .iter()
            .flat_map(|value| value.to_ne_bytes())
            .collect();

        let size = device.create_buffer_init(&BufferInitDescriptor {
            label: None,
            contents: &size,
            usage: BufferUsages::UNIFORM,
        });

        let atlas = texture::view(self.atlas.texture(device));

        let inputs = device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.inputs,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: quads.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: size.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureView(&atlas),
                },
            ],
        });

        let target = texture::view(canvas);

        let mut encoder = device.create_command_encoder(&Default::default());

        {
            let attachment = RenderPassColorAttachment {
                view: &target,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            };

            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                color_attachments: &[Some(attachment)],
                ..Default::default()
            });

            pass.set_pipeline(if erase {
                &self.erase_pipeline
            } else {
                &self.pipeline
            });

            pass.set_bind_group(0, &inputs, &[]);

            for run in self.runs.drain(..) {
                let picture = run
                    .picture
                    .and_then(|key| self.pictures.get(&key))
                    .unwrap_or(&self.blank);

                pass.set_bind_group(1, picture, &[]);

                // six corners make each quad's two triangles
                pass.draw(0..6, run.first..run.first + run.count);
            }
        }

        queue.submit([encoder.finish()]);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use vello::wgpu::{
        BufferDescriptor, BufferUsages, DeviceDescriptor, Extent3d, Instance, InstanceDescriptor,
        MapMode, PollType, RequestAdapterOptions, TexelCopyBufferInfo, TexelCopyBufferLayout,
    };

    use crate::graphics::gpu::wait::wait;
    use crate::graphics::{Area, Color, Corners};

    use super::super::Clips;
    use super::*;

    #[test]
    #[ignore = "requires a graphics adapter"]
    fn erases_rounded_masks_without_changing_later_drawing() {
        let instance = Instance::new(InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = wait(instance.request_adapter(&RequestAdapterOptions::default()))
            .expect("failed to find a gpu");
        let (device, queue) =
            wait(adapter.request_device(&DeviceDescriptor::default())).expect("failed to open gpu");
        let canvas = texture::canvas(&device, 16, 16);
        let mut quads = Quads::new(&device, &queue);

        quads.rectangle(
            Area::new(0.0, 0.0, 16.0, 16.0),
            Corners::default(),
            Color::BLUE,
            Clips::default(),
        );
        quads.draw(&device, &queue, &canvas);
        quads.erase(
            &device,
            &queue,
            &canvas,
            Clip {
                area: Area::new(4.0, 4.0, 8.0, 8.0),
                radius: Corners::from(4.0),
            },
            1.0,
        );
        quads.erase(
            &device,
            &queue,
            &canvas,
            Clip {
                area: Area::new(0.0, 0.0, 4.0, 4.0),
                radius: Corners::default(),
            },
            0.25,
        );
        quads.rectangle(
            Area::new(0.0, 12.0, 16.0, 4.0),
            Corners::default(),
            Color::RED,
            Clips::default(),
        );
        quads.draw(&device, &queue, &canvas);

        let pixels = pixels(&device, &queue, &canvas, 16, 16);
        let pixel = |x: usize, y: usize| &pixels[y * 256 + x * 4..y * 256 + x * 4 + 4];
        assert_eq!(pixel(8, 8), [0, 0, 0, 0]);
        assert_eq!(pixel(4, 4), [0, 0, 255, 255]);
        assert_eq!(pixel(1, 1), [0, 0, 191, 191]);
        assert_eq!(pixel(8, 14), [255, 0, 0, 255]);
    }

    // the shadow quad is vello's blurred rounded rectangle, pixel for pixel
    #[test]
    #[ignore = "requires a graphics adapter"]
    fn draws_a_shadow_as_vello_does() {
        use std::num::NonZeroUsize;

        use vello::kurbo::{Affine, Rect};
        use vello::peniko;
        use vello::wgpu::{TextureDescriptor, TextureDimension, TextureFormat, TextureUsages};
        use vello::{AaConfig, AaSupport, RenderParams, RendererOptions, Scene};

        let instance = Instance::new(InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = wait(instance.request_adapter(&RequestAdapterOptions::default()))
            .expect("failed to find a gpu");
        let (device, queue) =
            wait(adapter.request_device(&DeviceDescriptor::default())).expect("failed to open gpu");

        let area = Area::new(16.0, 12.0, 32.0, 28.0);
        let (radius, deviation) = (8.0, 4.0);

        // by the quads, onto a cleared canvas: premultiplied, so black's alpha is the amount
        let canvas = texture::canvas(&device, 64, 64);
        let mut quads = Quads::new(&device, &queue);
        quads.shadow(area, radius, deviation, Color::BLACK, Clips::default());
        quads.draw(&device, &queue, &canvas);
        let by_quads = pixels(&device, &queue, &canvas, 64, 64);

        // by vello, the way shadow.rs draws them; vello takes the blur at each pixel's
        // corner where the quads take their pixel's middle, so its rectangle is moved half
        // a pixel to land on the same pixels
        let target = device.create_texture(&TextureDescriptor {
            label: None,
            size: Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::STORAGE_BINDING | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let options = RendererOptions {
            antialiasing_support: AaSupport::area_only(),
            num_init_threads: NonZeroUsize::new(1),
            ..RendererOptions::default()
        };
        let mut vello = vello::Renderer::new(&device, options).expect("failed to start vello");
        let mut scene = Scene::new();
        scene.draw_blurred_rounded_rect(
            Affine::IDENTITY,
            Rect::new(
                f64::from(area.x) - 0.5,
                f64::from(area.y) - 0.5,
                f64::from(area.x + area.width) - 0.5,
                f64::from(area.y + area.height) - 0.5,
            ),
            peniko::Color::BLACK,
            f64::from(radius),
            f64::from(deviation),
        );
        let params = RenderParams {
            base_color: peniko::Color::TRANSPARENT,
            width: 64,
            height: 64,
            antialiasing_method: AaConfig::Area,
        };
        vello
            .render_to_texture(&device, &queue, &scene, &texture::view(&target), &params)
            .expect("failed to draw scene");
        let by_vello = pixels(&device, &queue, &target, 64, 64);

        let alpha = |pixels: &[u8], x: usize, y: usize| i32::from(pixels[y * 256 + x * 4 + 3]);
        let mut worst = 0;
        for y in 0..64 {
            for x in 0..64 {
                worst = worst.max((alpha(&by_quads, x, y) - alpha(&by_vello, x, y)).abs());
            }
        }
        // the middle is nearly solid, the far corner of the reach nearly clear, the edge half
        assert!(
            alpha(&by_quads, 32, 26) > 240,
            "{}",
            alpha(&by_quads, 32, 26)
        );
        assert!(alpha(&by_quads, 7, 3) < 8, "{}", alpha(&by_quads, 7, 3));
        let edge = alpha(&by_quads, 32, 12);
        assert!((100..160).contains(&edge), "{edge}");
        assert!(
            worst <= 2,
            "the quads' shadow differs from vello's by {worst}/255"
        );
    }

    // the texture's rows, 256 bytes each (64 pixels of rgba8 at most)
    fn pixels(
        device: &Device,
        queue: &Queue,
        texture: &Texture,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let readback = device.create_buffer(&BufferDescriptor {
            label: None,
            size: 256 * u64::from(height),
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            TexelCopyBufferInfo {
                buffer: &readback,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(height),
                },
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let (send, receive) = mpsc::channel();
        readback.slice(..).map_async(MapMode::Read, move |result| {
            send.send(result).unwrap();
        });
        device.poll(PollType::wait_indefinitely()).unwrap();
        receive.recv().unwrap().unwrap();

        readback.slice(..).get_mapped_range().unwrap().to_vec()
    }
}
