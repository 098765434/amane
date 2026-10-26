use std::borrow::Cow;
use std::fs;
use std::path::Path;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use vello::wgpu::naga::ShaderStage;
use vello::wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BlendState,
    BufferBindingType, ColorTargetState, ColorWrites, Device, ErrorFilter, FragmentState,
    PipelineLayoutDescriptor, RenderPipeline, RenderPipelineDescriptor, SamplerBindingType,
    ShaderModule, ShaderModuleDescriptor, ShaderSource, ShaderStages, TextureFormat,
    TextureSampleType, TextureViewDimension, VertexState,
};

use crate::style::INPUTS;

// amane's vertex half, which also gives wgsl shaders their inputs
const VERTEX: &str = include_str!("shader.wgsl");

const WGSL_INPUTS: &str = "
@group(0) @binding(0) var<uniform> size: vec2<f32>;
@group(0) @binding(1) var<uniform> time: f32;
@group(0) @binding(3) var<uniform> values: array<vec4<f32>, 64>;
";

// an effect pass also gets its inputs, and how many real pixels a logical one covers
const EFFECT_INPUTS: &str = "
@group(0) @binding(4) var<uniform> scale: f32;
@group(1) @binding(0) var input_sampler: sampler;
@group(1) @binding(1) var input0: texture_2d<f32>;
@group(1) @binding(2) var input1: texture_2d<f32>;
@group(1) @binding(3) var input2: texture_2d<f32>;
@group(1) @binding(4) var input3: texture_2d<f32>;
";

const GLSL_INPUTS: &str = "#version 450
layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 color;
layout(set = 0, binding = 0) uniform Size { vec2 size; };
layout(set = 0, binding = 1) uniform Time { float time; };
layout(set = 0, binding = 3) uniform Values { vec4 values[64]; };
";

// what a shader is compiled for, each with its own inputs and target
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    // a rectangle's shader, drawn over its fill
    Plain,

    // an effect pass whose result a later pass reads, kept exactly as written
    Pass,

    // an effect's last pass, shown in place of the drawing
    Last,
}

/*
 * a user's fragment shader over a rectangle: it gets uv from 0 to 1 across the rectangle,
 * size in logical pixels, time in seconds and the rectangle's values (64 is VALUE_ROWS),
 * and returns a plain, not premultiplied, color; effect passes also get their inputs
 */
pub struct Shader {
    pub pipeline: RenderPipeline,
    pub inputs: BindGroupLayout,

    // an effect pass's textures, none for a plain shader
    pub textures: Option<BindGroupLayout>,
}

impl Shader {
    // .glsl and .frag files are glsl, anything else is wgsl; the error says what went wrong
    pub fn load(device: &Device, path: &Path, kind: Kind) -> Result<Self, String> {
        let source = fs::read_to_string(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;

        let extension = path.extension().and_then(|extension| extension.to_str());

        let is_glsl = matches!(extension, Some("glsl" | "frag"));

        if is_glsl && kind != Kind::Plain {
            return Err(format!("{}: effect passes are wgsl only", path.display()));
        }

        // caught here instead of printed, so a broken shader is reported and drawn without
        let scope = device.push_error_scope(ErrorFilter::Validation);

        let shader = compile(device, &source, is_glsl, kind);

        match ready(scope.pop()) {
            Some(error) => Err(at_user_lines(&error.to_string(), path, kind, is_glsl)),
            None => Ok(shader),
        }
    }
}

fn compile(device: &Device, source: &str, is_glsl: bool, kind: Kind) -> Shader {
    let vertex = module(device, ShaderSource::Wgsl(Cow::Borrowed(VERTEX)));

    let fragment = if is_glsl {
        let shader = format!("{GLSL_INPUTS}{source}");

        module(
            device,
            ShaderSource::Glsl {
                shader: Cow::Owned(shader),
                stage: ShaderStage::Fragment,
                defines: Default::default(),
            },
        )
    } else {
        let effect_inputs = if kind == Kind::Plain {
            ""
        } else {
            EFFECT_INPUTS
        };

        // the vertex half is added so wgsl sees the same Placement and Corner types
        let shader = format!("{VERTEX}{WGSL_INPUTS}{effect_inputs}{source}");

        module(device, ShaderSource::Wgsl(Cow::Owned(shader)))
    };

    let mut entries = vec![
        uniform(0, ShaderStages::FRAGMENT),
        uniform(1, ShaderStages::FRAGMENT),
        uniform(2, ShaderStages::VERTEX_FRAGMENT),
        uniform(3, ShaderStages::FRAGMENT),
    ];

    if kind != Kind::Plain {
        entries.push(uniform(4, ShaderStages::FRAGMENT));
    }

    let inputs = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: None,
        entries: &entries,
    });

    let textures = (kind != Kind::Plain).then(|| texture_layout(device));

    let pipeline = pipeline(device, &inputs, textures.as_ref(), &vertex, &fragment, kind);

    Shader {
        pipeline,
        inputs,
        textures,
    }
}

/*
 * naga counts lines from the start of what it compiled, amane's half
 * included, so the place it points at is moved back into the user's file
 */
fn at_user_lines(error: &str, path: &Path, kind: Kind, is_glsl: bool) -> String {
    let added = if is_glsl {
        GLSL_INPUTS.to_string()
    } else if kind == Kind::Plain {
        format!("{VERTEX}{WGSL_INPUTS}")
    } else {
        format!("{VERTEX}{WGSL_INPUTS}{EFFECT_INPUTS}")
    };

    let added = added.matches('\n').count();

    let mut lines = Vec::new();

    for line in error.lines() {
        let place = ["┌─ wgsl:", "┌─ glsl:"]
            .iter()
            .find_map(|marker| Some((marker, line.split_once(marker)?)));

        let Some((marker, (indent, rest))) = place else {
            lines.push(line.to_string());

            continue;
        };

        let mut parts = rest.splitn(2, ':');

        let number = parts.next().and_then(|number| number.parse::<usize>().ok());

        match number {
            Some(number) if number > added => {
                let column = parts.next().unwrap_or("1");

                let marker = marker.trim_end_matches(['w', 'g', 's', 'l', ':']);

                lines.push(format!(
                    "{indent}{marker}{}:{}:{column}",
                    path.display(),
                    number - added
                ));
            }

            _ => lines.push(line.to_string()),
        }
    }

    format!("{}: {}", path.display(), lines.join("\n"))
}

// a native device settles its error scopes at once, so the future is ready on its first poll
fn ready<T>(future: impl Future<Output = Option<T>>) -> Option<T> {
    let mut context = Context::from_waker(Waker::noop());

    match pin!(future).poll(&mut context) {
        Poll::Ready(error) => error,
        Poll::Pending => None,
    }
}

fn module(device: &Device, source: ShaderSource) -> ShaderModule {
    device.create_shader_module(ShaderModuleDescriptor {
        label: Some("custom shader"),
        source,
    })
}

fn pipeline(
    device: &Device,
    inputs: &BindGroupLayout,
    textures: Option<&BindGroupLayout>,
    vertex: &ShaderModule,
    fragment: &ShaderModule,
    kind: Kind,
) -> RenderPipeline {
    let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(inputs), textures],
        immediate_size: 0,
    });

    /*
     * drawn onto a clear texture, so blending turns the plain color into a
     * premultiplied one; a pass between is read back as written instead, in
     * half floats so a blurred edge keeps more than 256 steps
     */
    let target = match kind {
        Kind::Plain | Kind::Last => ColorTargetState {
            format: TextureFormat::Rgba8Unorm,
            blend: Some(BlendState::ALPHA_BLENDING),
            write_mask: ColorWrites::ALL,
        },

        Kind::Pass => ColorTargetState {
            format: TextureFormat::Rgba16Float,
            blend: None,
            write_mask: ColorWrites::ALL,
        },
    };

    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: None,
        layout: Some(&layout),
        vertex: VertexState {
            module: vertex,
            entry_point: Some("vertex"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(FragmentState {
            module: fragment,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            targets: &[Some(target)],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn texture_layout(device: &Device) -> BindGroupLayout {
    let mut entries = vec![BindGroupLayoutEntry {
        binding: 0,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Sampler(SamplerBindingType::Filtering),
        count: None,
    }];

    for index in 0..INPUTS {
        entries.push(BindGroupLayoutEntry {
            binding: index as u32 + 1,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: true },
                view_dimension: TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
    }

    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: None,
        entries: &entries,
    })
}

fn uniform(binding: u32, visibility: ShaderStages) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

// a file's shader, kept through a failed recompile so the last good one still draws
pub struct Loaded {
    pub shader: Option<Shader>,

    // the shader files' generation it was compiled in
    pub generation: u64,
}
