# Radio switches: `Rfkill` (fork addition)

The kernel's rfkill switches: each radio (Wi-Fi, Bluetooth, mobile, NFC, …) and whether
software or a hardware switch holds it off.

```rust,ignore
use amane::{Rfkill, Service};

let rfkill = Rfkill::read();
if rfkill.airplane() { /* every radio held off */ }
if rfkill.hard_blocked("wlan") { /* the Wi-Fi switch is off; software can't lift it */ }

// as `rfkill block all`; the change comes back through the service
Rfkill::set_blocked(None, true)?;
Rfkill::set_blocked(Some("bluetooth"), false)?;
```

| API | Meaning |
|---|---|
| `radios()` | each `Radio`: `index` (as `rfkill<index>` in sysfs), `kind` (`wlan`, `bluetooth`, `wwan`, `nfc`, `gps`, `fm`, `uwb`, `wimax`), `soft`, `hard` |
| `present()`, `has(kind)` | whether there's any radio, or one of `kind` |
| `blocked(kind)` | every radio of `kind` held off, by software or a switch; false when there's none |
| `hard_blocked(kind)` | every radio of `kind` held off by a hardware switch |
| `airplane()` | every radio held off |
| `Rfkill::set_blocked(kind, blocked)` | a software block or unblock of every radio of `kind`, or all for `None` |

## How it works

`/dev/rfkill` sends an event for every switch when it's opened and then one per change (8 bytes:
index, type, operation, soft, hard; a read of 8 leaves out the byte newer kernels add). The
service blocks on it, so nothing polls. Writing a change-all event is what `rfkill block` does;
systemd gives the seat's user access to the device. Where it can't be opened, the service reads
`/sys/class/rfkill` every second, only while watched.

Bluetooth's power-on lifts a soft block through `Rfkill::set_blocked(Some("bluetooth"), false)`.

Code: `src/services/rfkill.rs`. Tests: the device events and the sysfs fallback, from recorded
events and a temporary tree.
