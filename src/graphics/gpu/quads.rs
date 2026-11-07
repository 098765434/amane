mod batch;
mod clip;
mod draw;
mod pipeline;
// the fork's shapes and shadows
mod shapes;
mod video;

use std::collections::HashMap;

use vello::wgpu::{BindGroup, BindGroupLayout, RenderPipeline, Sampler, Texture};

use super::atlas::Atlas;

pub use clip::{Clip, Clips};
pub use shapes::Paint;

// how many numbers one quad takes: Quad in quads.wgsl has eleven rows of four
const QUAD_SIZE: usize = 11 * 4;

// what kind of quad it is, as quads.wgsl tells them apart
const SHAPE: f32 = 0.0;
const LETTER: f32 = 1.0;
const PICTURE: f32 = 2.0;
const SHADOW: f32 = 3.0;
const ARC: f32 = 4.0;
const SEGMENT: f32 = 5.0;
const TRAPEZOID: f32 = 6.0;
const OUTLINE: f32 = 7.0;
const VIDEO: f32 = 8.0;

// how far past its rectangle a shadow's blur is drawn, in deviations; vello cuts its own off there too
const SHADOW_REACH: f32 = 2.5;

// what a quad reads besides the atlas
#[derive(Clone, Copy, PartialEq, Eq)]
enum Bound {
    // an image, by where the loaded image lives
    Picture(usize),

    // a video feed's planes
    Video(u64),
}

// quads next to each other that read the same image, drawn in one go
struct Run {
    // what they read, none for quads that read no image
    picture: Option<Bound>,

    first: u32,
    count: u32,
}

/*
 * rounded rectangles, borders, letters, images, drop shadows, arcs, straight
 * lines, the area under a line and small filled outlines, gathered as they come and drawn in one
 * pass; this is most of what a shell shows, and the gpu draws it far faster
 * as plain triangles than vello does as vector shapes
 */
pub struct Quads {
    pipeline: RenderPipeline,
    erase_pipeline: RenderPipeline,
    inputs: BindGroupLayout,
    picture_inputs: BindGroupLayout,

    sampler: Sampler,

    atlas: Atlas,

    // images already on the gpu, keyed by where the loaded image lives
    pictures: HashMap<usize, BindGroup>,

    // video feeds' frames on the gpu
    videos: video::Videos,

    // what quads that read no image bind in the image's place
    blank: BindGroup,

    // the colour plane images bind, which they never read
    blank_colour: Texture,

    // every quad's numbers one after another, as quads.wgsl reads them
    waiting: Vec<f32>,

    // the corners of the filled outlines among them, x and y one after another
    points: Vec<f32>,
    runs: Vec<Run>,
}

impl Quads {
    // the feeds no window drew this frame let go of their planes
    pub fn forget_undrawn_videos(&mut self) {
        self.videos.forget_undrawn();
    }

    // lets go of everything a hidden window drew with: its images, its letters and the room for quads
    pub fn release(&mut self) {
        self.pictures.clear();
        self.videos = video::Videos::default();

        self.atlas.release();

        self.waiting = Vec::new();
        self.points = Vec::new();
        self.runs = Vec::new();
    }
}
