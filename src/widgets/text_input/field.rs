use crate::{Key, Modifiers};

// how many edits undo remembers per input
const UNDO_LIMIT: usize = 200;

// what a text input holds: its text, the cursor and selection, and what undo can go back to
#[derive(Clone, Default)]
pub struct Field {
    pub text: String,

    // counts letters, not bytes
    pub cursor: usize,

    // the other end of the selection, none without one
    pub anchor: Option<usize>,

    // how far a one-line input has scrolled to keep the cursor in sight, in pixels
    pub scroll: f32,

    // text an input method is still putting together, shown at the cursor but not typed yet
    pub preedit: Option<Preedit>,

    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,

    // typing letter after letter is undone as one step
    last_edit: Edit,
}

#[derive(Clone, Default, PartialEq)]
pub struct Preedit {
    pub text: String,

    // where the input method's cursor is in it, in letters
    pub cursor: Option<usize>,
}

#[derive(Clone)]
struct Snapshot {
    text: String,
    cursor: usize,
}

#[derive(Clone, Copy, Default, PartialEq)]
enum Edit {
    #[default]
    None,
    Typing,
    Deleting,
    Other,
}

// what a key did, beyond the text and cursor the field changed itself
#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    // the key was one the input uses; false lets it go on to the window
    pub used: bool,

    pub changed: bool,

    // text to put on the clipboard
    pub copy: Option<String>,

    // the clipboard's text should go in at the cursor
    pub paste: bool,

    pub submit: bool,
}

impl Outcome {
    fn used() -> Self {
        Self {
            used: true,
            ..Self::default()
        }
    }

    fn changed(changed: bool) -> Self {
        Self {
            used: true,
            changed,
            ..Self::default()
        }
    }
}

impl Field {
    pub fn new(text: &str) -> Self {
        Self {
            text: String::from(text),
            cursor: text.chars().count(),
            ..Self::default()
        }
    }

    // what a key does to the input, as text fields usually do
    pub fn apply(&mut self, key: Key, held: Modifiers, multiline: bool) -> Outcome {
        let word = held.ctrl;
        let extend = held.shift;

        // a command chord is not text: these are the ones an input knows, the rest go on
        if held.command() && matches!(key, Key::Character(_) | Key::Space) {
            return self.chord(key, held);
        }

        match key {
            Key::Character(letter) => {
                let mut typed = [0; 4];

                Outcome::changed(self.type_text(letter.encode_utf8(&mut typed)))
            }

            Key::Space => Outcome::changed(self.type_text(" ")),

            Key::Enter if multiline => Outcome::changed(self.insert("\n")),

            Key::Enter => Outcome {
                used: true,
                submit: true,
                ..Outcome::default()
            },

            Key::Backspace if word => Outcome::changed(self.delete_to(self.word_before())),
            Key::Backspace => Outcome::changed(self.delete_to(self.cursor.saturating_sub(1))),

            Key::Delete if word => Outcome::changed(self.delete_to(self.word_after())),
            Key::Delete => Outcome::changed(self.delete_to(self.cursor + 1)),

            Key::Left if word => self.go(self.word_before(), extend),
            Key::Right if word => self.go(self.word_after(), extend),

            // without shift, an arrow first lands on that side of the selection
            Key::Left if !extend && self.anchor.is_some() => self.go(self.selection().0, false),
            Key::Right if !extend && self.anchor.is_some() => self.go(self.selection().1, false),

            Key::Left => self.go(self.cursor.saturating_sub(1), extend),
            Key::Right => self.go(self.cursor + 1, extend),

            Key::Home if word || !multiline => self.go(0, extend),
            Key::End if word || !multiline => self.go(self.length(), extend),

            Key::Home => self.go(self.line_start(self.cursor), extend),
            Key::End => self.go(self.line_end(self.cursor), extend),

            Key::Up if multiline => self.go(self.line_step(-1), extend),
            Key::Down if multiline => self.go(self.line_step(1), extend),

            _ => Outcome::default(),
        }
    }

    fn chord(&mut self, key: Key, held: Modifiers) -> Outcome {
        if !held.ctrl || held.alt || held.logo {
            return Outcome::default();
        }

        match key {
            Key::Character('a' | 'A') => {
                self.select(0, self.length());

                Outcome::used()
            }

            Key::Character('c' | 'C') => Outcome {
                used: true,
                copy: Some(self.selected()).filter(|text| !text.is_empty()),
                ..Outcome::default()
            },

            Key::Character('x' | 'X') => {
                let copy = Some(self.selected()).filter(|text| !text.is_empty());

                Outcome {
                    used: true,
                    changed: copy.is_some() && self.replace_selection(""),
                    copy,
                    ..Outcome::default()
                }
            }

            Key::Character('v' | 'V') => Outcome {
                used: true,
                paste: true,
                ..Outcome::default()
            },

            Key::Character('z') if !held.shift => Outcome::changed(self.undo()),
            Key::Character('Z' | 'z' | 'y' | 'Y') => Outcome::changed(self.redo()),

            _ => Outcome::default(),
        }
    }

    // the text before the cursor, for drawing where it sits
    pub fn before_cursor(&self) -> String {
        self.text.chars().take(self.cursor).collect()
    }

    // the selection as an ordered range, empty at the cursor without one
    pub fn selection(&self) -> (usize, usize) {
        match self.anchor {
            Some(anchor) => (anchor.min(self.cursor), anchor.max(self.cursor)),
            None => (self.cursor, self.cursor),
        }
    }

    pub fn selected(&self) -> String {
        let (from, to) = self.selection();

        self.text.chars().skip(from).take(to - from).collect()
    }

    // selects from `from` to `to`, with the cursor at `to`
    pub fn select(&mut self, from: usize, to: usize) {
        self.anchor = Some(from.min(self.length()));
        self.cursor = to.min(self.length());
        self.last_edit = Edit::None;

        self.settle();
    }

    // moves the cursor, extending the selection or dropping it
    pub fn move_to(&mut self, to: usize, extend: bool) {
        if extend {
            self.anchor.get_or_insert(self.cursor);
        } else {
            self.anchor = None;
        }

        self.cursor = to.min(self.length());
        self.last_edit = Edit::None;

        self.settle();
    }

    // puts text in at the cursor, over the selection; one line leaves line breaks out
    pub fn paste(&mut self, text: &str, multiline: bool) -> bool {
        let clean: String = text
            .chars()
            .filter(|letter| (multiline && *letter == '\n') || !letter.is_control())
            .collect();

        self.insert(&clean)
    }

    // what an input method finished typing: what it took away around the cursor, then its text
    pub fn commit_input_method(&mut self, before: usize, after: usize, text: &str) -> bool {
        self.preedit = None;

        let start = self.cursor.saturating_sub(before);
        let end = (self.cursor + after).min(self.length());

        let removed = start < end && {
            self.anchor = Some(start);
            self.cursor = end;

            self.replace_selection("")
        };

        let typed = !text.is_empty() && self.insert(text);

        removed || typed
    }

    fn go(&mut self, to: usize, extend: bool) -> Outcome {
        self.move_to(to, extend);

        Outcome::used()
    }

    fn type_text(&mut self, text: &str) -> bool {
        // a run of typing is one undo step per word, a new one starting after a space
        let after_space = self.before_cursor().ends_with(char::is_whitespace);

        let joins = self.last_edit == Edit::Typing
            && self.anchor.is_none()
            && !(after_space && !text.starts_with(char::is_whitespace));

        if !joins {
            self.remember();
        }

        let changed = self.replace_selection(text);

        self.last_edit = Edit::Typing;

        changed
    }

    fn insert(&mut self, text: &str) -> bool {
        if text.is_empty() && self.anchor.is_none() {
            return false;
        }

        self.remember();

        let changed = self.replace_selection(text);

        self.last_edit = Edit::Other;

        changed
    }

    // deletes the selection, or up to `to` from the cursor without one
    fn delete_to(&mut self, to: usize) -> bool {
        if self.anchor.is_none() {
            let to = to.min(self.length());

            if to == self.cursor {
                return false;
            }

            self.anchor = Some(to);
        }

        if self.last_edit != Edit::Deleting {
            self.remember();
        }

        let changed = self.replace_selection("");

        self.last_edit = Edit::Deleting;

        changed
    }

    fn replace_selection(&mut self, text: &str) -> bool {
        let (from, to) = self.selection();

        if from == to && text.is_empty() {
            self.anchor = None;

            return false;
        }

        let range = self.byte_index(from)..self.byte_index(to);

        self.text.replace_range(range, text);

        self.cursor = from + text.chars().count();
        self.anchor = None;

        true
    }

    fn remember(&mut self) {
        self.undo.push(Snapshot {
            text: self.text.clone(),
            cursor: self.cursor,
        });

        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }

        self.redo.clear();
    }

    fn undo(&mut self) -> bool {
        let Some(snapshot) = self.undo.pop() else {
            return false;
        };

        let now = self.restore(snapshot);

        self.redo.push(now);

        true
    }

    fn redo(&mut self) -> bool {
        let Some(snapshot) = self.redo.pop() else {
            return false;
        };

        let now = self.restore(snapshot);

        self.undo.push(now);

        true
    }

    // puts a snapshot back and returns what it replaced
    fn restore(&mut self, snapshot: Snapshot) -> Snapshot {
        let now = Snapshot {
            text: std::mem::replace(&mut self.text, snapshot.text),
            cursor: self.cursor,
        };

        self.cursor = snapshot.cursor.min(self.length());
        self.anchor = None;
        self.last_edit = Edit::None;

        now
    }

    // an empty selection is none
    fn settle(&mut self) {
        if self.anchor == Some(self.cursor) {
            self.anchor = None;
        }
    }

    pub fn length(&self) -> usize {
        self.text.chars().count()
    }

    // where the word before the cursor starts, skipping the spaces before it
    fn word_before(&self) -> usize {
        let letters: Vec<char> = self.text.chars().collect();

        let mut at = self.cursor.min(letters.len());

        while at > 0 && !is_word(letters[at - 1]) {
            at -= 1;
        }

        while at > 0 && is_word(letters[at - 1]) {
            at -= 1;
        }

        at
    }

    // where the word after the cursor ends, skipping the spaces before it
    fn word_after(&self) -> usize {
        let letters: Vec<char> = self.text.chars().collect();

        let mut at = self.cursor.min(letters.len());

        while at < letters.len() && !is_word(letters[at]) {
            at += 1;
        }

        while at < letters.len() && is_word(letters[at]) {
            at += 1;
        }

        at
    }

    // selects the word around a letter, for a double click
    pub fn select_word(&mut self, at: usize) {
        let letters: Vec<char> = self.text.chars().collect();

        let at = at.min(letters.len());

        let mut start = at;
        let mut end = at;

        while start > 0 && is_word(letters[start - 1]) {
            start -= 1;
        }

        while end < letters.len() && is_word(letters[end]) {
            end += 1;
        }

        self.select(start, end);
    }

    // selects the line a letter is on, for a triple click
    pub fn select_line(&mut self, at: usize) {
        self.select(self.line_start(at), self.line_end(at));
    }

    // the line a letter is on and how far along it, both counted from 0
    pub fn line_and_column(&self, at: usize) -> (usize, usize) {
        let before: String = self.text.chars().take(at).collect();

        let line = before.matches('\n').count();
        let column = before
            .chars()
            .rev()
            .take_while(|letter| *letter != '\n')
            .count();

        (line, column)
    }

    // the letter at a line and column, the column kept inside the line
    pub fn index_of(&self, line: usize, column: usize) -> usize {
        let mut index = 0;

        for (number, text) in self.text.split('\n').enumerate() {
            let length = text.chars().count();

            if number == line {
                return index + column.min(length);
            }

            // the line and its break
            index += length + 1;
        }

        self.length()
    }

    fn line_start(&self, at: usize) -> usize {
        let (line, _) = self.line_and_column(at);

        self.index_of(line, 0)
    }

    fn line_end(&self, at: usize) -> usize {
        let (line, _) = self.line_and_column(at);

        self.index_of(line, usize::MAX)
    }

    // the cursor one line up or down, keeping its column where the line is long enough
    fn line_step(&self, step: isize) -> usize {
        let (line, column) = self.line_and_column(self.cursor);

        match line.checked_add_signed(step) {
            Some(line) if line <= self.text.matches('\n').count() => self.index_of(line, column),

            // past the first or last line goes to that end of the text
            _ if step < 0 => 0,
            _ => self.length(),
        }
    }

    // a String is indexed by bytes, and one letter can take several
    fn byte_index(&self, at: usize) -> usize {
        match self.text.char_indices().nth(at) {
            Some((index, _)) => index,
            None => self.text.len(),
        }
    }
}

fn is_word(letter: char) -> bool {
    letter.is_alphanumeric() || letter == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Modifiers = Modifiers {
        ctrl: false,
        shift: false,
        alt: false,
        logo: false,
    };

    const CTRL: Modifiers = Modifiers { ctrl: true, ..NONE };
    const SHIFT: Modifiers = Modifiers {
        shift: true,
        ..NONE
    };

    fn typed(text: &str) -> Field {
        let mut field = Field::default();

        for letter in text.chars() {
            field.apply(Key::Character(letter), NONE, true);
        }

        field
    }

    #[test]
    fn edits_by_letter_not_by_byte() {
        let mut field = typed("héllo");

        field.apply(Key::Left, NONE, false);
        field.apply(Key::Left, NONE, false);
        field.apply(Key::Left, NONE, false);

        // the cursor sits after "hé", two letters but three bytes
        assert_eq!(field.before_cursor(), "hé");

        assert!(field.apply(Key::Backspace, NONE, false).changed);
        assert_eq!(field.text, "hllo");

        assert!(!field.apply(Key::Home, NONE, false).changed);
        assert!(!field.apply(Key::Backspace, NONE, false).changed);

        field.apply(Key::End, NONE, false);
        field.apply(Key::Space, NONE, false);

        assert_eq!(field.text, "hllo ");
        assert_eq!(field.cursor, 5);
    }

    #[test]
    fn shift_selects_and_typing_replaces_the_selection() {
        let mut field = typed("hello world");

        field.apply(Key::Left, SHIFT, false);
        field.apply(Key::Left, SHIFT, false);

        assert_eq!(field.selected(), "ld");

        field.apply(Key::Character('!'), NONE, false);

        assert_eq!(field.text, "hello wor!");
        assert_eq!(field.anchor, None);
    }

    #[test]
    fn ctrl_moves_and_deletes_by_word() {
        let mut field = typed("one two  three");

        assert!(field.apply(Key::Backspace, CTRL, false).changed);
        assert_eq!(field.text, "one two  ");

        field.apply(Key::Left, CTRL, false);
        assert_eq!(field.cursor, 4);

        field.apply(Key::Delete, CTRL, false);
        assert_eq!(field.text, "one   ");
    }

    #[test]
    fn copy_cut_and_paste_are_handed_out() {
        let mut field = typed("abc");

        field.apply(Key::Character('a'), CTRL, false);

        let copied = field.apply(Key::Character('c'), CTRL, false);
        assert_eq!(copied.copy.as_deref(), Some("abc"));

        let cut = field.apply(Key::Character('x'), CTRL, false);
        assert_eq!(cut.copy.as_deref(), Some("abc"));
        assert!(cut.changed);
        assert_eq!(field.text, "");

        assert!(field.apply(Key::Character('v'), CTRL, false).paste);

        // chords an input has no use for go on to the window
        assert!(!field.apply(Key::Character('q'), CTRL, false).used);
    }

    #[test]
    fn undo_takes_back_a_run_of_typing_at_once() {
        let mut field = typed("hello");

        field.apply(Key::Space, NONE, false);

        for letter in "world".chars() {
            field.apply(Key::Character(letter), NONE, false);
        }

        field.apply(Key::Character('z'), CTRL, false);
        assert_eq!(field.text, "hello ");

        field.apply(Key::Character('z'), CTRL, false);
        assert_eq!(field.text, "");

        field.apply(
            Key::Character('Z'),
            Modifiers {
                shift: true,
                ..CTRL
            },
            false,
        );
        assert_eq!(field.text, "hello ");

        // moving the cursor ends the run, so the next letters are a step of their own
        field.apply(Key::Home, NONE, false);
        field.apply(Key::Character('>'), NONE, false);
        field.apply(Key::Character('z'), CTRL, false);
        assert_eq!(field.text, "hello ");
    }

    #[test]
    fn lines_move_up_and_down_keeping_the_column() {
        let mut field = typed("first\nab\nthird");

        field.apply(Key::Up, NONE, true);
        assert_eq!(field.line_and_column(field.cursor), (1, 2));

        field.apply(Key::Up, NONE, true);
        assert_eq!(field.line_and_column(field.cursor), (0, 2));

        field.apply(Key::End, NONE, true);
        assert_eq!(field.cursor, 5);

        // one line turns Enter into submit, many lines into a line break
        assert!(field.apply(Key::Enter, NONE, false).submit);
        assert!(field.apply(Key::Enter, NONE, true).changed);
    }

    #[test]
    fn double_and_triple_clicks_select_a_word_and_a_line() {
        let mut field = typed("one two\nthree");

        field.select_word(5);
        assert_eq!(field.selected(), "two");

        field.select_line(2);
        assert_eq!(field.selected(), "one two");
    }

    #[test]
    fn pasting_into_one_line_leaves_line_breaks_out() {
        let mut field = Field::default();

        field.paste("a\nb\tc", false);

        assert_eq!(field.text, "abc");
    }

    #[test]
    fn an_input_method_replaces_around_the_cursor() {
        let mut field = typed("nihao");

        assert!(field.commit_input_method(5, 0, "你好"));

        assert_eq!(field.text, "你好");
        assert_eq!(field.cursor, 2);
    }
}
