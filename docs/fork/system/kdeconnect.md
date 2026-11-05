# KDE Connect (fork addition)

Phones through **KDE Connect**: `kdeconnectd` runs headless on the session bus
(`org.kde.kdeconnect`), and amane reads and drives it, so an app draws the phone
itself and no KDE window ever shows. Two services:

- `KdeConnect`: the devices, their pairing, battery and plugins, and the actions
  (pair, ring, send files, clipboard, the phone's run commands, remote input).
- `PhoneMessages`: one phone's text messages. It's a separate service so a
  program that never shows messages (the shell) doesn't follow them.

Neither one starts the daemon by reading. `KdeConnect::start()` does, for a
"Turn On" button.

## Usage

```rust
let kdeconnect = KdeConnect::read();

if let Some(phone) = kdeconnect.primary() {
    // the first connected phone with a battery, for a bar
    let battery = phone.battery().unwrap();
    let label = format!("{} {}%", phone.name(), battery.percent);
}

for device in kdeconnect.available() {
    // on the network, not paired: a click asks
    KdeConnect::request_pair(device.id());
}

if let Some(asking) = kdeconnect.pairing_request() {
    // the code both screens show
    let code = asking.verification_key();
    KdeConnect::accept_pairing(asking.id()); // or cancel_pairing
}
```

```rust
PhoneMessages::open(device_id);           // the inbox, then the phone's latest of every thread
let messages = PhoneMessages::read();
for thread in messages.threads() {        // newest first
    let unread = thread.unread();
    let who = thread.title();             // contact names where the phone shared them
}
PhoneMessages::load_thread(thread, SMS_PAGE);  // the newest 50; messages(thread) as they come
PhoneMessages::reply(thread, "On my way");     // shows at once as pending()
```

### `KdeConnect`

| Function | Gives or does |
| --- | --- |
| `running()` | the daemon is on the bus |
| `devices()`, `device(id)` | every device, the paired ones first, then by name |
| `paired()`, `available()` | paired devices; reachable unpaired ones |
| `pairing_request()` | a device asking to pair (`PairState::RequestedByPeer`) |
| `primary()` | the first connected device with a battery |
| `failure(id)` | why the last action on a device failed, for 10 s |
| `start()`, `refresh()` | starts the daemon; announces this computer again so phones answer |
| `request_pair`, `accept_pairing`, `cancel_pairing`, `unpair` | pairing |
| `ring(id)` | rings the phone, even when it's on silent |
| `send_files(id, paths)` | each one to the phone's downloads (`share` plugin) |
| `send_clipboard(id)`, `set_clipboard_auto_sync(id, on)` | the clipboard plugin |
| `set_run_commands(id, on)`, `run_command_list()` | the phone's remote for this computer: Lock Screen, Suspend, Lock & Suspend, Screenshot |
| `move_pointer`, `click(PhoneClick)`, `scroll`, `type_text`, `press_key(PhoneKey)` | this computer as the phone's trackpad and keyboard |

A `PhoneDevice` has `id`, `name`, `kind` (`"phone"`, `"tablet"`, `"desktop"`,
`"laptop"`, `"tv"`), `pair_state`, `reachable`, `connected` (paired and reachable),
`verification_key`, `battery` (`PhoneBattery { percent, charging }`),
`clipboard_auto_sync`, `run_commands`, `keyboard_ready` (a text field is focused
on the phone), and `can_ring`, `can_share`, `can_message`, `can_remote_input` and
`can_run_commands` from its loaded plugins.

### `PhoneMessages`

| Function | Gives or does |
| --- | --- |
| `open(id)`, `refresh()` | reads the daemon's threads, then asks the phone for every thread's latest message |
| `threads()`, `thread(id)` | `SmsThread`s, newest first: `title`, `latest`, `unread`, `addresses` |
| `messages(thread)` | that thread's `SmsMessage`s, oldest first, as far as they've come |
| `load_thread(thread, count)`, `loading(thread)` | asks for the newest `count` messages |
| `reply(thread, text)` | sends; a `pending()` message (negative uid) stands in until the phone sends it back as sent |
| `name(address)` | the contact's name, or the number |

## How it works

- **Devices are listed with a raw call** to `org.kde.kdeconnect.daemon.devices(bb)`
  with `(false, false)`. The daemon's introspection lists methods twice, so a
  generated proxy, or `busctl introspect`, refuses it.
- **A device** is `GetAll` on `org.kde.kdeconnect.device` at
  `/modules/kdeconnect/devices/<id>` (`name`, `type`, `isPaired`, `isReachable`,
  `pairState` 0–3, `verificationKey`), plus `loadedPlugins()` while it's connected.
- **Plugins live at sub-paths** (`…/<id>/battery`, `findmyphone`, `share`,
  `clipboard`, `remotecontrol`, `remotekeyboard`). The `conversations` interface
  is on the device object itself.
- **Updates:** every signal from the daemon's owner on the daemon, device,
  battery, clipboard or remote keyboard interfaces reads everything again. That's
  a handful of calls for the one or two phones anyone has, and it only writes the
  service when something changed. A resync runs every 30 s while the daemon runs,
  and again when its owner changes.
- **Two settings have no D-Bus setter.** amane writes them to the plugins'
  QSettings files, as kdeconnect's own pages do:
  - Run commands go in `~/.config/kdeconnect/<id>/kdeconnect_runcommand/config`,
    `[General] commands=` as quoted JSON, under fixed ids so commands added by
    hand stay. Then `reloadPlugins()` runs.
  - Clipboard auto-share is `kdeconnect_clipboard/config`, `[General]
    autoShare`. The plugin only reads it when it starts, so it's disabled and
    enabled again.
- **Messages:** `activeConversations()` answers `av` of
  `(isa(s)xiixixa(xsss))` structs: event, body, addresses, date (ms), type
  (Android's: 1 in, 2 out, 3 draft, 4 outbox, 5 failed, 6 queued), read, thread,
  uid, sim and attachments. `conversationCreated`/`Updated` carry one as `v`;
  `conversationLoaded(x, u)` ends a `requestConversation` window. A message is
  stored once per uid.
- **Contacts** come from the vCards the contacts plugin syncs to
  `~/.local/share/kpeoplevcard/kdeconnect-<id>/*.vcf`. A number matches by its
  digits, or one ending the other with 7 or more digits, so a local number
  matches its `+country` form.

## Limits

- A sleeping phone can't be woken. It answers when it wakes, so give the user a
  refresh.
- A thread's history only exists once the phone has sent it. Show it loading.
- MMS and group pictures only show as an attachment count.
- `reply` returning means the daemon took it, not that it was delivered.
- The other direction needs no code. The phone driving this computer (the
  touchpad and presenter plugins) is done by kdeconnect through the compositor,
  and its notifications and media come through the notification server and
  MPRIS.

Checked against kdeconnect 26.04.3.
