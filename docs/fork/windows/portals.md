# Desktop Dialogs and Notifications

## Picking files

`FileDialog` shows the desktop's own file dialog through the file chooser portal, so it looks like every other app's:

```rust,ignore
use amane::FileDialog;

FileDialog::open("Open a picture")
    .filter("Images", &["*.png", "*.jpg"])
    .filter("Everything", &["*"])
    .multiple(true)
    .show(|files| Gallery::write().add(files));

FileDialog::save("Save the note")
    .name("note.md")
    .folder("/home/me/Documents")
    .show(|files| Note::read().save_to(&files[0]));
```

| Method | Does |
|---|---|
| `open(title)` / `save(title)` | a dialog that picks files that are there, or where to save one |
| `filter(name, patterns)` | only shows files matching one of the glob patterns; the user picks between filters by name |
| `multiple(true)` | lets the user pick more than one (open only) |
| `directory(true)` | picks folders instead of files (open only) |
| `name(name)` | the file name a save dialog suggests |
| `folder(path)` | where the dialog starts |
| `show(then)` | shows it and moves on; `then` gets the picked paths |

`show` returns right away. The user takes their time, so `then` runs later, on the event loop like any handler, once they've picked. When they cancel, or when no portal is running, `then` isn't called.

The dialog isn't tied to one of your windows yet, so the compositor places it like a new window.

## Sending notifications

`Notify` sends a desktop notification to whatever notification server is running, the shell's or another:

```rust,ignore
use amane::{Notify, Urgency};

Notify::new("Saved")
    .body("notes.md")
    .icon("document-save")
    .urgency(Urgency::Low)
    .send();
```

It's sent under the app's app-id and from another thread, so `send` is safe in input handlers.
