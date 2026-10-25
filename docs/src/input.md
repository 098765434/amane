# Input

A bar needs buttons: click a workspace to switch to it, scroll over the volume to change it, click the clock to open a calendar.

This page covers mouse and keyboard input: clicks, hover, scrolling, dragging, the pointer's look, and keys.

## Handling a click

```rust
use amane::{App, Button, LayerWindow, Parent, Rectangle, Text};

fn main() {
    App::new().window(view).run();
}

fn view() -> LayerWindow {
    LayerWindow::new().width(200.0).height(40.0).child(
        Rectangle::new()
            .width(Parent)
            .height(Parent)
            .fill("#1e1e2e")
            .on_click(clicked)
            .child(Text::new("click me")),
    )
}

fn clicked(button: Button) {
    println!("clicked with {button:?}");
}
```

## How input reaches a handler

Input handlers are set on a `Rectangle`. Each frame, Amane remembers where every rectangle with a handler ended up. When you click, it finds the rectangle under the pointer and calls its handler.

If rectangles overlap, the one drawn last gets the event. That's the innermost and topmost one: a button inside a panel gets the click, not the panel.

The handler runs after the frame is gone, so it can't change widgets directly. It changes state instead, usually by writing to a Service, and the view shows the change on the next frame:

```rust,ignore
fn clicked(_: Button) {
    Counter::write().count += 1;
}
```

## Mouse handlers

All of these are methods on `Rectangle`:

| Method | Called | Gets |
|---|---|---|
| `on_click` | when a button is pressed and released over the same rectangle | which `Button`: `Left`, `Right`, or `Middle` |
| `on_hover` | when the pointer comes in, and when it goes out | `true` when it comes in, `false` when it goes out |
| `on_scroll` | for each wheel step or touchpad movement | a `Scroll` with `x` and `y` |
| `on_move` | on every movement while the pointer is over it | a `Point` |
| `on_drag` | on a left press, and on every movement until release | a `Point` |
| `on_release` | once, when the left button is let go after a drag that started on it, wherever the pointer is then | a `Point` |
| `on_drop` | when files dragged in from another program are let go over it | the files, a `Vec<PathBuf>` |
| `on_files_over` | while files from another program are held over it, and once more when they leave or are let go | an `Option<Point>` (`None` once they've gone) and the files; it answers `true` when they can be let go there |

A `Point` has `x` and `y`, measured in pixels from the rectangle's top-left corner. During `on_drag`, the point keeps coming even after the pointer leaves the rectangle, so it can be negative or bigger than the rectangle. Clamp it.

`on_release` comes before `on_click`, so a widget that is both clicked and dragged can tell a drag's end from a click and ignore the click.

## Files from other programs

Files dragged in from a file manager go to the innermost `on_drop` under the pointer. While they're held, `on_files_over` hears where they are and which files they are (Amane reads the list as the drag comes in), and its answer shows in the pointer: `false` and the drag shows it can't be let go there. A window with no `on_files_over` under the files takes them anywhere.

```rust,ignore
Rectangle::new()
    .on_files_over(|at, files| at.is_some() && files.iter().all(|file| file.extension() == Some("md".as_ref())))
    .on_drop(|files| open(files))
```

A `Scroll` is measured in lines, so a mouse wheel and a touchpad move things by the same amount. Positive `y` means scrolling down, and positive `x` means scrolling right.

A `Scroll` also says where it came from, for things that should move differently under fingers than under a wheel:

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

## Passing values into handlers

A handler can be a function, like `clicked` above, or a closure. A closure can carry values from the view into the handler:

```rust
use amane::{App, Column, LayerWindow, Parent, Rectangle, Text, Widget};

fn main() {
    App::new().window(view).run();
}

fn view() -> LayerWindow {
    let mut rows: Vec<Box<dyn Widget>> = Vec::new();

    for number in 1..=3 {
        let row = Rectangle::new()
            .width(Parent)
            .height(30.0)
            .on_click(move |_| println!("clicked row {number}"))
            .child(Text::new(format!("row {number}")));

        rows.push(Box::new(row));
    }

    LayerWindow::new().width(200.0).height(90.0).child(Column::new(rows))
}
```

`move` copies `number` into the closure, so each row remembers its own number. `|_|` ignores the `Button` argument.

A closure can't borrow from a Service's `read()`, because the handler lives longer than the view. Copy out what you need first:

```rust,ignore
let id = workspace.id();                                   // a copy, not a borrow
let name = String::from(device.name());                    // an owned copy of the text

Rectangle::new()
    .width(30.0)
    .height(Parent)
    .on_click(move |_| Workspaces::focus(id))
```

## Double clicks and side buttons

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

## Hover effects

A hover effect needs state, because the view has to know whether the pointer is inside:

```rust
use std::time::Duration;

use amane::{App, Color, LayerWindow, Parent, Rectangle, Service};

struct Hover {
    inside: bool,
}

impl Service for Hover {
    fn new() -> Self {
        Self { inside: false }
    }

    fn interval() -> Duration {
        Duration::from_secs(3600)
    }
}

fn main() {
    App::new().window(view).run();
}

fn view() -> LayerWindow {
    let hover = Hover::read();

    let fill = if hover.inside { Color::from("#45475a") } else { Color::from("#1e1e2e") };

    LayerWindow::new().width(200.0).height(40.0).child(
        Rectangle::new()
            .width(Parent)
            .height(Parent)
            .fill(fill)
            .on_hover(|inside| Hover::write().inside = inside),
    )
}
```

## Changing the pointer

```rust,ignore
Rectangle::new()
    .width(100.0)
    .height(30.0)
    .cursor(Pointer)
    .on_click(clicked)
```

While the pointer is over the rectangle, it takes that look. The names are values you import from `amane`: `Default`, `Pointer` (a hand), `Text`, `Grab`, `Grabbing`, `Move`, `NotAllowed`, `Wait`, `Crosshair`, and the resize arrows `ResizeTop`, `ResizeBottom`, `ResizeLeft`, `ResizeRight`, `ResizeTopLeft`, `ResizeTopRight`, `ResizeBottomLeft`, `ResizeBottomRight`, `ResizeHorizontal`, and `ResizeVertical`.

`Text` the cursor and `Text` the widget share a name, and both come from `amane`. Rust tells them apart by how they're used. If you'd rather be explicit, write `Cursor::Text`.

A `TextInput` shows the text cursor on its own.

## Transformed rectangles

Input follows a rectangle's transform. A rotated button only reacts inside its rotated shape, not inside the box it took up before rotating ([Rectangle](rectangle.md)).

## Keyboard input

Keys go to a window, not a widget. First the window has to ask for keyboard focus ([Layer Windows](layer_windows.md)), then it handles keys with `on_key`:

```rust
use std::process;

use amane::{App, Key, Keyboard, LayerWindow, Text};

fn main() {
    App::new().window(view).run();
}

fn view() -> LayerWindow {
    LayerWindow::new()
        .width(300.0)
        .height(40.0)
        .keyboard(Keyboard::Exclusive)
        .on_key(key_pressed)
        .child(Text::new("press escape to quit"))
}

fn key_pressed(key: Key) {
    if key == Key::Escape {
        process::exit(0);
    }
}
```

`std::process` is Rust's standard library.

`Key` is one of:

- `Character(char)`: a key that types something, already shifted, like `Character('A')`
- `Enter`, `Escape`, `Tab`, `Backspace`, `Space`
- `Up`, `Down`, `Left`, `Right`, `Home`, `End`, `PageUp`, `PageDown`
- `Delete`, `Insert`
- `F(n)`: a function key, `F(1)` to `F(24)`
- `Other`: any key Amane has no name for yet

Keypad keys arrive as the key they stand for, so keypad `4` with Num Lock off is `Left`.

A held key repeats at the rate the compositor asks for, so holding `Backspace` keeps deleting. Each repeat calls `on_key` again.

`on_key_release` runs when a key goes up, for things that last while a key is held, like peeking at something while `Alt` is down. Releases skip text inputs.

### When a text input has focus

A focused `TextInput` takes keys before the window does, because typing should type:

- Text keys and the editing keys go to the input only ([Text Input](text_input.md#keys)). A chord the input has no use for, like `Ctrl+Q`, goes on to the window.
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

A `Ctrl`+letter chord arrives as `Character(letter)` with `modifiers().ctrl` set (not as `Other`). `Modifiers::command()` is a shortcut for "ctrl, alt, or super is held" — i.e. this key is a command, not text. A focused `TextInput` already ignores command chords instead of typing them.

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

## Terms

- **Handler:** a function Amane calls when an event happens.
- **Target:** the area a rectangle with handlers took up in the last frame.
- **Keyboard focus:** the window the compositor sends keys to.
