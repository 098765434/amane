# Battery via UPower (fork addition)

`Battery` reads from **UPower** (`org.freedesktop.UPower`, system bus) when
it is running. That adds peripheral batteries (mouse, keyboard, headset,
controller), warning levels, time estimates and AC state to what the
kernel-only reader gave. Without UPower it falls back to the original
sysfs reader (`/sys/class/power_supply`), so the battery still shows.

The source is picked once, when the service starts: if `GetDisplayDevice`
answers, UPower is used for the rest of the session.

```rust
let battery = Battery::read();

if battery.present() {
    let color = if battery.warning() >= WarningLevel::Low { urgent } else { fg };
    let glyph = if battery.on_ac() { "AC" } else { "" };
    // battery.percent(), battery.time_to_empty(), battery.icon() ...
}

for device in battery.peripherals() {
    // "MX Master 3 23%"
    let label = format!("{} {}%", device.model(), device.percent());
}
```

## Getters

| Function | Gives | sysfs fallback |
| --- | --- | --- |
| `present()` | `false` on a machine without a battery | ✅ |
| `percent()` | charge, `0..=100` (`u8`, rounded) | ✅ |
| `charging()` | `true` while charging | ✅ |
| `full()` | plugged in and not charging | ✅ |
| `state()` | `BatteryState`: `Charging`, `Discharging`, `Empty`, `Full`, `PendingCharge`, `PendingDischarge`, `Unknown` | ✅ |
| `on_ac()` | plugged in (UPower's `OnBattery`, inverted) | from status |
| `warning()` | `WarningLevel`: `None`, `Discharging`, `Low`, `Critical`, `Action` | always `None` |
| `time_to_empty()` / `time_to_full()` | `Option<Duration>`, `None` while UPower is still estimating | always `None` |
| `icon()` | a freedesktop icon name like `battery-good-charging-symbolic` | empty |
| `peripherals()` | `&[PowerDevice]`, sorted by model | empty |

`WarningLevel` is ordered, so `warning() >= WarningLevel::Low` covers low,
critical and action. The upstream getters (`present`, `percent`,
`charging`, `full`) keep their shapes and meaning with either source.

`PowerDevice` has `model()`, `kind()` (`DeviceKind`: `Mouse`, `Keyboard`,
`Phone`, `MediaPlayer`, `Tablet`, `GamingInput`, `Headset`, `Other(u32)`),
`percent()` and `state()`. A UPower device counts as a peripheral when
`PowerSupply` is false and it is present. That is UPower's own line between
the machine's batteries and everything else.

## How it works

The overall charge comes from UPower's **display device**, which combines
all of the machine's batteries into the one a bar should show. Nothing is
polled:

- One thread waits for `org.freedesktop.DBus.Properties.PropertiesChanged`
  and keeps signals whose path is under `/org/freedesktop/UPower` and whose
  interface is `org.freedesktop.UPower` or `org.freedesktop.UPower.Device`.
  That also drops power-profiles-daemon's `/org/freedesktop/UPower/PowerProfiles`.
- Two helper threads wait for `DeviceAdded` / `DeviceRemoved`.

Each kept signal re-reads the whole state (one `GetAll` per device) outside
the lock and writes only when it differs, so unrelated system-bus traffic
and repeated identical updates cause no redraws. `Bus::signals` matches on
interface and member only, and the object path is filtered on the client
side, so no new D-Bus API was needed.

Bluetooth devices may show up both here and in `Bluetooth::devices()`
(`battery()`). UPower covers non-Bluetooth peripherals too, such as Logitech
receivers and wired controllers.
