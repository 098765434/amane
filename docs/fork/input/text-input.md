# Text input (fork additions)

What the fork adds to upstream's [Text Input](../../src/text_input.md) page: selection, mouse
editing, the editing keys, clipboard, undo, many lines, input methods, and reading focus.
Upstream's `TextInput` was one line, with no selection and no key repeat.

## Usage

### Options

Two more builder methods, next to upstream's `width`, `placeholder`, `password`, `focused`,
`on_change` and `on_submit`:

```rust,ignore
TextInput::new("notes")
    .multiline(true)                  // Enter starts a new line
    .selection_color(color)           // behind selected text, the text color faintly by default
```

Clicking the input gives it focus and puts the cursor where you clicked. Dragging selects. A
one-line input scrolls sideways to keep the cursor in sight.

### Many lines

`.multiline(true)` makes Enter start a new line, and Up and Down move between lines. With `on_submit` set as well, Enter sends instead, like a chat's field, and Shift+Enter starts a new line. The input is as tall as its lines, so put it in a `ScrollArea` for a long text:

```rust,ignore
ScrollArea::new("notes", TextInput::new("notes").multiline(true).on_change(save))
    .width(Parent)
    .height(Parent)
```

Lines only break where the text has a line break; a long line is cut off at the input's edge. `examples/text_editor.rs` is a small editor built this way.

### Reading the text and focus

Next to upstream's `TextInput::set_text`, `TextInput::text` reads what an input holds, by name, and `TextInput::has_focus` says whether it takes the keys, to draw a focus ring. `TextInput::take_secret` takes a password input's text out and leaves the input empty. The text comes in a `Zeroizing<String>` that wipes itself when dropped: read a password this way to hand it on, as an unlock prompt does.

### Keys

While an input has focus:

| Keys | Do |
|---|---|
| letters, `Space` | type, over the selection if there is one |
| `Backspace`, `Delete` | delete the selection, or a letter before or after the cursor |
| `Ctrl+Backspace`, `Ctrl+Delete` | delete a word |
| `Left`, `Right`, `Home`, `End` | move the cursor; with `Shift` they select |
| `Ctrl+Left`, `Ctrl+Right` | move by a word |
| `Ctrl+Home`, `Ctrl+End` | the start and end of a multiline text (`Home` and `End` stay on the line) |
| `Ctrl+A`, `Ctrl+C`, `Ctrl+X`, `Ctrl+V` | select all, copy, cut, paste ([Clipboard](../windows/clipboard.md)) |
| `Ctrl+Z`, `Ctrl+Shift+Z` or `Ctrl+Y` | undo and redo; a run of typing is undone a word at a time |
| `Enter` | calls `on_submit`, or starts a new line in a multiline input |
| `Escape` | takes focus away, and then also reaches the window's `on_key` |

- `Up`, `Down`, `PageUp`, `PageDown` and `Tab` skip a one-line input and go straight to the window's `on_key`, so a list under a search box can be moved through with the arrow keys.
- Other chords with `Ctrl`, `Alt` or `Super` held aren't text, so they go on to the window's `on_key`. A window's `on_shortcut` gets every chord before the input does ([Shortcuts](input.md#shortcuts)).
- Held keys repeat.
- A double click selects a word, a triple click a line.

Text selected with the mouse or with `Shift` becomes the primary selection, and a middle click pastes the primary selection where you click.

### Input methods

Where the compositor supports `text-input-v3`, an input method like fcitx5 or IBus can type into the input, for languages a keyboard can't type directly. While it's composing, its unfinished text is shown underlined at the cursor. Only what it finishes goes into the text and reaches `on_change`.

A password input tells the input method it's a password (`ContentPurpose::Password`, `SensitiveData | HiddenText`), and sends it no surrounding text: the input method never learns what a password holds. It takes only the input method's finished text. Preedit isn't shown, and requests to delete around the cursor are ignored.

### Passwords

`password` changes what's drawn, and keeps the text off the clipboard and the primary selection. Copy and cut wipe what they took instead of copying it. A password input keeps no undo history. Its text grows into a new buffer only after the old one is wiped, and is wiped again when the input is cleared or dropped, so old copies don't stay in freed memory. The text passed to `on_change` and `on_submit` is the real text, a copy that's wiped once the handler returns. A handler that keeps it should keep it in a `Zeroizing<String>`.

## Design / internals

- The editing model is `src/widgets/text_input/field.rs`, with unit tests. Undo keeps 200 steps per input; typing is grouped a word at a time.
- The focused input's key handler returns whether it used a key, so chords it doesn't know (Ctrl+Q) reach the window.
- The IME is `zwp_text_input_v3` (`src/wayland/text_input.rs`): enabled while an input has focus and the window has the keyboard; surrounding text, cursor rectangle and content type (password, multiline) are sent on change.
- History: phase 5 of [App windows](../windows/app-windows.md#phase-5-text-input-done).
