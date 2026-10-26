# Input (fork additions)

What the fork adds to upstream's [Input](../../src/input.md) page: `on_track`, drag release, files
from other programs, scroll sources, double clicks, more keys, key repeat and release, modifiers and
shortcuts. Everything else on that page is unchanged.

## Usage

### More pointer handlers

All of these are methods on `Rectangle`, next to upstream's `on_click`, `on_hover`, `on_scroll`,
`on_move` and `on_drag`:

| Method | Runs | Gets |
|---|---|---|
| `on_track` | on every movement while the pointer is over it, like `on_move`, without redrawing the window | a `Point` |
| `on_release` | once, when the left button is let go after a drag that started on it, wherever the pointer is then | a `Point` |
| `on_drop` | when files dragged in from another program are let go over it (upstream, missing from its table) | the files, a `Vec<PathBuf>` |
| `on_files_over` | while files from another program are held over it, and once more when they leave or are let go | an `Option<Point>` (`None` once they've gone) and the files; it answers `true` when they can be let go there |

`on_release` comes before `on_click`, so a widget that is both clicked and dragged can tell a drag's end from a click and ignore the click.

### Following the pointer without redrawing

`on_move` redraws its window on every pointer move, which a handler that only records where the
pointer is doesn't need. `on_track` is told about every move the same way, and the move redraws
nothing unless an `on_move` handler is there too (`6b0099b`). A Service the handler writes still
wakes that Service's readers, as any write does.

```rust,ignore
// remember where a right click will anchor its menu, without drawing a frame per move
Rectangle::new()
    .on_track(|point| Menu::write().anchor = point)
    .on_click(|button| if button == Button::Right { open_menu() })
```

### Files from other programs

Files dragged in from a file manager go to the innermost `on_drop` under the pointer. While they're held, `on_files_over` hears where they are and which files they are (Amane reads the list as the drag comes in), and its answer shows in the pointer: `false` and the drag shows it can't be let go there. A window with no `on_files_over` under the files takes them anywhere.

```rust,ignore
Rectangle::new()
    .on_files_over(|at, files| at.is_some() && files.iter().all(|file| file.extension() == Some("md".as_ref())))
    .on_drop(|files| open(files))
```

Text dragged in, and dragging out, are in [Clipboard and Drag and Drop](../windows/clipboard.md).

### Scroll source and finger lift

A `Scroll` is measured in lines, so a mouse wheel and a touchpad move things by the same amount. It also says where it came from, for things that should move differently under fingers than under a wheel:

| Field or method | Gives |
|---|---|
| `source` | a `ScrollSource`: `Wheel` (a wheel that clicks in steps, and anything the compositor doesn't name), `Finger` (a touchpad), `Continuous` (like a trackpoint) or `Tilt` (a wheel pushed sideways) |
| `stop` | `true` when the fingers lift off the touchpad. `x` and `y` are usually `0` then |
| `pixels()` | `(x, y)` in pixels instead of lines, to follow fingers one to one |

A carousel that follows the fingers and settles on a page when they lift:

```rust,ignore
use amane::{Scroll, ScrollSource};

fn scrolled(scroll: Scroll) {
    match scroll.source {
        ScrollSource::Finger | ScrollSource::Continuous => {
            move_by(scroll.pixels().1);

            if scroll.stop {
                settle_on_nearest_page();
            }
        }

        // a wheel steps a page a notch
        ScrollSource::Wheel | ScrollSource::Tilt => step(scroll.y.signum()),
    }
}
```

Not every compositor sends `stop`, so something that waits for it should also give up a moment after the last movement.

### Double clicks and side buttons

`click_count()` says how many quick clicks in a row the one being handled is: `1`, then `2` for a double click and `3` for a triple click. Read it in `on_click` or `on_drag`:

```rust,ignore
Rectangle::new().on_click(|button| {
    if button == Button::Left && click_count() == 2 {
        open_file();
    }
})
```

A click counts as the next one when it comes within 400 ms of the last, with the same button, less than 4 pixels away. A double click also reports a first click before it, so a widget that does something different on each should make the single click harmless.

The side buttons many mice have arrive as `Button::Back` and `Button::Forward`.

### More keys

`Key` gains:

- `PageUp`, `PageDown`
- `Delete`, `Insert`
- `F(n)`: a function key, `F(1)` to `F(24)`

Keypad keys arrive as the key they stand for, so keypad `4` with Num Lock off is `Left`.

A held key repeats at the rate the compositor asks for, so holding `Backspace` keeps deleting. Each repeat calls `on_key` again.

`on_key_release` runs when a key goes up, for things that last while a key is held, like peeking at something while `Alt` is down. Releases skip text inputs.

### When a text input has focus

A focused `TextInput` takes keys before the window does, because typing should type:

- Text keys and the editing keys go to the input only ([Text input](text-input.md#keys)). A chord the input has no use for, like `Ctrl+Q`, goes on to the window.
- `Up`, `Down`, `Tab`, `PageUp`, `PageDown`, `Insert`, the `F` keys and `Other` skip the input and go to the window's `on_key`. That's how arrow keys can move through a list while you type in a search box.
- `Escape` takes focus away from the input, and then also goes to the window, which may want to close.

### Modifier keys

`modifiers()` reports the keys held right now, for telling a command chord apart from typed text:

```rust,ignore
use amane::{Key, modifiers};

fn key_pressed(key: Key) {
    let mods = modifiers();            // { ctrl, shift, alt, logo }

    if mods.ctrl {
        if let Key::Character('a') = key {
            select_all();              // Ctrl+A
        }
        return;                        // don't type the chord
    }

    if let Key::Left = key {
        move_caret(mods.shift);        // Shift+Left extends a selection
    }
}
```

A `Ctrl`+letter chord arrives as `Character(letter)` with `modifiers().ctrl` set (not as `Other`). `Modifiers::command()` is a shortcut for "ctrl, alt, or super is held" — i.e. this key is a command, not text. A focused `TextInput` already ignores command chords instead of typing them. A modifier going down or up redraws the surface with the keyboard, so a view that reads `modifiers()` can show something while one is held (a lone Super press reaches `on_key` only as `Other`).

### Shortcuts

A focused text input takes keys before `on_key`, so `on_key` alone can't make Ctrl+S save while you type. `on_shortcut` runs first, for every key pressed with `Ctrl`, `Alt` or `Super` held. Return `true` for a chord it used, so the input doesn't see it too:

```rust,ignore
Window::new().on_shortcut(|key| {
    if !modifiers().ctrl {
        return false;
    }

    match key {
        Key::Character('s') => save(),
        Key::Character('w') => close_window("main"),
        _ => return false,
    }

    true
})
```

Chords it returns `false` for go on as before: to the text input, then to `on_key`. Windows, layer windows and popups all have it.

## Design / internals

- Scroll source and stop come from `wl_pointer`'s `axis_source` / `axis_stop` (`src/wayland/pointer.rs`).
- Modifiers live in `src/input/modifiers.rs`; a window's key handlers are one `KeyHandlers` struct (`src/input/target.rs`).
- Files dragged in are handled in `src/wayland/dropped.rs`: the file list is read as the drag enters, every move goes to the target's `on_files_over`, whose answer accepts or refuses the drop at that spot, and letting go tells that target the files have gone before handing them to `on_drop`.
- While a drag runs, the dragged target keeps the cursor shape until the drop (`2e9f8e0`, see [Blur](../graphics/blur.md#drag-release-hook)).
