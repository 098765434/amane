#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo)]

/*
 * moving pictures handed in from another thread, like a decoded video
 * stream: the newest frame replaces the last, and only the windows showing
 * the feed are drawn again for it
 */

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use crate::changes::{self, Source};

static NEXT: AtomicU64 = AtomicU64::new(1);

// how a frame's numbers turn into colours
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Colors {
    // high definition video's, which phones and cameras send
    #[default]
    Bt709,

    // standard definition video's
    Bt601,
}

/*
 * one picture in NV12: a byte of brightness for every pixel, then a byte
 * pair of colour for every two by two; both rows are packed tightly, so y
 * is width × height long and uv is width × height / 2 (sizes rounded up to even)
 */
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub y: Vec<u8>,
    pub uv: Vec<u8>,
    pub colors: Colors,

    // brightness runs 0 to 255, not video's 16 to 235
    pub full_range: bool,
}

impl Frame {
    // the colour plane's size in pairs
    pub fn chroma_size(&self) -> (u32, u32) {
        (self.width.div_ceil(2), self.height.div_ceil(2))
    }

    // whether the planes are as long as the size says
    pub fn is_whole(&self) -> bool {
        let (chroma_width, chroma_height) = self.chroma_size();

        self.width > 0
            && self.height > 0
            && self.y.len() == (self.width * self.height) as usize
            && self.uv.len() == (chroma_width * chroma_height * 2) as usize
    }
}

struct Feed {
    id: u64,
    newest: Mutex<Option<(u64, Arc<Frame>)>>,
    count: AtomicU64,
}

// where frames go in; clones share the one feed, and any thread can publish to it
#[derive(Clone)]
pub struct VideoFeed(Arc<Feed>);

impl Default for VideoFeed {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoFeed {
    pub fn new() -> Self {
        Self(Arc::new(Feed {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            newest: Mutex::new(None),
            count: AtomicU64::new(0),
        }))
    }

    // replaces what it shows; a frame whose planes don't fit its size is dropped
    pub fn publish(&self, frame: Frame) {
        if !frame.is_whole() {
            return;
        }

        let number = self.0.count.fetch_add(1, Ordering::Relaxed) + 1;

        *self.0.newest.lock().unwrap_or_else(PoisonError::into_inner) =
            Some((number, Arc::new(frame)));

        changes::mark_source(Source::Video(self.0.id));
    }

    // shows nothing until the next frame
    pub fn clear(&self) {
        *self.0.newest.lock().unwrap_or_else(PoisonError::into_inner) = None;

        changes::mark_source(Source::Video(self.0.id));
    }

    // the newest frame's size, none before the first
    pub fn size(&self) -> Option<(u32, u32)> {
        self.0
            .newest
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|(_, frame)| (frame.width, frame.height))
    }

    // how many frames came in all
    pub fn frames(&self) -> u64 {
        self.0.count.load(Ordering::Relaxed)
    }

    // the frame to draw now, noting that the window drawing it shows this feed
    pub(crate) fn current(&self) -> Option<Picture> {
        changes::note_source(Source::Video(self.0.id));

        let newest = self.0.newest.lock().unwrap_or_else(PoisonError::into_inner);

        newest.as_ref().map(|(number, frame)| Picture {
            feed: self.0.id,
            number: *number,
            frame: Arc::clone(frame),
        })
    }
}

// one frame as a draw command carries it; made by the Video widget
#[derive(Debug, Clone)]
pub struct Picture {
    pub(crate) feed: u64,
    pub(crate) number: u64,
    pub(crate) frame: Arc<Frame>,
}

// the same frame of the same feed; comparing the pixels would cost as much as drawing them
impl PartialEq for Picture {
    fn eq(&self, other: &Self) -> bool {
        self.feed == other.feed && self.number == other.number
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn frame(width: u32, height: u32) -> Frame {
        let chroma = width.div_ceil(2) * height.div_ceil(2) * 2;

        Frame {
            width,
            height,
            y: vec![16; (width * height) as usize],
            uv: vec![128; chroma as usize],
            colors: Colors::Bt709,
            full_range: false,
        }
    }

    #[test]
    fn the_newest_frame_replaces_the_last() {
        let feed = VideoFeed::new();

        assert!(feed.current().is_none());

        feed.publish(frame(4, 2));
        let first = feed.current().expect("a frame");

        feed.publish(frame(6, 4));
        let second = feed.current().expect("a frame");

        assert_ne!(first, second);
        assert_eq!(feed.size(), Some((6, 4)));
        assert_eq!(feed.frames(), 2);
    }

    #[test]
    fn a_frame_whose_planes_dont_fit_is_dropped() {
        let feed = VideoFeed::new();

        let mut broken = frame(4, 4);
        broken.uv.pop();

        feed.publish(broken);

        assert!(feed.current().is_none());
    }

    #[test]
    fn odd_sizes_round_the_colour_plane_up() {
        let odd = frame(5, 3);

        assert_eq!(odd.chroma_size(), (3, 2));
        assert!(odd.is_whole());
    }

    #[test]
    fn clones_share_one_feed() {
        let feed = VideoFeed::new();
        let other = feed.clone();

        other.publish(frame(2, 2));

        assert_eq!(feed.size(), Some((2, 2)));

        feed.clear();

        assert!(other.current().is_none());
    }
}
