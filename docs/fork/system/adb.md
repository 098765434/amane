# adb (fork addition)

Android phones through **adb**: the phones adb has, over USB or Wireless
debugging; the phones on the network offering Wireless debugging; and pairing
and connecting them. It's what a client like the shell's phone screen (scrcpy's
server) needs before it can start. KDE Connect knows where a phone is
(`PhoneDevice::addresses`), and `Adb` matches it by that address.

## Usage

```rust
let adb = Adb::read();

// the phone KDE Connect sees at this address, if adb can use it now
if let Some(device) = adb.ready_at(address) {
    let serial = device.serial.clone();

    // a blocking call, off the event loop
    let said = amane::adb(Some(&serial), &["shell", "getprop", "ro.build.version.sdk"]);
}

// keep the phone at this address connected while it offers Wireless debugging
Adb::want(address);

// while a pairing QR code shows: look for phones offering pairing, then pair with one
Adb::look(true);

for service in adb.found() {
    if service.kind == WirelessKind::Pairing && service.name == "studio-amane42" {
        Adb::pair(service.address, service.port, "123456");
    }
}
```

| Function | Gives or does |
| --- | --- |
| `installed()` | `adb` runs; without it the service does nothing |
| `devices()` | each `AdbDevice`: `serial`, `state` (`AdbState::Device`, `Unauthorized`, `Offline`, `Other`), `model`, `product`, `address()` over the network, `model_name()` |
| `ready_at(address)`, `ready_usb()` | the devices adb can use now, by network address or over USB |
| `found()` | each `WirelessService` announced on the network: `kind` (`Pairing` or `Connect`), `name` (the instance), `address`, `port` |
| `want(address)`, `unwant(address)` | connects whenever that address offers `Connect` and adb doesn't have it |
| `look(on)` | looks for services without wanting any, like while pairing |
| `pair(address, port, code)`, `connect(address, port)`, `disconnect(serial)` | on their own thread; `busy()` says what's under way, `error()` why the last failed |
| `amane::adb(serial, args)` | runs `adb [-s serial] args…` and waits: what it printed, or why it failed |

## How it works

- **Devices:** `adb track-devices -l` writes the whole list on every change, as
  four hex digits of length and then the list (the same lines `adb devices -l`
  prints). The service reads it on its own thread, and starts it again 5 s
  after the adb server goes away. Nothing is polled.
- **Wireless debugging** announces `_adb-tls-pairing._tcp` (while a pairing
  code or QR code is open on the phone) and `_adb-tls-connect._tcp` (while it
  takes connections) over mDNS. Their ports change each time it's turned on.
  The service browses them with `mdns-sd` while something wants or looks. The
  nixpkgs adb server answers its own `adb mdns` with "unknown host service", so
  adb isn't asked.
- **A QR code** for "Pair device with QR code" is the text
  `WIFI:T:ADB;S:<name>;P:<password>;;`. The phone then announces a pairing
  service whose instance is `<name>`, and `pair` with `<password>` completes it.
- `adb pair` and `adb connect` exit 0 when they fail, so their output is read
  ("Successfully paired", "connected to").

## Limits

- mDNS needs UDP 5353 open in the firewall (`networking.firewall.allowedUDPPorts`).
- Android forgets Wireless debugging on reboot and on another Wi-Fi network. It
  has to be turned on again on the phone, but pairing stays.
- USB needs the phone's "Allow USB debugging?" accepted. Until then it's
  `Unauthorized`.

Checked against android-tools 35.0.2.
