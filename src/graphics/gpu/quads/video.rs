#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo)]

/*
 * video frames as two planes on the gpu, brightness and colour, kept per
 * feed and written over in place as frames come; the quad turns them into
 * colours as it draws, so a frame is never converted or copied on the cpu
 */

use std::collections::{HashMap, HashSet};

use vello::wgpu::{
    BindGroup, Device, Extent3d, Origin3d, Queue, TexelCopyBufferLayout, TexelCopyTextureInfo,
    Texture, TextureAspect, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};

use crate::graphics::Area;
use crate::video::{Colors, Frame, Picture};

use super::pipeline::picture_group;
use super::{Bound, Clips, Quads, VIDEO};

// one feed's planes on the gpu
pub struct Planes {
    // the frame they hold, so an unchanged one isn't written again
    number: u64,
    width: u32,
    height: u32,
    brightness: Texture,
    colour: Texture,
    group: BindGroup,
}

#[derive(Default)]
pub struct Videos {
    pub planes: HashMap<u64, Planes>,

    // the feeds drawn since the last frame ended
    drawn: HashSet<u64>,
}

impl Videos {
    pub fn group(&self, feed: u64) -> Option<&BindGroup> {
        self.planes.get(&feed).map(|planes| &planes.group)
    }

    // a feed no window drew this frame lets go of its planes; the next frame brings them back
    pub fn forget_undrawn(&mut self) {
        let drawn = std::mem::take(&mut self.drawn);

        self.planes.retain(|feed, _| drawn.contains(feed));
    }
}

impl Quads {
    pub fn video(
        &mut self,
        device: &Device,
        queue: &Queue,
        picture: &Picture,
        area: Area,
        clips: Clips,
    ) {
        let frame = &picture.frame;

        self.videos.drawn.insert(picture.feed);

        let resized = self
            .videos
            .planes
            .get(&picture.feed)
            .is_none_or(|planes| planes.width != frame.width || planes.height != frame.height);

        if resized {
            let (chroma_width, chroma_height) = frame.chroma_size();

            let brightness = plane(device, frame.width, frame.height, TextureFormat::R8Unorm);
            let colour = plane(device, chroma_width, chroma_height, TextureFormat::Rg8Unorm);

            let group = picture_group(
                device,
                &self.picture_inputs,
                &brightness,
                &colour,
                &self.sampler,
            );

            self.videos.planes.insert(
                picture.feed,
                Planes {
                    number: 0,
                    width: frame.width,
                    height: frame.height,
                    brightness,
                    colour,
                    group,
                },
            );
        }

        let Some(planes) = self.videos.planes.get_mut(&picture.feed) else {
            return;
        };

        if planes.number != picture.number {
            write(queue, &planes.brightness, frame, Plane::Brightness);
            write(queue, &planes.colour, frame, Plane::Colour);

            planes.number = picture.number;
        }

        let matrix = match frame.colors {
            Colors::Bt709 => 0.0,
            Colors::Bt601 => 1.0,
        };

        let range = if frame.full_range { 1.0 } else { 0.0 };

        self.push_with(
            area,
            ([0.0; 4], 0.0),
            [matrix, range],
            [1.0; 4],
            clips,
            VIDEO,
            [0.0, 0.0, 1.0, 1.0],
            Some(Bound::Video(picture.feed)),
        );
    }
}

enum Plane {
    Brightness,
    Colour,
}

fn plane(device: &Device, width: u32, height: u32, format: TextureFormat) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: None,
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn write(queue: &Queue, texture: &Texture, frame: &Frame, plane: Plane) {
    let (width, height, bytes, pixels) = match plane {
        Plane::Brightness => (frame.width, frame.height, 1, &frame.y),
        Plane::Colour => {
            let (width, height) = frame.chroma_size();

            (width, height, 2, &frame.uv)
        }
    };

    queue.write_texture(
        TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        pixels,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * bytes),
            rows_per_image: Some(height),
        },
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}
