# Status items and Live Activities (fork addition)

A program's own **item in the status bar**, like macOS's `NSStatusItem`, and
**Live Activities**, as macOS 27.3 puts in the menu bar: something going on now,
like a timer, a song or an upload. amane is the client side and holds the
protocol types. The bar serves `org.amane.StatusBar1` on the session bus and
draws both. amane-shell is that bar. Unlike the tray (StatusNotifierItem), the
bar draws everything here itself, and drops reach the program.

## Usage

```rust
use amane::status_item::{
    Accepts, ActivityContent, Dismiss, ItemEvent, ItemIcon, ItemSpec, MenuEntry, Trailing,
};
use amane::{LiveActivity, StatusItem};

// the id is unique on the desktop, like an app id
let mut spec = ItemSpec::new("org.amane.drop", ItemIcon::Glyph("tray".into()));
spec.accepts = Accepts { files: true, text: false };
spec.opens_on_drag = true;
spec.menu = vec![MenuEntry::new("quit", "Quit")];

let item = StatusItem::register(spec);

item.on_event(|event| match event {
    // x is the item's middle on that monitor, edge where the bar is
    ItemEvent::Activated { button, x, output, edge } => {}
    ItemEvent::MenuSelected(id) => {}
    ItemEvent::DroppedFiles(files) => {}
    ItemEvent::DroppedText(text) => {}
    ItemEvent::DragHover { over, x, output, edge } => {}
    // the user took it out of the bar (or put it back); keep running
    ItemEvent::Shown(shown) => {}
});

item.update(new_spec); // the whole spec again
item.remove();

let mut content = ActivityContent::new("Drop", ItemIcon::Glyph("upload".into()), "Uploading");
content.trailing = Trailing::Progress(0.3); // or Text, Countdown(ms), CountUp(ms)

let activity = LiveActivity::start(content.clone());
activity.on_event(|event| { /* Action(id) from a button, or Dismissed */ });
activity.update(content, false); // true alerts: the bar briefly expands it
activity.end(Dismiss::After(3));
```

`examples/status_item.rs` puts up an item and a countdown and prints what
happens. Run it beside a bar.

What an activity shows:

| Field | Compact | Minimal | Expanded |
|---|---|---|---|
| `leading` (an `ItemIcon`) | ✓ | ✓ | ✓ |
| `trailing`: text, a countdown or count-up to a time, a progress ring | ✓ | | ✓ |
| `title`, `subtitle` | | | ✓ |
| `progress` (a bar), `buttons` (up to 3) | | | ✓ |
| `tint` (a palette name, so it follows the theme) | ✓ | ✓ | ✓ |

- **Ranking.** `relevance` (0 to 1) ranks several activities. The bar shows the
  most relevant compact and the rest minimal.
- **Countdowns.** A `Countdown` or `CountUp` is counted by the bar, so it
  needs no update each second.
- **Staleness.** `stale_after` (seconds) is how long without an update before
  the bar shows it as out of date.
- **Ending.** The bar ends any activity after 8 hours.

## Calling it from a script

Every payload is one JSON string, so `busctl` works:

```sh
busctl --user call org.amane.StatusBar1 /org/amane/StatusBar org.amane.StatusBar1 \
    StartActivity s '{"title":"Tea","leading":{"glyph":"timer"},"trailing":{"countdown":1767225600000}}'
# → u 7
busctl --user call org.amane.StatusBar1 /org/amane/StatusBar org.amane.StatusBar1 \
    EndActivity us 7 '"now"'
```

An activity or item ends when the program that made it leaves the bus, so a
script's `busctl` call alone only flashes one by. A script keeps a connection
open, for example with `gdbus` in a loop or a small Python `dbus-next` client.

## The protocol

At `/org/amane/StatusBar`, interface `org.amane.StatusBar1`:

| Method | Arguments | Answers |
|---|---|---|
| `RegisterItem` | `s` an `ItemSpec` | an error when another program has the id |
| `UpdateItem` | `s` an `ItemSpec` | |
| `RemoveItem` | `s` the id | |
| `StartActivity` | `s` an `ActivityContent` | `u` its handle |
| `UpdateActivity` | `u` handle, `s` an `ActivityContent`, `b` alert | |
| `EndActivity` | `u` handle, `s` a `Dismiss` (`"now"` or `{"after":5}`) | |

| Signal | Arguments |
|---|---|
| `ItemEvent` | `s` the item's id, `s` an `ItemEvent` |
| `ActivityEvent` | `u` the handle, `s` an `ActivityEvent` (`{"action":"stop"}` or `"dismissed"`) |

- **Versions.** Payloads carry `v` (now `1`; left out means `1`). The bar
  refuses a payload newer than it knows.
- **Signals.** These go only to the program that made the item or activity.
- **Ownership.** Only the program that made an item or activity changes it.
  The bar drops all of a program's items and activities when it leaves the bus.
- **Leaving out fields.** Every field but an item's `id` may be left out.

## How the client works

- **Order of calls.** Calls go out on amane's worker thread in the order they
  were made. A `LiveActivity` exists at once, and its updates wait in line
  behind the `StartActivity` that numbers it.
- **Events.** Events come in on a thread of their own. A handler that should
  change a window writes a `Service` and lets that redraw.
- **Bar restarts.** The client keeps every item and live activity. When
  `org.amane.StatusBar1` gets a new owner (the bar restarted), it registers
  them all again and maps the new handles back.

## Related

- amane-shell's [writing a status app](https://github.com/098765434/amane-shell/blob/main/docs/integration/writing-a-status-app.md):
  a window under the item, drags through it, what to put in an activity, running as a
  service. amane-drop is the worked example.
- amane-shell's [status apps internals](https://github.com/098765434/amane-shell/blob/main/docs/development/status-apps.md):
  how the bar serves this, and testing beside a running desktop.
- [tray](tray.md): the StatusNotifierItem tray, for programs that already speak it.
- [notifications](notifications.md): `Notify`, for what is over rather than going on.
