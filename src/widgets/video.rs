#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo)]

use crate::graphics::{Area, Corners, Renderer};
use crate::style::Fit;
use crate::{Point, Size, VideoFeed, Widget};

/*
 * a feed's newest frame, shown whole in the middle of its area (or covering
 * it); nothing shows before the first frame
 */
pub struct Video {
    feed: VideoFeed,
    width: Size,
    height: Size,
    fit: Fit,
    radius: f32,
}

impl Video {
    pub fn new(feed: &VideoFeed) -> Self {
        Self {
            feed: feed.clone(),
            width: Size::Parent,
            height: Size::Parent,
            fit: Fit::Contain,
            radius: 0.0,
        }
    }

    pub fn width(mut self, width: impl Into<Size>) -> Self {
        self.width = width.into();

        self
    }

    pub fn height(mut self, height: impl Into<Size>) -> Self {
        self.height = height.into();

        self
    }

    // fills the whole area, cutting off what spills past it
    pub fn cover(mut self) -> Self {
        self.fit = Fit::Cover;

        self
    }

    // the shown picture's corners
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;

        self
    }

    /*
     * the frame pixel under `point`, for a contained frame of `frame` size in
     * an area `width` by `height` with `point` measured from its top left;
     * none over the bars beside the picture
     */
    pub fn frame_point(width: f32, height: f32, frame: (u32, u32), point: Point) -> Option<Point> {
        let area = Area::new(0.0, 0.0, width, height);
        let placed = Fit::Contain.place(area, frame.0 as f32, frame.1 as f32);

        if placed.width <= 0.0 || placed.height <= 0.0 {
            return None;
        }

        let x = (point.x - placed.x) / placed.width * frame.0 as f32;
        let y = (point.y - placed.y) / placed.height * frame.1 as f32;

        let inside = (0.0..frame.0 as f32).contains(&x) && (0.0..frame.1 as f32).contains(&y);

        inside.then_some(Point { x, y })
    }
}

impl Widget for Video {
    fn width(&self) -> Size {
        self.width
    }

    fn height(&self) -> Size {
        self.height
    }

    fn draw(&self, renderer: &mut Renderer, area: Area) {
        let Some(picture) = self.feed.current() else {
            return;
        };

        let placement = self.fit.place(
            area,
            picture.frame.width as f32,
            picture.frame.height as f32,
        );

        // contained, the picture's own corners are rounded; covering, the area's are
        let shown = match self.fit {
            Fit::Cover => area,
            _ => placement,
        };

        renderer.video(shown, Corners::from(self.radius), picture, placement);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_maps_onto_the_contained_frame() {
        // a tall phone frame in a square area: bars on both sides
        let frame = (1080, 2400);
        let point = Video::frame_point(1000.0, 1000.0, frame, Point { x: 500.0, y: 500.0 })
            .expect("the middle is on the picture");

        assert!((point.x - 540.0).abs() < 0.5);
        assert!((point.y - 1200.0).abs() < 0.5);

        assert!(Video::frame_point(1000.0, 1000.0, frame, Point { x: 10.0, y: 500.0 }).is_none());
    }
}
