# Appearance (fork addition)

`Appearance` reads the desktop's **light/dark preference** and **accent
colour** from the XDG Settings portal (`xdg-desktop-portal`). Most programs
already follow this setting, so a bar that reads it matches them.

```rust
let appearance = Appearance::read();

let (background, foreground) = if appearance.prefers_dark() {
    (Color::BLACK, Color::WHITE)
} else {
    (Color::WHITE, Color::BLACK)
};

let accent = appearance.accent().unwrap_or(Palette::read().accent());
```

| Function | Gives |
| --- | --- |
| `present()` | `false` without xdg-desktop-portal. Everything else then reads as no preference. |
| `scheme()` | a `Scheme`: `Dark`, `Light` or `NoPreference` |
| `prefers_dark()` | `true` when the scheme is `Dark` |
| `accent()` | the accent `Color`, or `None` when the portal backend doesn't know one |

## How it works

At start-up, `Read("org.freedesktop.appearance", …)` is called once each for
`color-scheme` and `accent-color`, on
`org.freedesktop.portal.Desktop` / `/org/freedesktop/portal/desktop`.

`color-scheme` is `1` for dark and `2` for light. Anything else means no
preference. `accent-color` is three sRGB parts from 0 to 1. A part outside
that range means unset, which some backends send, and becomes `None`.

After that, one thread waits for `SettingChanged` in the
`org.freedesktop.appearance` namespace. A change made elsewhere shows
without polling, and nothing is written when nothing changed. Without a
portal the thread never starts.

Which backend answers depends on the portal config. With
xdg-desktop-portal-gnome or -gtk, the scheme follows
`org.gnome.desktop.interface color-scheme` in dconf. `accent-color` is only
filled by backends that know one, such as GNOME 47+ and KDE.

Read-only. The portal has no way to *set* the scheme, so changing the
system theme stays desktop-specific. `Palette` has no hook to start from
the accent, so the shell picks between `Appearance::accent()` and its
`Palette` colours itself.
