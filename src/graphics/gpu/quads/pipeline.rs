use std::collections::HashMap;

use vello::wgpu::{
    AddressMode, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout,
    BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource, BindingType, BlendState,
    BufferBindingType, ColorTargetState, ColorWrites, Device, Extent3d, FilterMode, FragmentState,
    MipmapFilterMode, Origin3d, PipelineLayoutDescriptor, Queue, RenderPipelineDescriptor, Sampler,
    SamplerBindingType, SamplerDescriptor, ShaderStages, TexelCopyBufferLayout,
    TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor, TextureDimension,
    TextureFormat, TextureSampleType, TextureUsages, TextureViewDimension, VertexState,
    include_wgsl,
};

use crate::graphics::gpu::atlas::Atlas;
use crate::graphics::gpu::{picture, texture};
use crate::graphics::image::Bitmap;

use super::Quads;

impl Quads {
    pub fn new(device: &Device, queue: &Queue) -> Self {
        let inputs = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                quads_entry(),
                canvas_entry(),
                texture_entry(2, false),
                points_entry(),
            ],
        });

        let picture_inputs = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                texture_entry(0, true),
                sampler_entry(),
                texture_entry(2, true),
            ],
        });

        let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&inputs), Some(&picture_inputs)],
            immediate_size: 0,
        });

        let module = device.create_shader_module(include_wgsl!("../quads.wgsl"));

        // the canvas holds premultiplied colors, so the quads land on it the same way
        let pipeline = |blend| {
            device.create_render_pipeline(&RenderPipelineDescriptor {
                label: None,
                layout: Some(&layout),
                vertex: VertexState {
                    module: &module,
                    entry_point: Some("vertex"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(FragmentState {
                    module: &module,
                    entry_point: Some("fragment"),
                    compilation_options: Default::default(),
                    targets: &[Some(ColorTargetState {
                        format: TextureFormat::Rgba8Unorm,
                        blend: Some(blend),
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };

        let erase_pipeline = pipeline(super::super::cut::erase_blend());
        let pipeline = pipeline(BlendState::PREMULTIPLIED_ALPHA_BLENDING);

        // smooth between pixels and between the prepared sizes
        let sampler = device.create_sampler(&SamplerDescriptor {
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: MipmapFilterMode::Linear,
            ..Default::default()
        });

        let empty = Bitmap::empty();
        let empty = picture::upload(device, queue, &empty);

        let blank_colour = empty_colour(device, queue);

        let blank = picture_group(device, &picture_inputs, &empty, &blank_colour, &sampler);

        Self {
            pipeline,
            erase_pipeline,
            inputs,
            picture_inputs,
            sampler,
            atlas: Atlas::new(),
            pictures: HashMap::new(),
            videos: Default::default(),
            blank,
            blank_colour,
            waiting: Vec::new(),
            points: Vec::new(),
            runs: Vec::new(),
        }
    }
}

pub fn picture_group(
    device: &Device,
    layout: &BindGroupLayout,
    texture: &Texture,
    // a video's colour plane, beside its brightness in texture; images bind a blank one
    colour: &Texture,
    sampler: &Sampler,
) -> BindGroup {
    let view = texture::view(texture);
    let colour = texture::view(colour);

    device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(&view),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Sampler(sampler),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::TextureView(&colour),
            },
        ],
    })
}

// one pixel of neutral colour, for the binding images don't read
fn empty_colour(device: &Device, queue: &Queue) -> Texture {
    let texture = device.create_texture(&TextureDescriptor {
        label: None,
        size: Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rg8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &[128, 128],
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(2),
            rows_per_image: Some(1),
        },
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );

    texture
}

fn quads_entry() -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding: 0,
        visibility: ShaderStages::VERTEX_FRAGMENT,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

// the corners of the filled outlines
fn points_entry() -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding: 3,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn canvas_entry() -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding: 1,
        visibility: ShaderStages::VERTEX,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_entry(binding: u32, filterable: bool) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Texture {
            sample_type: TextureSampleType::Float { filterable },
            view_dimension: TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry() -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding: 1,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Sampler(SamplerBindingType::Filtering),
        count: None,
    }
}
