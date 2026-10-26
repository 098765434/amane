# Power profiles (fork addition)

`PowerProfile` reads and sets the system power profile through
**power-profiles-daemon**: `power-saver`, `balanced` or `performance`.

## Usage

```rust
let profile = PowerProfile::read();

if profile.present() {
    for name in profile.available() {
        let selected = name == profile.active();
        // a button that calls PowerProfile::set(name)
    }

    if let Some(reason) = profile.degraded() {
        // "performance limited: high-operating-temperature"
    }
}
```

| Function | Gives |
| --- | --- |
| `present()` | `false` when power-profiles-daemon isn't installed. Hide the toggle. |
| `active()` | the current profile |
| `available()` | the profiles this machine offers. Some have no `performance`. |
| `degraded()` | why performance is held back (`PerformanceDegraded`), or `None` |
| `PowerProfile::set(name)` | switches the profile (sets `ActiveProfile`) on the worker thread |

## Design / internals

The daemon answers under `net.hadess.PowerProfiles` (`/net/hadess/PowerProfiles`).
Newer versions also answer under `org.freedesktop.UPower.PowerProfiles`
(`/org/freedesktop/UPower/PowerProfiles`). Both are tried once at start-up
with a `GetAll`, and the first that answers is kept. This also
D-Bus-activates the daemon when it is installed but not running.

After that, one thread waits for `PropertiesChanged` on that object path
and re-reads the properties. A change made elsewhere, such as
`powerprofilesctl set balanced`, shows without polling, and nothing is
written when nothing changed.

`HoldProfile` / `ReleaseProfile` (an app keeping a profile while it runs)
are not exposed.
