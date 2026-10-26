use std::path::Path;
use std::sync::LazyLock;
use std::time::Instant;

use vello::wgpu::util::{BufferInitDescriptor, DeviceExt};
use vello::wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindingResource, Buffer, BufferUsages, LoadOp, Operations,
    RenderPassColorAttachment, RenderPassDescriptor, StoreOp, Texture, TextureView,
};

use crate::graphics::{self, Area, Transform, VALUE_ROWS, shader_files};
use crate::style::INPUTS;

use super::shader::{Kind, Loaded, Shader};
use super::{Gpu, texture};

// every shader's time counts from when the first one was drawn
static START: LazyLock<Instant> = LazyLock::new(Instant::now);

impl Gpu {
    /*
     * with rounded corners the shader draws on a canvas of its own, which is
     * trimmed to the outline and laid over what the commands before it drew;
     * with square ones it blends straight onto the canvas, saving three full
     * size passes and a texture every frame
     */
    pub(super) fn shade(
        &mut self,
        canvas: &Texture,
        shader: &Path,
        values: &[[f32; 4]],
        area: Area,
        outline: Option<&graphics::BezierPath>,
        transform: Transform,
    ) {
        let Some(outline) = outline else {
            self.draw_shader(canvas, shader, Kind::Plain, values, area, transform, &[]);

            return;
        };

        let layer = self.take_canvas(canvas.width(), canvas.height());

        self.draw_shader(&layer, shader, Kind::Plain, values, area, transform, &[]);

        self.trim(&layer, outline, transform);

        self.lay(&layer, canvas, 1.0, false);

        self.give_back(layer);
    }

    /*
     * compiled the first time a path is drawn, and again once its file is
     * saved; false when the file has never compiled, so there's nothing to draw
     */
    pub(super) fn load_shader(&mut self, path: &Path, kind: Kind) -> bool {
        let generation = shader_files::generation();

        let kept = self.shaders.get(path).and_then(|kinds| kinds.get(&kind));

        if let Some(loaded) = kept
            && loaded.generation == generation
        {
            return loaded.shader.is_some();
        }

        shader_files::watch(path);

        let compiled = Shader::load(&self.device, path, kind);

        let loaded = self
            .shaders
            .entry(path.to_path_buf())
            .or_default()
            .entry(kind)
            .or_insert(Loaded {
                shader: None,
                generation,
            });

        loaded.generation = generation;

        match compiled {
            Ok(shader) => {
                loaded.shader = Some(shader);

                shader_files::report(path, None);
            }

            Err(error) => {
                eprintln!("amane: {error}");

                shader_files::report(path, Some(error));
            }
        }

        loaded.shader.is_some()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_shader(
        &mut self,
        layer: &Texture,
        path: &Path,
        kind: Kind,
        values: &[[f32; 4]],
        area: Area,
        transform: Transform,
        inputs: &[&Texture],
    ) {
        if !self.load_shader(path, kind) {
            return;
        }

        let Some(shader) = self
            .shaders
            .get(path)
            .and_then(|kinds| kinds.get(&kind))
            .and_then(|loaded| loaded.shader.as_ref())
        else {
            return;
        };

        let time = START.elapsed().as_secs_f32();

        let size = self.uniform([area.width, area.height, 0.0, 0.0]);
        let time = self.uniform([time, 0.0, 0.0, 0.0]);

        let placement = self.uniform_rows([
            [area.x, area.y, area.width, area.height],
            [transform.sx, transform.kx, transform.tx, 0.0],
            [transform.ky, transform.sy, transform.ty, 0.0],
            [layer.width() as f32, layer.height() as f32, 0.0, 0.0],
        ]);

        // the shader always reads every row, so the ones left out are 0
        let mut rows = [[0.0; 4]; VALUE_ROWS];

        for (row, value) in rows.iter_mut().zip(values) {
            *row = *value;
        }

        let values = self.uniform_rows(rows);

        // how many real pixels one logical pixel covers, for passes that reach in logical pixels
        let scale = transform.even_scale().unwrap_or_else(|| {
            (transform.sx * transform.sy - transform.kx * transform.ky)
                .abs()
                .sqrt()
        });

        let scale = self.uniform([scale, 0.0, 0.0, 0.0]);

        let mut entries = vec![
            BindGroupEntry {
                binding: 0,
                resource: size.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: time.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: placement.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 3,
                resource: values.as_entire_binding(),
            },
        ];

        if kind != Kind::Plain {
            entries.push(BindGroupEntry {
                binding: 4,
                resource: scale.as_entire_binding(),
            });
        }

        let inputs_group = self.device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &shader.inputs,
            entries: &entries,
        });

        let textures_group = shader.textures.as_ref().map(|layout| {
            // inputs the pass wasn't given read as clear
            let views: Vec<TextureView> = (0..INPUTS)
                .map(|index| texture::view(inputs.get(index).copied().unwrap_or(&self.blank)))
                .collect();

            let mut entries = vec![BindGroupEntry {
                binding: 0,
                resource: BindingResource::Sampler(&self.input_sampler),
            }];

            for (index, view) in views.iter().enumerate() {
                entries.push(BindGroupEntry {
                    binding: index as u32 + 1,
                    resource: BindingResource::TextureView(view),
                });
            }

            self.device.create_bind_group(&BindGroupDescriptor {
                label: None,
                layout,
                entries: &entries,
            })
        });

        let target = texture::view(layer);

        let attachment = RenderPassColorAttachment {
            view: &target,
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Load,
                store: StoreOp::Store,
            },
        };

        let mut encoder = self.device.create_command_encoder(&Default::default());

        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                color_attachments: &[Some(attachment)],
                ..Default::default()
            });

            pass.set_pipeline(&shader.pipeline);

            pass.set_bind_group(0, &inputs_group, &[]);

            if let Some(textures) = &textures_group {
                pass.set_bind_group(1, textures, &[]);
            }

            // the six corners of the rectangle's two triangles
            pass.draw(0..6, 0..1);
        }

        self.queue.submit([encoder.finish()]);
    }

    fn uniform(&self, values: [f32; 4]) -> Buffer {
        self.uniform_rows([values])
    }

    fn uniform_rows<const ROWS: usize>(&self, rows: [[f32; 4]; ROWS]) -> Buffer {
        let mut bytes = Vec::new();

        for row in rows {
            for value in row {
                bytes.extend(value.to_ne_bytes());
            }
        }

        self.device.create_buffer_init(&BufferInitDescriptor {
            label: None,
            contents: &bytes,
            usage: BufferUsages::UNIFORM,
        })
    }
}
