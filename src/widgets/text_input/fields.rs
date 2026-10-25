use std::cell::RefCell;
use std::collections::HashMap;

use crate::changes;

use super::TextHandler;
use super::field::Field;

thread_local! {
    // what each text input holds, kept apart from the widgets that are rebuilt each redraw
    static FIELDS: RefCell<HashMap<&'static str, Field>> = RefCell::new(HashMap::new());

    // each input's last on_change, for text that arrives after its key, like a paste
    static ON_CHANGE: RefCell<HashMap<&'static str, TextHandler>> = RefCell::new(HashMap::new());

    // whether each input is a password and has many lines, for the input method
    static KINDS: RefCell<HashMap<&'static str, (bool, bool)>> = RefCell::new(HashMap::new());

    // where the focused input's cursor was last drawn, in window units: x, y, width, height
    static CURSOR: std::cell::Cell<(f32, f32, f32, f32)> = const { std::cell::Cell::new((0.0, 0.0, 0.0, 0.0)) };
}

pub fn get(id: &'static str) -> Field {
    FIELDS.with_borrow(|fields| fields.get(id).cloned().unwrap_or_default())
}

pub fn set(id: &'static str, field: Field) {
    FIELDS.with_borrow_mut(|fields| {
        fields.insert(id, field);
    });
}

pub fn set_on_change(id: &'static str, on_change: Option<TextHandler>) {
    ON_CHANGE.with_borrow_mut(|handlers| match on_change {
        Some(on_change) => {
            handlers.insert(id, on_change);
        }

        None => {
            handlers.remove(id);
        }
    });
}

/*
 * changes an input from outside its key handler, like a paste or an input method;
 * tells its on_change when the text changed, and draws it again either way
 */
pub fn change(id: &'static str, edit: impl FnOnce(&mut Field) -> bool) {
    let mut field = get(id);

    let changed = edit(&mut field);
    let text = field.text.clone();

    set(id, field);

    if changed && let Some(on_change) = ON_CHANGE.with_borrow(|handlers| handlers.get(id).cloned())
    {
        on_change(text);
    }

    // the field is no service, so every window looks again
    changes::mark_all();
}

pub fn set_kind(id: &'static str, password: bool, multiline: bool) {
    KINDS.with_borrow_mut(|kinds| {
        kinds.insert(id, (password, multiline));
    });
}

pub fn kind(id: &'static str) -> (bool, bool) {
    KINDS.with_borrow(|kinds| kinds.get(id).copied().unwrap_or_default())
}

pub fn set_cursor_area(x: f32, y: f32, width: f32, height: f32) {
    CURSOR.set((x, y, width, height));
}

pub fn cursor_area() -> (f32, f32, f32, f32) {
    CURSOR.get()
}
