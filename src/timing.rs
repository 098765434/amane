use std::collections::{HashMap, VecDeque};
use std::env;
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::Instant;

// AMANE_FRAMES=1 prints a line for every drawn frame, to find where frame time goes
static ENABLED: LazyLock<bool> = LazyLock::new(|| env::var_os("AMANE_FRAMES").is_some());

pub fn enabled() -> bool {
    *ENABLED
}

// when a frame's steps finished, from the redraw starting to the gpu taking the frame
pub struct Timing {
    pub started: Instant,
    pub viewed: Instant,
    pub drawn: Instant,
    pub presented: Instant,
}

/*
 * gap is the time since this window's last frame, so a drop shows as a
 * gap near 33 ms or more; view, draw and gpu are this frame's own steps
 */
// a frame the gpu couldn't present, so nothing of it reached the screen
pub fn skipped(name: &str) {
    if *ENABLED {
        eprintln!("skipped {name}");
    }
}

pub fn log(name: &str, width: u32, height: u32, last: Option<Instant>, timing: &Timing) {
    if !*ENABLED {
        return;
    }

    let milliseconds = |from: Instant, to: Instant| to.duration_since(from).as_secs_f32() * 1000.0;

    let gap = match last {
        Some(last) => milliseconds(last, timing.started),
        None => 0.0,
    };

    eprintln!(
        "frame {name} {width}x{height} gap {gap:.1} view {:.1} draw {:.1} gpu {:.1}",
        milliseconds(timing.started, timing.viewed),
        milliseconds(timing.viewed, timing.drawn),
        milliseconds(timing.drawn, timing.presented),
    );
}

// how many frames the averages cover
const KEPT: usize = 30;

// the last frames of each window, by its name: gap, view, draw and present in milliseconds
static RECENT: LazyLock<Mutex<HashMap<String, VecDeque<[f32; 4]>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/*
 * how a window's last frames went, averaged over up to 30 of them;
 * present is the time to hand the frame to the gpu and show it, not
 * the gpu's own work
 */
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FrameStats {
    pub fps: f32,
    pub view_ms: f32,
    pub draw_ms: f32,
    pub present_ms: f32,
}

pub fn record(name: &str, last: Option<Instant>, timing: &Timing) {
    let milliseconds = |from: Instant, to: Instant| to.duration_since(from).as_secs_f32() * 1000.0;

    let frame = [
        last.map_or(0.0, |last| milliseconds(last, timing.started)),
        milliseconds(timing.started, timing.viewed),
        milliseconds(timing.viewed, timing.drawn),
        milliseconds(timing.drawn, timing.presented),
    ];

    let mut recent = RECENT.lock().unwrap_or_else(PoisonError::into_inner);
    let frames = recent.entry(name.to_owned()).or_default();

    if frames.len() == KEPT {
        frames.pop_front();
    }

    frames.push_back(frame);
}

/*
 * a layer window by its namespace, a normal window by its name as
 * current_window() gives it; none before the window has drawn a frame
 */
pub fn frame_stats(window: &str) -> Option<FrameStats> {
    let recent = RECENT.lock().unwrap_or_else(PoisonError::into_inner);

    let frames = recent
        .get(window)
        .or_else(|| recent.get(&format!("normal {window}")))?;

    if frames.is_empty() {
        return None;
    }

    let average = |index: usize| {
        let values: Vec<f32> = frames
            .iter()
            .map(|frame| frame[index])
            .filter(|value| index != 0 || *value > 0.0)
            .collect();

        if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<f32>() / values.len() as f32
        }
    };

    let gap = average(0);

    Some(FrameStats {
        fps: if gap > 0.0 { 1000.0 / gap } else { 0.0 },
        view_ms: average(1),
        draw_ms: average(2),
        present_ms: average(3),
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn averages_a_window_s_last_frames() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);

        let mut last = None;

        for frame in 0..3u64 {
            let begun = frame * 10;

            let timing = Timing {
                started: at(begun),
                viewed: at(begun + 1),
                drawn: at(begun + 3),
                presented: at(begun + 4),
            };

            record("normal test-stats", last, &timing);
            last = Some(timing.started);
        }

        let stats = frame_stats("test-stats").unwrap();

        assert!((stats.fps - 100.0).abs() < 1.0, "{stats:?}");
        assert!((stats.draw_ms - 2.0).abs() < 0.1, "{stats:?}");
        assert!(frame_stats("nothing-drawn").is_none());
    }
}
