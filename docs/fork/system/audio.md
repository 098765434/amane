# Audio devices (fork)

Upstream's `Audio` service only follows the *default* output and input. The fork adds the list of
devices, what kind each one is, and switching the default, so a shell can offer a sound menu and
show a headphones icon while headphones are in use. Everything else in
[the Audio page](../../src/audio.md) is unchanged.

```rust
use amane::{Audio, AudioKind, Service};

let audio = Audio::read();

for output in audio.outputs() {
    println!("{} {:?} default={}", output.description(), output.kind(), output.default());
}

if audio.output_kind() == AudioKind::Headphones {
    // show a headphones glyph
}

Audio::set_output("bluez_output.AA_BB_CC_DD_EE_FF.1");
```

| Function | Gives or does |
|---|---|
| `outputs()` / `inputs()` | every output / input as an `AudioDevice`, in the sound server's order; monitors of outputs are left out |
| `output_kind()` / `input_kind()` | the default device's `AudioKind`, `Internal` when there is none |
| `Audio::set_output(name)` / `Audio::set_input(name)` | makes that device the default; PipeWire moves what is playing along |
| `AudioDevice::name()` | the sound server's name, which the setters take |
| `AudioDevice::description()` | the label to show (falls back to the name) |
| `AudioDevice::kind()` | `Internal`, `Speaker`, `Headphones`, `Headset`, `Display`, `Car`, `Phone`, `Webcam` or `Microphone` |
| `AudioDevice::default()` | `true` for the current default |

## How the kind is found

First match wins (`src/services/audio/device.rs`, unit-tested):

1. **The jack in use** — the active port and node name contain `headset`, `headphone`, `hdmi` or
   `displayport`. One sound card switches between its speakers and plugged-in headphones by
   changing its port (or, with UCM, its node), so this catches wired headphones.
2. **`device.form_factor`**, which USB and Bluetooth devices set: `internal`, `speaker` / `hifi` /
   `portable` / `computer`, `headphone`, `headset` / `hands-free`, `tv`, `car`, `handset`,
   `webcam`, `microphone`.
3. A port or node named `speaker` is the built-in speaker.
4. **`device.icon_name`** for devices that only set an icon: `audio-headset*`, `audio-headphones*`,
   `audio-speakers*`, `video-display*`, `camera-web*`.
5. Otherwise `Internal`.

A device that lands on the wrong kind can be checked with
`pw-dump | grep -E 'form.factor|icon.name|node.name'` (or `pactl list sinks` with pulseaudio).

The device list re-reads on every sink, source and server event, which covers plugging, Bluetooth
connects and default changes. See `examples/audio.rs`.
