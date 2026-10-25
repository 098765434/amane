# Clipboard and Drag and Drop

Amane shares text with other programs the same ways they share it with each other: the clipboard (Ctrl+C, Ctrl+V), the primary selection (select, then middle click), and dragging.

## Copy and paste

```rust,ignore
use amane::{Clipboard, Key, modifiers};

fn shortcut(key: Key) -> bool {
    if !modifiers().ctrl {
        return false;
    }

    match key {
        Key::Character('c') => Clipboard::copy(Document::read().selected()),
        Key::Character('v') => Clipboard::paste(|text| Document::write().insert(&text)),
        _ => return false,
    }

    true
}
```

- `Clipboard::copy(text)` puts text on the clipboard. It has to come from input, like a key or a click, or the compositor may turn it down. Amane keeps the text and sends it to whatever program pastes it, until something else is copied.
- `Clipboard::paste(then)` reads the clipboard and calls `then` with its text. The program that copied it may take a moment to answer, so `then` runs on another thread once it has. Write what you need into a Service there, and the windows that read it redraw. When there's no text on the clipboard, `then` isn't called.

`PrimarySelection::copy` and `PrimarySelection::paste` work the same way for the primary selection, where the compositor supports it. Copy the selected text to it whenever the selection changes, and paste it on a middle click.

## Text dragged in

`on_text_drop` runs with text dragged in from another program and let go over the rectangle, like a link dragged from a browser:

```rust,ignore
Rectangle::new()
    .on_text_drop(|text| Notes::write().append(&text))
```

Files go to `on_drop` instead ([Input](input.md#files-from-other-programs)).

## Dragging out

`drag_out` lets a rectangle be dragged out of the window, into a file manager, an editor or another app. Once the pointer has moved a few pixels with the left button down, it asks for what to hand over:

```rust,ignore
use amane::DragData;

Rectangle::new()
    .drag_out(|| DragData::Files(vec![Download::read().path.clone()]))
```

- `DragData::Text(text)` is offered as plain text.
- `DragData::Files(paths)` is offered as a list of `file://` links, and as plain paths to programs that only take text.

The compositor takes the pointer over until the button is let go, so the press doesn't end in a click. Dragging out to another Amane window works too.
