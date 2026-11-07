#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo)]

//! Asking niri directly: its outputs, windows, screencasts, and which of the niri fork's
//! features it has. Each call opens a connection, sends one request and waits up to two
//! seconds for the answer, so call them from a service thread, never from a view. Under
//! another compositor they answer [`NiriError::NotRunning`].
//!
//! The live state (workspaces, windows, the overview, gestures, keyboard layouts) comes from
//! the services instead, which follow niri's event stream.

use std::fmt;
use std::io::{BufRead, BufReader};
use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{Value, json};

use super::send;

/// Why niri didn't answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NiriError {
    /// `NIRI_SOCKET` isn't set, or nothing listens on it.
    NotRunning,
    /// The connection failed, or no answer came in time.
    Io(String),
    /// niri answered with an error, such as stock niri asked for a fork feature.
    Refused(String),
    /// The answer wasn't what amane reads.
    Unexpected(String),
}

impl fmt::Display for NiriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRunning => write!(f, "niri isn't running"),
            Self::Io(err) => write!(f, "no answer from niri: {err}"),
            Self::Refused(err) => write!(f, "niri refused: {err}"),
            Self::Unexpected(what) => write!(f, "unexpected answer from niri: {what}"),
        }
    }
}

impl std::error::Error for NiriError {}

/// Sends one request and reads its `Ok` answer under `name`.
fn ask(request: &Value, name: &str) -> Result<Value, NiriError> {
    let stream = send(&request.to_string()).ok_or(NiriError::NotRunning)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|err| NiriError::Io(err.to_string()))?;

    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|err| NiriError::Io(err.to_string()))?;
    let mut reply: Value =
        serde_json::from_str(&line).map_err(|err| NiriError::Unexpected(err.to_string()))?;

    if let Some(err) = reply.get("Err") {
        return Err(NiriError::Refused(
            err.as_str().unwrap_or("an error").to_owned(),
        ));
    }
    reply
        .get_mut("Ok")
        .and_then(|ok| ok.get_mut(name))
        .map(Value::take)
        .ok_or_else(|| NiriError::Unexpected(format!("no {name}")))
}

/// The niri fork's features this niri has, with each one's contract version. Empty on stock
/// niri, which can't parse the request.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Features {
    list: Vec<(String, u32)>,
}

impl Features {
    /// The version of a feature, like `features().version("blur-shapes") == Some(2)`.
    pub fn version(&self, name: &str) -> Option<u32> {
        self.list
            .iter()
            .find(|(feature, _)| feature == name)
            .map(|(_, version)| *version)
    }

    /// Whether the feature is there, at any version.
    pub fn has(&self, name: &str) -> bool {
        self.version(name).is_some()
    }

    /// Whether this is the niri fork at all.
    pub fn is_fork(&self) -> bool {
        !self.list.is_empty()
    }

    /// Every feature and its version.
    pub fn iter(&self) -> impl Iterator<Item = (&str, u32)> {
        self.list
            .iter()
            .map(|(name, version)| (name.as_str(), *version))
    }

    fn parse(list: &Value) -> Self {
        let list = list
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|feature| {
                        let name = feature.get("name")?.as_str()?.to_owned();
                        let version = u32::try_from(feature.get("version")?.as_u64()?).ok()?;
                        Some((name, version))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { list }
    }
}

/// What niri answered to `Features`, asked once for the program's life. Empty when niri isn't
/// running, or is stock niri.
pub fn features() -> &'static Features {
    static FEATURES: OnceLock<Features> = OnceLock::new();
    FEATURES.get_or_init(|| {
        ask(&json!("Features"), "Features")
            .map(|list| Features::parse(&list))
            .unwrap_or_default()
    })
}

/// A connected output.
#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    /// The connector, like `eDP-1`.
    pub name: String,
    pub make: String,
    pub model: String,
    /// Where it is in the layout, when it's turned on.
    pub logical: Option<Logical>,
    /// Whether it could show HDR (the fork; false on stock niri).
    pub hdr_supported: bool,
    /// Whether it shows HDR now (the fork).
    pub hdr_enabled: bool,
    /// SDR white in nits while it shows HDR (the fork).
    pub sdr_brightness: Option<f64>,
    /// The panel's peak brightness in nits, from its EDID (the fork).
    pub peak_luminance: Option<f64>,
}

/// An output's place in the layout, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Logical {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

/// The connected outputs, by name.
pub fn outputs() -> Result<Vec<Output>, NiriError> {
    let outputs = ask(&json!("Outputs"), "Outputs")?;
    let outputs = outputs
        .as_object()
        .ok_or_else(|| NiriError::Unexpected(String::from("outputs aren't a map")))?;

    let mut list: Vec<Output> = outputs.values().filter_map(parse_output).collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(list)
}

fn parse_output(output: &Value) -> Option<Output> {
    let text = |key: &str| output.get(key)?.as_str().map(str::to_owned);
    let logical = output
        .get("logical")
        .filter(|l| !l.is_null())
        .and_then(|l| {
            Some(Logical {
                x: i32::try_from(l.get("x")?.as_i64()?).ok()?,
                y: i32::try_from(l.get("y")?.as_i64()?).ok()?,
                width: u32::try_from(l.get("width")?.as_u64()?).ok()?,
                height: u32::try_from(l.get("height")?.as_u64()?).ok()?,
                scale: l.get("scale")?.as_f64()?,
            })
        });
    Some(Output {
        name: text("name")?,
        make: text("make").unwrap_or_default(),
        model: text("model").unwrap_or_default(),
        logical,
        hdr_supported: output.get("hdr_supported").and_then(Value::as_bool) == Some(true),
        hdr_enabled: output.get("hdr_enabled").and_then(Value::as_bool) == Some(true),
        sdr_brightness: output.get("sdr_brightness").and_then(Value::as_f64),
        peak_luminance: output.get("peak_luminance").and_then(Value::as_f64),
    })
}

/// An open window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    pub id: u64,
    pub title: Option<String>,
    pub app_id: Option<String>,
    /// The process that made it, when niri knows.
    pub pid: Option<i32>,
    pub workspace_id: Option<u64>,
    pub is_focused: bool,
    pub is_floating: bool,
    /// Whether it's fullscreen (the fork; false on stock niri).
    pub is_fullscreen: bool,
}

/// Every open window.
pub fn windows() -> Result<Vec<Window>, NiriError> {
    let windows = ask(&json!("Windows"), "Windows")?;
    Ok(windows
        .as_array()
        .map(|list| list.iter().filter_map(parse_window).collect())
        .unwrap_or_default())
}

/// The window with the keyboard focus, if any.
pub fn focused_window() -> Result<Option<Window>, NiriError> {
    let window = ask(&json!("FocusedWindow"), "FocusedWindow")?;
    Ok(parse_window(&window))
}

fn parse_window(window: &Value) -> Option<Window> {
    let text = |key: &str| window.get(key)?.as_str().map(str::to_owned);
    let flag = |key: &str| window.get(key).and_then(Value::as_bool) == Some(true);
    Some(Window {
        id: window.get("id")?.as_u64()?,
        title: text("title"),
        app_id: text("app_id"),
        pid: window
            .get("pid")
            .and_then(Value::as_i64)
            .and_then(|pid| i32::try_from(pid).ok()),
        workspace_id: window.get("workspace_id").and_then(Value::as_u64),
        is_focused: flag("is_focused"),
        is_floating: flag("is_floating"),
        is_fullscreen: flag("is_fullscreen"),
    })
}

/// A screencast niri is running.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cast {
    pub session_id: u64,
    pub stream_id: u64,
    /// `PipeWire` (a portal screencast) or `WlrScreencopy`.
    pub kind: String,
    pub target: CastTarget,
    pub is_active: bool,
    /// The program casting, when niri knows.
    pub pid: Option<i32>,
}

/// What a screencast shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CastTarget {
    Nothing,
    Output(String),
    Window(u64),
}

/// The screencasts running now.
pub fn casts() -> Result<Vec<Cast>, NiriError> {
    let casts = ask(&json!("Casts"), "Casts")?;
    Ok(casts
        .as_array()
        .map(|list| list.iter().filter_map(parse_cast).collect())
        .unwrap_or_default())
}

fn parse_cast(cast: &Value) -> Option<Cast> {
    let target = cast.get("target")?;
    let target = if let Some(output) = target.get("Output") {
        CastTarget::Output(output.get("name")?.as_str()?.to_owned())
    } else if let Some(window) = target.get("Window") {
        CastTarget::Window(window.get("id")?.as_u64()?)
    } else {
        CastTarget::Nothing
    };
    Some(Cast {
        session_id: cast.get("session_id")?.as_u64()?,
        stream_id: cast.get("stream_id")?.as_u64()?,
        kind: cast.get("kind")?.as_str()?.to_owned(),
        target,
        is_active: cast.get("is_active").and_then(Value::as_bool) == Some(true),
        pid: cast
            .get("pid")
            .and_then(Value::as_i64)
            .and_then(|pid| i32::try_from(pid).ok()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_features_list() {
        let list = serde_json::from_str(
            r#"[{"name":"blur-shapes","version":2},{"name":"desktop-obscured","version":1}]"#,
        )
        .expect("json");
        let features = Features::parse(&list);
        assert_eq!(features.version("blur-shapes"), Some(2));
        assert!(features.has("desktop-obscured"));
        assert!(!features.has("hdr"));
        assert!(features.is_fork());
        assert!(!Features::default().is_fork());
    }

    #[test]
    fn reads_outputs_windows_and_casts() {
        let output: Value = serde_json::from_str(
            r#"{"name":"eDP-1","make":"Samsung","model":"ATNA40","logical":{"x":0,"y":0,"width":1600,"height":1000,"scale":1.75,"transform":"Normal"},"hdr_supported":true,"hdr_enabled":false,"sdr_brightness":null,"peak_luminance":600.0}"#,
        )
        .expect("json");
        let output = parse_output(&output).expect("an output");
        assert_eq!(output.name, "eDP-1");
        assert_eq!(output.logical.map(|l| l.width), Some(1600));
        assert!(output.hdr_supported && !output.hdr_enabled);
        assert_eq!(output.peak_luminance, Some(600.0));

        // stock niri: no fork fields, and an output that's off has no logical place
        let stock: Value =
            serde_json::from_str(r#"{"name":"HDMI-A-1","make":"","model":"","logical":null}"#)
                .expect("json");
        let stock = parse_output(&stock).expect("an output");
        assert!(stock.logical.is_none() && !stock.hdr_supported);

        let window: Value = serde_json::from_str(
            r#"{"id":7,"title":"foot","app_id":"foot","pid":4242,"workspace_id":3,"is_focused":true,"is_floating":false,"is_urgent":false}"#,
        )
        .expect("json");
        let window = parse_window(&window).expect("a window");
        assert_eq!((window.id, window.pid), (7, Some(4242)));
        assert!(window.is_focused && !window.is_fullscreen);

        let cast: Value = serde_json::from_str(
            r#"{"stream_id":2,"session_id":1,"kind":"PipeWire","target":{"Window":{"id":7}},"is_dynamic_target":false,"is_active":true,"pid":99,"pw_node_id":55}"#,
        )
        .expect("json");
        let cast = parse_cast(&cast).expect("a cast");
        assert_eq!(cast.target, CastTarget::Window(7));
        assert_eq!(cast.pid, Some(99));
    }
}
