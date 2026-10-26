# Notifications (fork additions)

The fork adds what a shell needs to draw toasts and a notification
center on top of upstream's `Notifications` server: how long the sender
asked a notification to show, the hints that say what kind it is, an
icon as a file, and a way for the shell to let one go. amane still
never closes a notification by itself. The shell decides when a toast
ends and what stays in its history. Upstream's guide is
[Notifications](../../src/notifications.md).

## Usage

```rust
let notifications = Notifications::read();

for notification in notifications.list() {
    let lifetime = match notification.timeout() {
        Timeout::After(duration) => Some(duration),
        Timeout::Default => Some(Duration::from_secs(5)),
        Timeout::Never => None,
    };

    // group by program, draw notification.icon_path() …
}

// once a transient toast is done, from an input handler or a Service
Notifications::expire(id);
```

| Function | Gives |
| --- | --- |
| `timeout()` | `Timeout::Default` (the sender left it to the shell), `Timeout::Never`, or `Timeout::After(duration)` |
| `icon_path()` | the icon as a file: a theme icon found by its name, a path as it was sent, or the sender's program icon from `desktop-entry` |
| `category()` | the kind of event, like `"email.arrived"` or `"device.added"`, or empty |
| `desktop_entry()` | the sender's desktop file id, like `"firefox"`, or empty. It is steadier than `app_name()` for grouping. |
| `transient()` | `true` when it should be left out of a history once shown |
| `resident()` | `true` when it stays open after an action |
| `Notifications::expire(id)` | closes it with the reason *expired*, for a toast the shell is done with |

To hide popups after a few seconds, compare `received()` plus the timeout with the current
time in a Service that polls, and only show the recent ones in your popup window. Keep the full
list for a notification center. When a `transient()` notification's popup ends, call
`Notifications::expire(id)` so it leaves the list too.

## Design / internals

**Timeout.** `Notify` sends `expire_timeout` in milliseconds: `-1` for
the server's default, `0` for never, or a duration. Upstream ignored it.
The fork keeps it as `Timeout` and leaves the policy to the shell. A
shell with a notification center usually hides the toast when the
timeout runs out and keeps the notification in its list.

**Hints.** `category`, `desktop-entry` and `transient` are read next to
`urgency` and `resident`. A missing hint reads as empty or `false`.

**Icon.** `icon_path()` is found once, when the notification arrives, on
the bus thread. The first lookup by name walks the icon themes, which
takes seconds, so a view never waits for it. A `file://` link is
unwrapped. When the sender names no icon, the `desktop-entry` hint is
looked up instead. The `image()` hints, such as an avatar, are separate
and unchanged.

**Expire.** `expire` closes the notification and emits
`NotificationClosed` with reason 1 (*expired*), where `dismiss` sends
2. A sender can tell that its notification timed out, rather than being
dismissed by the user.

**Capabilities.** The server now also lists `persistence`: notifications
stay until they are dismissed. It does not list `body-markup`, so
senders should send plain text. Some still send simple tags like `<b>`,
so a shell should strip them.
