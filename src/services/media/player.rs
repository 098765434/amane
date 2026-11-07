use std::time::{Duration, Instant};

use crate::Value;
use crate::services::worker;

use super::mpris;

// one open player, like spotify, mpv or a browser tab
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaPlayer {
    // the bus name like org.mpris.MediaPlayer2.spotify, which the controls talk to
    pub(crate) name: String,

    // what the player calls itself, like "Spotify"
    pub(crate) identity: String,

    pub(crate) title: String,
    pub(crate) artist: String,
    pub(crate) art_url: String,

    pub(crate) playing: bool,

    // where the track was when `read_at` was, and how fast it moves on in thousandths of real
    // time; players don't announce the position while playing, so it is worked out from these
    pub(crate) position: Duration,
    pub(crate) rate_milli: u32,
    pub(crate) read_at: Instant,
    pub(crate) length: Duration,
}

impl MediaPlayer {
    pub(crate) fn read(name: &str) -> Self {
        let properties = mpris::properties(name);
        let metadata = properties.get("Metadata");

        Self {
            name: String::from(name),
            identity: mpris::identity(name),

            title: String::from(metadata.get("xesam:title").text()),
            artist: first_artist(metadata.get("xesam:artist")),
            art_url: String::from(metadata.get("mpris:artUrl").text()),

            playing: properties.get("PlaybackStatus").text() == "Playing",

            position: micros(properties.get("Position")),
            rate_milli: rate_milli(properties.get("Rate")),
            read_at: Instant::now(),
            length: micros(metadata.get("mpris:length")),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn artist(&self) -> &str {
        &self.artist
    }

    // usually a file:// or https:// link to the cover
    pub fn art_url(&self) -> &str {
        &self.art_url
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    // worked out from the last reading while playing, never past the end
    pub fn position(&self) -> Duration {
        self.position_at(Instant::now())
    }

    fn position_at(&self, now: Instant) -> Duration {
        if !self.playing {
            return self.position;
        }

        let moved = now.saturating_duration_since(self.read_at) * self.rate_milli / 1000;
        let position = self.position + moved;

        if self.length > Duration::ZERO {
            position.min(self.length)
        } else {
            position
        }
    }

    pub fn length(&self) -> Duration {
        self.length
    }

    pub fn play_pause(&self) {
        self.send("PlayPause");
    }

    pub fn next(&self) {
        self.send("Next");
    }

    pub fn previous(&self) {
        self.send("Previous");
    }

    fn send(&self, method: &'static str) {
        let name = self.name.clone();

        worker::run(move || mpris::send(&name, method));
    }
}

// mpris gives a list of artists, a bar only has room for one
fn first_artist(artists: &Value) -> String {
    let Some(first) = artists.list().first() else {
        return String::new();
    };

    String::from(first.text())
}

// 1.0 when the player doesn't say, as the spec has it
fn rate_milli(rate: &Value) -> u32 {
    match rate {
        Value::Number(rate) if *rate >= 0.0 => (rate * 1000.0).round() as u32,
        _ => 1000,
    }
}

// mpris counts time in microseconds, and a missing value reads as 0
fn micros(value: &Value) -> Duration {
    let micros = value.number().max(0.0) as u64;

    Duration::from_micros(micros)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(playing: bool, rate_milli: u32, read_at: Instant) -> MediaPlayer {
        MediaPlayer {
            name: String::from("org.mpris.MediaPlayer2.test"),
            identity: String::from("Test"),
            title: String::from("Song"),
            artist: String::new(),
            art_url: String::new(),
            playing,
            position: Duration::from_secs(10),
            rate_milli,
            read_at,
            length: Duration::from_secs(60),
        }
    }

    #[test]
    fn works_out_the_position_from_the_last_reading() {
        let read_at = Instant::now();
        let later = read_at + Duration::from_secs(5);

        assert_eq!(
            player(true, 1000, read_at).position_at(later),
            Duration::from_secs(15)
        );
        assert_eq!(
            player(true, 2000, read_at).position_at(later),
            Duration::from_secs(20)
        );
        assert_eq!(
            player(false, 1000, read_at).position_at(later),
            Duration::from_secs(10)
        );

        let much_later = read_at + Duration::from_secs(600);
        assert_eq!(
            player(true, 1000, read_at).position_at(much_later),
            Duration::from_secs(60)
        );
    }

    #[test]
    fn a_missing_rate_is_normal_speed() {
        assert_eq!(rate_milli(&Value::Number(1.5)), 1500);
        assert_eq!(rate_milli(&Value::default()), 1000);
    }
}
