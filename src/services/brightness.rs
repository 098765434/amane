use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::thread;
use std::time::Duration;

use crate::compositor::{self, HdrPanel, niri::client};
use crate::services::uevent;
use crate::services::worker;
use crate::{Argument, Bus, Service};

const BACKLIGHTS: &str = "/sys/class/backlight";

const LOGIND: &str = "org.freedesktop.login1";
const SESSION_PATH: &str = "/org/freedesktop/login1/session/auto";
const SESSION: &str = "org.freedesktop.login1.Session";

// SDR white at 1 % in HDR, in nits: a dim room's white, never black
const HDR_DIMMEST: f64 = 25.0;

// SDR white at 100 % in HDR, unless the panel's peak is lower; about what
// the OLED's backlight reaches at full outside HDR
const HDR_BRIGHTEST: f64 = 500.0;

// how many polls apart the compositor is asked whether HDR is on, when it can't say itself
const HDR_EVERY: u32 = 4;

#[derive(Default)]
pub struct Brightness {
    // none on a desktop monitor without a backlight
    path: Option<PathBuf>,

    // 0 to 100
    percent: u8,

    // the built-in panel while it's in HDR, whose brightness is then its
    // SDR white rather than the backlight
    hdr: Option<HdrPanel>,
}

/*
 * the kernel sends a uevent when the backlight is set, and the niri fork says when its outputs
 * changed (HDR, SDR white), so nothing polls; without either, the old polls come back: the
 * backlight every half second, HDR every two
 */
impl Service for Brightness {
    fn new() -> Self {
        let mut brightness = Self {
            path: find(),
            hdr: compositor::hdr_panel(),
            ..Self::default()
        };

        brightness.update();

        brightness
    }

    fn interval() -> Duration {
        Duration::from_millis(500)
    }

    fn listen() {
        static OUTPUTS: Once = Once::new();
        let pushed_hdr = client::features().has("outputs-changed");
        if pushed_hdr {
            OUTPUTS.call_once(|| {
                thread::spawn(|| {
                    compositor::listen_outputs_changed(|| {
                        // asked outside the write: the answer can take up to two seconds
                        let hdr = compositor::hdr_panel();
                        let mut brightness = Self::write();
                        let changed = brightness.hdr != hdr;
                        brightness.hdr = hdr;
                        if !brightness.update() && !changed {
                            brightness.quiet();
                        }
                    });
                });
            });
        }

        if let Some(events) = uevent::listen("backlight") {
            // a set between new() and listening would otherwise wait for the next one
            Self::refresh_level();
            for event in events {
                if event.action == "change" {
                    Self::refresh_level();
                }
            }
            // the events stopped: poll from here on
        }

        Self::poll(pushed_hdr);
    }

    fn update(&mut self) -> bool {
        let before = self.percent;

        if let Some(panel) = &self.hdr {
            self.percent = hdr_percent(panel);
            return self.percent != before;
        }

        let Some(path) = &self.path else {
            return false;
        };

        let current = read(path, "brightness");
        let highest = read(path, "max_brightness");

        if highest == 0 {
            return false;
        }

        self.percent = (current * 100 / highest) as u8;

        self.percent != before
    }
}

impl Brightness {
    fn refresh_level() {
        let mut brightness = Self::write();
        if !brightness.update() {
            brightness.quiet();
        }
    }

    fn poll(pushed_hdr: bool) {
        let mut polls = 0;

        loop {
            thread::sleep(Self::interval());

            polls += 1;
            let hdr = (!pushed_hdr && polls >= HDR_EVERY).then(|| {
                polls = 0;
                compositor::hdr_panel()
            });

            let mut brightness = Self::write();
            if let Some(hdr) = hdr {
                brightness.hdr = hdr;
            }

            if !brightness.update() {
                brightness.quiet();
            }
        }
    }

    pub fn present(&self) -> bool {
        self.path.is_some() || self.hdr.is_some()
    }

    /// The built-in panel while it's in HDR, when [`set`](Self::set) moves
    /// its SDR white instead of the backlight.
    pub fn hdr(&self) -> Option<&HdrPanel> {
        self.hdr.as_ref()
    }

    pub fn percent(&self) -> u8 {
        self.percent
    }

    /*
     * the backlight file belongs to root, so logind writes it for us;
     * it allows that for whoever sits at the machine, with no password.
     * in HDR the compositor's SDR white moves instead, since the panel
     * then takes its light from the signal
     */
    pub fn set(percent: u8) {
        worker::run(move || {
            let panel = Brightness::read().hdr.clone();

            match panel {
                Some(panel) => write_sdr_white(panel, percent),
                None => write_level(percent),
            }
        });
    }
}

// the brightest SDR white this panel is given
fn hdr_top(panel: &HdrPanel) -> f64 {
    panel
        .peak
        .map_or(HDR_BRIGHTEST, |peak| peak.min(HDR_BRIGHTEST))
        .max(HDR_DIMMEST * 2.0)
}

/*
 * even steps of the eye rather than of light: each percent multiplies
 * SDR white by the same amount, from the dimmest at 1 % to the top at 100
 */
fn hdr_nits(panel: &HdrPanel, percent: u8) -> f64 {
    let at = f64::from(percent.clamp(1, 100) - 1) / 99.0;

    HDR_DIMMEST * (hdr_top(panel) / HDR_DIMMEST).powf(at)
}

fn hdr_percent(panel: &HdrPanel) -> u8 {
    let nits = panel.sdr_white.clamp(HDR_DIMMEST, hdr_top(panel));

    let at = (nits / HDR_DIMMEST).ln() / (hdr_top(panel) / HDR_DIMMEST).ln();

    (1.0 + 99.0 * at).round().clamp(1.0, 100.0) as u8
}

fn write_sdr_white(mut panel: HdrPanel, percent: u8) {
    let nits = hdr_nits(&panel, percent);

    if !compositor::set_sdr_white(&panel.output, nits) {
        return;
    }

    panel.sdr_white = nits;

    let mut brightness = Brightness::write();

    brightness.percent = hdr_percent(&panel);
    brightness.hdr = Some(panel);
}

fn write_level(percent: u8) {
    let Some(path) = Brightness::read().path.clone() else {
        return;
    };

    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return;
    };

    let highest = read(&path, "max_brightness");

    // never 0, a black screen is hard to undo without seeing it
    let percent = u64::from(percent.clamp(1, 100));

    let level = (highest * percent / 100) as u32;

    let arguments = [
        Argument::from("backlight"),
        Argument::from(name),
        Argument::from(level),
    ];

    Bus::system().call(LOGIND, SESSION_PATH, SESSION, "SetBrightness", &arguments);

    Brightness::write().update();
}

// the first backlight the kernel lists, a laptop usually has exactly one
fn find() -> Option<PathBuf> {
    let mut entries = fs::read_dir(BACKLIGHTS).ok()?.flatten();

    let first = entries.next()?;

    Some(first.path())
}

fn read(folder: &Path, name: &str) -> u64 {
    let text = fs::read_to_string(folder.join(name)).unwrap_or_default();

    text.trim().parse().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(sdr_white: f64) -> HdrPanel {
        HdrPanel {
            output: String::from("eDP-1"),
            sdr_white,
            peak: Some(1107.0),
        }
    }

    #[test]
    fn hdr_runs_from_the_dimmest_to_the_top() {
        assert!((hdr_nits(&panel(0.0), 1) - HDR_DIMMEST).abs() < 1e-9);
        assert!((hdr_nits(&panel(0.0), 100) - HDR_BRIGHTEST).abs() < 1e-9);
    }

    #[test]
    fn hdr_percent_reads_back_what_was_set() {
        for percent in 1..=100 {
            let nits = hdr_nits(&panel(0.0), percent);

            assert_eq!(hdr_percent(&panel(nits)), percent);
        }
    }

    #[test]
    fn a_dim_panel_still_has_room() {
        let dim = HdrPanel {
            peak: Some(10.0),
            ..panel(0.0)
        };

        assert!(hdr_top(&dim) > HDR_DIMMEST);
    }
}
