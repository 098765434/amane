use super::pulse::Endpoint;

// what a sound goes to or comes from, so a shell can show the right icon
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioKind {
    // built into the machine, like laptop speakers or the laptop microphone
    #[default]
    Internal,

    // a separate speaker, like a usb or bluetooth one
    Speaker,

    Headphones,

    // headphones with a microphone
    Headset,

    // a tv or monitor, over hdmi or displayport
    Display,

    Car,

    Phone,

    Webcam,

    // a separate microphone
    Microphone,
}

// one output or input that can be made the default
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDevice {
    // the sound server's own name, which set_output and set_input take
    pub(crate) name: String,

    pub(crate) description: String,

    pub(crate) kind: AudioKind,

    // the one sounds go to, or come from, right now
    pub(crate) default: bool,
}

impl AudioDevice {
    pub(crate) fn from_endpoint(endpoint: &Endpoint, default: &str) -> Self {
        // a device without a description shows its name, so it still has a label
        let description = match endpoint.description.as_str() {
            "" => &endpoint.name,
            description => description,
        };

        Self {
            name: endpoint.name.clone(),
            description: String::from(description),
            kind: kind(endpoint),
            default: endpoint.name == default,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn kind(&self) -> AudioKind {
        self.kind
    }

    pub fn default(&self) -> bool {
        self.default
    }
}

/*
 * the jack in use says the most, since one sound card switches between its
 * speakers and plugged-in headphones; then what the device says it is, which
 * usb and bluetooth devices do; then its icon, which some only set
 */
fn kind(endpoint: &Endpoint) -> AudioKind {
    let jack = format!("{} {}", endpoint.port, endpoint.name).to_lowercase();

    if jack.contains("headset") {
        return AudioKind::Headset;
    }

    if jack.contains("headphone") {
        return AudioKind::Headphones;
    }

    if jack.contains("hdmi") || jack.contains("displayport") {
        return AudioKind::Display;
    }

    match endpoint.form_factor.as_str() {
        "internal" => return AudioKind::Internal,
        "speaker" | "hifi" | "portable" | "computer" => return AudioKind::Speaker,
        "headphone" => return AudioKind::Headphones,
        "headset" | "hands-free" => return AudioKind::Headset,
        "tv" => return AudioKind::Display,
        "car" => return AudioKind::Car,
        "handset" => return AudioKind::Phone,
        "webcam" => return AudioKind::Webcam,
        "microphone" => return AudioKind::Microphone,
        _ => {}
    }

    // a laptop's own speakers are a jack named speaker, and their icon would say a separate one
    if jack.contains("speaker") {
        return AudioKind::Internal;
    }

    let icon = endpoint.icon_name.as_str();

    if icon.starts_with("audio-headset") {
        return AudioKind::Headset;
    }

    if icon.starts_with("audio-headphones") {
        return AudioKind::Headphones;
    }

    if icon.starts_with("audio-speakers") {
        return AudioKind::Speaker;
    }

    if icon.starts_with("video-display") {
        return AudioKind::Display;
    }

    if icon.starts_with("camera-web") {
        return AudioKind::Webcam;
    }

    AudioKind::Internal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(name: &str, port: &str, form_factor: &str, icon_name: &str) -> Endpoint {
        Endpoint {
            name: String::from(name),
            port: String::from(port),
            form_factor: String::from(form_factor),
            icon_name: String::from(icon_name),
            ..Endpoint::default()
        }
    }

    #[test]
    fn the_jack_in_use_comes_first() {
        let speakers = endpoint(
            "alsa_output.pci-0000_c4_00.6.HiFi__Speaker__sink",
            "[Out] Speaker",
            "",
            "audio-speakers",
        );

        let headphones = endpoint(
            "alsa_output.pci-0000_c4_00.6.analog-stereo",
            "analog-output-headphones",
            "internal",
            "audio-card-analog",
        );

        let hdmi = endpoint(
            "alsa_output.pci-0000_c3_00.1.hdmi-stereo",
            "hdmi-output-0",
            "",
            "",
        );

        assert_eq!(kind(&speakers), AudioKind::Internal);
        assert_eq!(kind(&headphones), AudioKind::Headphones);
        assert_eq!(kind(&hdmi), AudioKind::Display);
    }

    #[test]
    fn usb_and_bluetooth_devices_say_what_they_are() {
        let headset = endpoint("bluez_output.AA_BB_CC_DD_EE_FF.1", "", "headset", "");
        let speaker = endpoint(
            "bluez_output.11_22_33_44_55_66.1",
            "speaker-output",
            "speaker",
            "",
        );
        let car = endpoint("bluez_output.99_88_77_66_55_44.1", "", "car", "");
        let webcam = endpoint(
            "alsa_input.usb-Logitech_C920.analog-stereo",
            "",
            "webcam",
            "",
        );

        assert_eq!(kind(&headset), AudioKind::Headset);
        assert_eq!(kind(&speaker), AudioKind::Speaker);
        assert_eq!(kind(&car), AudioKind::Car);
        assert_eq!(kind(&webcam), AudioKind::Webcam);
    }

    #[test]
    fn the_icon_is_the_last_hint() {
        let headphones = endpoint("bluez_output.AA_BB.1", "", "", "audio-headphones-bluetooth");
        let unknown = endpoint(
            "alsa_input.pci.HiFi__Mic2__source",
            "[In] Mic2",
            "",
            "audio-input-microphone",
        );

        assert_eq!(kind(&headphones), AudioKind::Headphones);
        assert_eq!(kind(&unknown), AudioKind::Internal);
    }

    #[test]
    fn the_default_is_marked_and_labels_fall_back_to_the_name() {
        let device = AudioDevice::from_endpoint(&endpoint("sink", "", "", ""), "sink");

        assert!(device.default());
        assert_eq!(device.description(), "sink");
    }
}
