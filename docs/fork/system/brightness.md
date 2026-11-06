# Brightness in HDR (fork addition)

Upstream's `Brightness` writes the first backlight in `/sys/class/backlight`, through logind's
`SetBrightness`. Upstream's guide is [System](../../src/system.md).

The fork adds one case. When the niri fork drives the built-in panel (`eDP`, `LVDS` or `DSI`) in
HDR, the panel takes its light from the PQ signal, so the backlight no longer dims it. This was
found on an amdgpu OLED panel. While that's so, `Brightness` moves niri's SDR white
(`sdr-brightness`, over IPC as `Output { action: SdrBrightness }`) instead of the backlight:

- `percent()` reads SDR white back as a percentage, and `set(percent)` sends it.
- 1 % is 25 nits and 100 % is 500 nits, or the panel's peak if that is lower. The steps between
  are even for the eye: each percent multiplies SDR white by the same factor. 203 nits, niri's
  default, is about 70 %.
- `present()` is also true for such a panel.
- `hdr()` returns the panel (`HdrPanel`: connector, SDR white, peak) while this applies.

niri is asked whether HDR is on every fourth poll, about every two seconds. On any other
compositor, `hdr()` is always none and nothing changes.

niri keeps an IPC change only until it reads its config again. Keeping the level is up to the
app; amane-shell writes it into its `displays.kdl` once it has rested.
