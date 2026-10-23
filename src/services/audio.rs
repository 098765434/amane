mod device;
mod pulse;

use crate::Service;
use crate::services::worker;

pub use device::{AudioDevice, AudioKind};

use pulse::Device;

#[derive(Default, Clone, PartialEq)]
pub struct Audio {
    // 0 to 100, for the default output
    volume: u8,

    muted: bool,

    // 0 to 100, for the default microphone
    microphone_volume: u8,

    microphone_muted: bool,

    // in the sound server's order, which stays put as devices come and go
    outputs: Vec<AudioDevice>,

    inputs: Vec<AudioDevice>,
}

// event-driven, the sound server announces every volume change
impl Service for Audio {
    fn new() -> Self {
        let mut audio = Self::default();

        audio.update();

        audio
    }

    fn update(&mut self) -> bool {
        let before = self.clone();

        let output = pulse::read(Device::Output);
        let input = pulse::read(Device::Input);

        self.volume = output.volume;
        self.muted = output.muted;

        self.microphone_volume = input.volume;
        self.microphone_muted = input.muted;

        self.outputs = devices(Device::Output);
        self.inputs = devices(Device::Input);

        *self != before
    }

    // the sound server also announces changes to devices nothing shows
    fn listen() {
        pulse::watch(|| {
            let mut audio = Self::write();

            if !audio.update() {
                audio.quiet();
            }
        });
    }
}

impl Audio {
    pub fn volume(&self) -> u8 {
        self.volume
    }

    pub fn muted(&self) -> bool {
        self.muted
    }

    pub fn microphone_volume(&self) -> u8 {
        self.microphone_volume
    }

    pub fn microphone_muted(&self) -> bool {
        self.microphone_muted
    }

    pub fn outputs(&self) -> &[AudioDevice] {
        &self.outputs
    }

    pub fn inputs(&self) -> &[AudioDevice] {
        &self.inputs
    }

    // what the default output is, like headphones, so a bar can show it
    pub fn output_kind(&self) -> AudioKind {
        default_kind(&self.outputs)
    }

    pub fn input_kind(&self) -> AudioKind {
        default_kind(&self.inputs)
    }

    // anything above 100 is treated as 100
    pub fn set_volume(volume: u8) {
        worker::run(move || pulse::set_volume(Device::Output, volume.min(100)));
    }

    pub fn toggle_mute() {
        worker::run(|| toggle(Device::Output));
    }

    // anything above 100 is treated as 100
    pub fn set_microphone_volume(volume: u8) {
        worker::run(move || pulse::set_volume(Device::Input, volume.min(100)));
    }

    pub fn toggle_microphone_mute() {
        worker::run(|| toggle(Device::Input));
    }

    // takes an output's name(), and what is playing moves to it
    pub fn set_output(name: &str) {
        let name = String::from(name);

        worker::run(move || pulse::set_default(Device::Output, &name));
    }

    // takes an input's name()
    pub fn set_input(name: &str) {
        let name = String::from(name);

        worker::run(move || pulse::set_default(Device::Input, &name));
    }
}

fn toggle(device: Device) {
    let level = pulse::read(device);

    pulse::set_muted(device, !level.muted);
}

fn devices(device: Device) -> Vec<AudioDevice> {
    let (endpoints, default) = pulse::devices(device);

    endpoints
        .iter()
        .map(|endpoint| AudioDevice::from_endpoint(endpoint, &default))
        .collect()
}

// built-in when nothing is the default, which is what most machines fall back to
fn default_kind(devices: &[AudioDevice]) -> AudioKind {
    devices
        .iter()
        .find(|device| device.default)
        .map(AudioDevice::kind)
        .unwrap_or_default()
}
