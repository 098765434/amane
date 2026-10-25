use std::cell::Cell;
use std::rc::Rc;

use crate::graphics::Area;
use crate::input::focus::{self, InputHandler};
use crate::input::{Cursor, Handlers, Target};
use crate::{
    Button, Clipboard, Key, Point, PrimarySelection, click_count, modifiers, pointer_position,
};

use super::{TextHandler, TextInput, fields};

thread_local! {
    // the input the left button is selecting text in, until it is let go
    static SELECTING: Cell<Option<&'static str>> = const { Cell::new(None) };
}

pub fn collect_targets(text_input: &TextInput, area: Area, targets: &mut Vec<Target>) {
    let id = text_input.id;
    let measure = text_input.measure();
    let multiline = text_input.multiline;

    let on_key = key_handler(text_input);

    // text that arrives later, like a paste, tells this redraw's on_change
    fields::set_on_change(id, text_input.on_change.clone());
    fields::set_kind(id, text_input.password, multiline);

    // the handler holds this redraw's on_change and on_submit, so a focused input takes the new one
    if focus::has(id) || text_input.focused {
        focus::set(id, on_key.clone(), multiline);
    }

    let focus_on_click = on_key.clone();

    // a middle click pastes the primary selection where it was clicked
    let on_click = move |button: Button| {
        focus::set(id, focus_on_click.clone(), multiline);

        if button != Button::Middle {
            return;
        }

        let pointer = pointer_position();

        let point = Point {
            x: pointer.x - area.x,
            y: pointer.y - area.y,
        };

        let at = measure.index_at(&fields::get(id), point);

        PrimarySelection::paste(move |text| {
            fields::change(id, |field| {
                field.move_to(at, false);

                field.paste(&text, multiline)
            });
        });
    };

    // the press places the cursor, moving with the button down selects
    let on_drag = move |point: Point| {
        let mut field = fields::get(id);

        let at = measure.index_at(&field, point);

        if SELECTING.get() == Some(id) {
            // a double or triple click keeps what it selected while the button is down
            if click_count() == 1 {
                field.move_to(at, true);
            }
        } else {
            SELECTING.set(Some(id));

            focus::set(id, on_key.clone(), multiline);

            match click_count() {
                1 => field.move_to(at, modifiers().shift),
                2 => field.select_word(at),
                _ => field.select_line(at),
            }
        }

        fields::set(id, field);
    };

    let password = text_input.password;

    // what was selected with the pointer is what a middle click pastes elsewhere
    let on_release = move |_: Point| {
        SELECTING.set(None);

        let selected = fields::get(id).selected();

        if !selected.is_empty() && !password {
            PrimarySelection::copy(selected);
        }
    };

    let handlers = Handlers {
        click: Some(Rc::new(on_click)),
        drag: Some(Rc::new(on_drag)),
        release: Some(Rc::new(on_release)),
        cursor: Some(Cursor::Text),
        ..Handlers::default()
    };

    targets.push(Target::new(area, handlers));
}

fn key_handler(text_input: &TextInput) -> InputHandler {
    let id = text_input.id;
    let multiline = text_input.multiline;
    let password = text_input.password;

    let on_change = text_input.on_change.clone();
    let on_submit = text_input.on_submit.clone();

    let on_key = move |key: Key| {
        if key == Key::Escape {
            focus::clear();

            return true;
        }

        let held = modifiers();

        let mut field = fields::get(id);

        let outcome = field.apply(key, held, multiline);

        let text = field.text.clone();
        let selected = field.selected();

        // stored before the handlers run, so they read the new text
        fields::set(id, field);

        // a password never leaves the input
        if let Some(copy) = outcome.copy.filter(|_| !password) {
            Clipboard::copy(copy);
        }

        if outcome.paste {
            Clipboard::paste(move |text| fields::change(id, |field| field.paste(&text, multiline)));
        }

        if outcome.changed {
            run(on_change.as_ref(), &text);
        }

        if outcome.submit {
            run(on_submit.as_ref(), &text);
        }

        // text selected with the keyboard is what a middle click pastes elsewhere
        if held.shift && !selected.is_empty() && !password {
            PrimarySelection::copy(selected);
        }

        outcome.used
    };

    Rc::new(on_key)
}

fn run(handler: Option<&TextHandler>, text: &str) {
    if let Some(handler) = handler {
        handler(String::from(text));
    }
}
