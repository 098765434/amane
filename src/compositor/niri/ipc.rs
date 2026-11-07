//! Asking niri one thing and reading its answer: the HDR panel and its SDR white (the niri fork).

use std::io::{BufRead, BufReader};
use std::time::Duration;

use serde_json::{Value, json};

use super::super::HdrPanel;
use super::*;

// sends one request and reads niri's one-line answer, or none in two seconds
pub(super) fn ask(request: &Value) -> Option<Value> {
    let stream = send(&request.to_string())?;

    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;

    let mut reply = String::new();

    BufReader::new(stream).read_line(&mut reply).ok()?;

    serde_json::from_str(&reply).ok()
}

/*
 * the built-in panel while niri drives it in HDR: in PQ the panel takes
 * its light from the signal, so the backlight no longer dims it; SDR white
 * is niri's `sdr-brightness`, 203 nits when the config leaves it out
 */
pub(crate) fn hdr_panel() -> Option<HdrPanel> {
    let reply = ask(&json!("Outputs"))?;

    let outputs = reply.get("Ok")?.get("Outputs")?.as_object()?;

    outputs.values().find_map(|output| {
        let name = output.get("name")?.as_str()?;

        let built_in = ["eDP", "LVDS", "DSI"]
            .iter()
            .any(|kind| name.starts_with(kind));

        let lit = output
            .get("current_mode")
            .is_some_and(|mode| !mode.is_null());

        let hdr = output.get("hdr_enabled").and_then(Value::as_bool) == Some(true);

        (built_in && lit && hdr).then(|| HdrPanel {
            output: name.to_string(),
            sdr_white: output
                .get("sdr_brightness")
                .and_then(Value::as_f64)
                .unwrap_or(203.0),
            peak: output.get("peak_luminance").and_then(Value::as_f64),
        })
    })
}

// SDR white on `output`, in nits, until niri reads its config again
pub(crate) fn set_sdr_white(output: &str, nits: f64) -> bool {
    // stock niri has no HDR to set
    if !client::features().has("hdr") {
        return false;
    }

    client::set_output(output, client::OutputChange::SdrBrightness(nits)).is_ok()
}
