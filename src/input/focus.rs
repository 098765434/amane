use std::cell::RefCell;
use std::rc::Rc;

use crate::Key;

// true when the text input used the key, false lets it go on to the window
pub type InputHandler = Rc<dyn Fn(Key) -> bool>;

// the text input that typed keys go to, and what it does with them
struct Focus {
    id: &'static str,
    on_key: InputHandler,

    // an input with many lines moves between them with Up and Down
    multiline: bool,
}

thread_local! {
    // only one text input takes keys at a time
    static FOCUS: RefCell<Option<Focus>> = const { RefCell::new(None) };
}

pub fn set(id: &'static str, on_key: InputHandler, multiline: bool) {
    FOCUS.set(Some(Focus {
        id,
        on_key,
        multiline,
    }));
}

pub fn clear() {
    FOCUS.set(None);
}

pub fn has(id: &'static str) -> bool {
    FOCUS.with_borrow(|focus| matches!(focus, Some(focus) if focus.id == id))
}

// the text input that has focus, if any
pub fn current() -> Option<&'static str> {
    FOCUS.with_borrow(|focus| focus.as_ref().map(|focus| focus.id))
}

// returns whether a text input took the key
pub fn send(key: Key) -> bool {
    let Some((on_key, multiline)) = FOCUS.with_borrow(|focus| {
        focus
            .as_ref()
            .map(|focus| (focus.on_key.clone(), focus.multiline))
    }) else {
        return false;
    };

    // a one-line input has no use for these, so they go on to the window's on_key
    let lines = matches!(key, Key::Up | Key::Down | Key::PageUp | Key::PageDown);

    if (lines && !multiline) || matches!(key, Key::Tab | Key::Insert | Key::F(_) | Key::Other) {
        return false;
    }

    // copied out first, so the handler is free to change the focus itself
    let used = on_key(key);

    // escape leaves the input and still reaches the window, which may want to close
    used && key != Key::Escape
}
