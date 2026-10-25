use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use crate::changes;

// the clipboard Ctrl+C and Ctrl+V use, shared with every other program
pub struct Clipboard;

// the text selected last, which a middle click pastes
pub struct PrimarySelection;

impl Clipboard {
    /*
     * puts text on the clipboard, for other programs to paste; it has to come from
     * input, like a key or a click, or the compositor may turn it down
     */
    pub fn copy(text: impl Into<String>) {
        request(Request::Copy(Board::Clipboard, text.into()));
    }

    /*
     * reads the clipboard and calls `then` with its text, once the program that copied it
     * has answered; it runs on the event loop like a handler, and not at all when there is
     * no text on it
     */
    pub fn paste(then: impl FnOnce(String) + Send + 'static) {
        request(Request::Paste(Board::Clipboard, Box::new(then)));
    }
}

impl PrimarySelection {
    // the same as Clipboard::copy, for the text a middle click pastes
    pub fn copy(text: impl Into<String>) {
        request(Request::Copy(Board::Primary, text.into()));
    }

    // the same as Clipboard::paste
    pub fn paste(then: impl FnOnce(String) + Send + 'static) {
        request(Request::Paste(Board::Primary, Box::new(then)));
    }
}

// what a rectangle hands over when it is dragged out to another program
#[derive(Clone, Debug, PartialEq)]
pub enum DragData {
    Text(String),
    Files(Vec<PathBuf>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Board {
    Clipboard,
    Primary,
}

pub(crate) type Then = Box<dyn FnOnce(String) + Send>;

pub(crate) enum Request {
    Copy(Board, String),
    Paste(Board, Then),
}

// done by the event loop when it wakes, which holds what the compositor needs
pub(crate) static REQUESTS: Mutex<Vec<Request>> = Mutex::new(Vec::new());


fn request(request: Request) {
    REQUESTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(request);

    changes::wake();
}
