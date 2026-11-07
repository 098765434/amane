# Network: signals and connection profiles (fork addition)

Upstream's `Network` covers Wi-Fi: the link, the access points, joining and leaving. The fork
makes it follow NetworkManager's signals instead of asking every second, and adds the profiles
beyond Wi-Fi.

| API | Meaning |
|---|---|
| `profiles()` | every saved `NetworkProfile`: `uuid`, `name`, `kind` (NetworkManager's type), `wifi_mode`, `active` |
| `vpns()` | the `vpn` and `wireguard` profiles, as GNOME counts them; a tunnel NetworkManager doesn't manage (tailscale) isn't one |
| `hotspot()` | an active Wi-Fi profile in access-point mode |
| `wired()` | the first Ethernet device with a cable in (`Wired`: `device`, `connected`, `name`) |
| `Network::activate(uuid)`, `deactivate(uuid)` | a profile up or down |
| `Network::set_device(interface, connected)` | connect a device with its best profile, as `nmcli device connect`, or disconnect it |

## How it works

- **Signals.** Every signal NetworkManager sends (devices, access points, active connections,
  settings) wakes the listener. A burst is read as one change: it refetches once the signals
  pause for 250 ms, or after a second at most.
- **Profiles' settings** (`GetSettings` per profile) are read again only when a signal came from
  under `/org/freedesktop/NetworkManager/Settings`.
- **A resync every 60 s** in case a signal went missing, only while something watches.
- Nothing is written when nothing changed that a view shows; signal strength counts in five
  steps, as the Wi-Fi glyph shows it.

Code: `src/services/network.rs`, `src/services/network/connections.rs`. Tests: profiles and
the wired device from recorded `GetSettings` answers, the same profiles as amane-shell's `nmcli`
fixtures.
