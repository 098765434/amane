# Text Input

`TextInput` is a box you can type in: a search field, a password field, a command prompt, or with `multiline` the body of a note.

## A text field

```rust
use amane::{App, Keyboard, LayerWindow, Rectangle, TextInput};

fn main() {
    App::new().window(view).run();
}

fn view() -> LayerWindow {
    LayerWindow::new()
        .width(300.0)
        .height(40.0)
        .keyboard(Keyboard::OnDemand)
        .child(
            Rectangle::new()
                .width(300.0)
                .height(40.0)
                .fill("#ffffff")
                .padding(8.0)
                .child(
                    TextInput::new("name")
                        .placeholder("your name")
                        .on_submit(|text| println!("hello, {text}")),
                ),
        )
}
```

Click the box, type, and press Enter.

## Focus and stored text

- The window asks for keyboard focus with `.keyboard(Keyboard::OnDemand)`. Without it, the compositor never sends the window any keys, and the input can't be typed in ([Layer Windows](layer_windows.md)).
- Clicking the input gives it focus and puts the cursor where you clicked. Dragging selects. From then on, typed keys go to it.
- The view is rebuilt on every frame, so the input can't keep its own text. Amane keeps it for you, under the name you passed to `TextInput::new`, here `"name"`. Each input needs its own name.

## Options

```rust,ignore
TextInput::new("search")
    .size(18.0)                       // text size, 16 by default
    .color(Color::WHITE)              // text color, black by default
    .width(240.0)                     // Parent by default
    .placeholder("search apps")       // shown while it's empty
    .password()                       // shows dots instead of the text
    .multiline(true)                  // Enter starts a new line
    .selection_color(color)           // behind selected text, the text color faintly by default
    .focused()                        // takes the keys as soon as it's drawn, no click needed
    .on_change(|text| ...)            // runs after every change, with the whole new text
    .on_submit(|text| ...)            // runs when Enter is pressed
```

`focused` is what you want for a launcher: open it, start typing. Pair it with `Keyboard::Exclusive` on the window.

A one-line input scrolls sideways to keep the cursor in sight.

## Many lines

`.multiline(true)` makes Enter start a new line, and Up and Down move between lines. `on_submit` isn't called. The input is as tall as its lines, so put it in a `ScrollArea` for a long text:

```rust,ignore
ScrollArea::new("notes", TextInput::new("notes").multiline(true).on_change(save))
    .width(Parent)
    .height(Parent)
```

Lines only break where the text has a line break; a long line is cut off at the input's edge. `examples/text_editor.rs` is a small editor built this way.

## Reading what was typed

The input owns its text. To use the text anywhere else, copy it into a Service in `on_change`:

```rust
use std::time::Duration;

use amane::{App, Column, Keyboard, LayerWindow, Service, Text, TextInput, children};

struct Search {
    query: String,
}

impl Service for Search {
    fn new() -> Self {
        Self { query: String::new() }
    }

    fn interval() -> Duration {
        Duration::from_secs(3600)
    }
}

fn main() {
    App::new().window(view).run();
}

fn view() -> LayerWindow {
    let search = Search::read();

    LayerWindow::new()
        .width(300.0)
        .height(80.0)
        .keyboard(Keyboard::Exclusive)
        .child(Column::new(children![
            TextInput::new("search")
                .focused()
                .on_change(|text| Search::write().query = text),
            Text::new(format!("searching for: {}", search.query)),
        ]))
}
```

## Setting the text

`TextInput::set_text` replaces what an input holds, by name, and puts the cursor at the end. `TextInput::text` reads it, and `TextInput::has_focus` says whether it takes the keys, to draw a focus ring:

```rust,ignore
TextInput::set_text("search", "");          // clear it, for example after Enter
TextInput::set_text("search", "firefox");   // fill it in
```

Enter doesn't clear the input on its own. Call `set_text` with an empty string in `on_submit` if you want that.

## Keys

While an input has focus:

| Keys | Do |
|---|---|
| letters, `Space` | type, over the selection if there is one |
| `Backspace`, `Delete` | delete the selection, or a letter before or after the cursor |
| `Ctrl+Backspace`, `Ctrl+Delete` | delete a word |
| `Left`, `Right`, `Home`, `End` | move the cursor; with `Shift` they select |
| `Ctrl+Left`, `Ctrl+Right` | move by a word |
| `Ctrl+Home`, `Ctrl+End` | the start and end of a multiline text (`Home` and `End` stay on the line) |
| `Ctrl+A`, `Ctrl+C`, `Ctrl+X`, `Ctrl+V` | select all, copy, cut, paste ([Clipboard](clipboard.md)) |
| `Ctrl+Z`, `Ctrl+Shift+Z` or `Ctrl+Y` | undo and redo; a run of typing is undone a word at a time |
| `Enter` | calls `on_submit`, or starts a new line in a multiline input |
| `Escape` | takes focus away, and then also reaches the window's `on_key` |

- `Up`, `Down`, `PageUp`, `PageDown` and `Tab` skip a one-line input and go straight to the window's `on_key`, so a list under a search box can be moved through with the arrow keys.
- Other chords with `Ctrl`, `Alt` or `Super` held aren't text, so they go on to the window's `on_key`. A window's `on_shortcut` gets every chord before the input does ([Shortcuts](input.md#shortcuts)).
- Held keys repeat.

Text selected with the mouse or with `Shift` becomes the primary selection, and a middle click pastes the primary selection where you click.

## Input methods

Where the compositor supports `text-input-v3`, an input method like fcitx5 or IBus can type into the input, for languages a keyboard can't type directly. While it's composing, its unfinished text is shown underlined at the cursor. Only what it finishes goes into the text and reaches `on_change`. A password input tells the input method it's sensitive.

## Password fields

```rust,ignore
TextInput::new("password")
    .placeholder("password")
    .password()
    .on_submit(|password| Lock::unlock(&password))
```

`password` changes what's drawn, and keeps the text off the clipboard and the primary selection. The text passed to `on_change` and `on_submit` is the real text. See [Lock Screen](lock_screen.md) for a full example.
