mod field;
mod fields;
mod targets;

use std::rc::Rc;

use crate::graphics::{Area, Renderer};
use crate::input::{Target, focus};
use crate::{Color, Point, Size, Text};

use super::Widget;

use field::Field;

pub(crate) use field::Preedit;
pub(crate) use fields::{change, cursor_area, get as field, kind};

// shared, so the key handler kept after a redraw still reaches it
pub type TextHandler = Rc<dyn Fn(String)>;

const CURSOR_WIDTH: f32 = 1.5;

pub struct TextInput {
    // the view builds a fresh input on every redraw, so the text is kept under this name
    id: &'static str,

    placeholder: String,
    size: f32,
    color: Color,
    selection_color: Option<Color>,
    width: Size,

    // shows a dot in place of each letter
    password: bool,

    // Enter starts a new line instead of submitting
    multiline: bool,

    // takes the keyboard as soon as it is drawn, without a click
    focused: bool,

    on_change: Option<TextHandler>,
    on_submit: Option<TextHandler>,
}

impl TextInput {
    pub fn new(id: &'static str) -> Self {
        Self {
            id,
            placeholder: String::new(),
            size: 16.0,
            color: Color::BLACK,
            selection_color: None,
            width: Size::Parent,
            password: false,
            multiline: false,
            focused: false,
            on_change: None,
            on_submit: None,
        }
    }

    pub fn width(mut self, width: impl Into<Size>) -> Self {
        self.width = width.into();

        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;

        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;

        self
    }

    // behind selected text; the text color, faint, by default
    pub fn selection_color(mut self, color: Color) -> Self {
        self.selection_color = Some(color);

        self
    }

    // shown faded while the input is empty
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();

        self
    }

    pub fn password(mut self) -> Self {
        self.password = true;

        self
    }

    /*
     * many lines: Enter starts a new line and Up and Down move between lines; it is as
     * tall as its lines, put it in a ScrollArea to scroll a long text
     */
    pub fn multiline(mut self, multiline: bool) -> Self {
        self.multiline = multiline;

        self
    }

    pub fn focused(mut self) -> Self {
        self.focused = true;

        self
    }

    // replaces what the input holds, with the cursor at the end
    pub fn set_text(id: &'static str, text: &str) {
        fields::set(id, Field::new(text));
    }

    // what the input holds right now
    pub fn text(id: &'static str) -> String {
        fields::get(id).text
    }

    // runs after every change to the text, with the whole new text
    pub fn on_change(mut self, handler: impl Fn(String) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));

        self
    }

    // runs when Enter is pressed in a one-line input
    pub fn on_submit(mut self, handler: impl Fn(String) + 'static) -> Self {
        self.on_submit = Some(Rc::new(handler));

        self
    }

    fn measure(&self) -> Measure {
        Measure {
            size: self.size,
            password: self.password,
            multiline: self.multiline,
        }
    }

    fn label(&self, content: impl Into<String>, color: Color) -> Text {
        Text::new(content).size(self.size).color(color)
    }

    // one line of text, scrolled, with its selection behind it and the cursor and preedit on it
    fn draw_line(&self, renderer: &mut Renderer, area: Area, line: Line) {
        let measure = self.measure();

        let shown = measure.shown(&line.text);

        let x = |column: usize| area.x - line.scroll + measure.x_of(&shown, column);

        if let Some((from, to)) = line.selected {
            let selection = Area::new(x(from), area.y, x(to) - x(from), area.height);

            let color = self.selection_color.unwrap_or(Color::rgba(
                self.color.r,
                self.color.g,
                self.color.b,
                self.color.a / 4,
            ));

            renderer.rectangle(selection, color, 0.0.into());
        }

        let Some((cursor, preedit)) = line.cursor else {
            let text = Area::new(
                area.x - line.scroll,
                area.y,
                area.width + line.scroll,
                area.height,
            );

            self.label(shown, self.color).draw(renderer, text);

            return;
        };

        // an input method's unfinished text sits at the cursor, underlined, until it is typed
        let before: String = shown.chars().take(cursor).collect();
        let after: String = shown.chars().skip(cursor).collect();

        let preedit_text = preedit.as_ref().map_or("", |preedit| preedit.text.as_str());

        let text = Area::new(
            area.x - line.scroll,
            area.y,
            area.width + line.scroll,
            area.height,
        );

        self.label(format!("{before}{preedit_text}{after}"), self.color)
            .draw(renderer, text);

        let start = x(cursor);

        if !preedit_text.is_empty() {
            let width = measure.width_of(preedit_text);

            let underline = Area::new(start, area.y + area.height - 1.5, width, 1.0);

            renderer.rectangle(underline, self.color, 0.0.into());
        }

        // the input method's own cursor inside its text, else after it
        let inside = preedit.as_ref().map_or(0.0, |preedit| {
            let letters = preedit.cursor.unwrap_or(preedit.text.chars().count());

            let typed: String = preedit.text.chars().take(letters).collect();

            measure.width_of(&typed)
        });

        let at = Area::new(start + inside, area.y, CURSOR_WIDTH, area.height);

        renderer.rectangle(at, self.color, 0.0.into());

        fields::set_cursor_area(at.x, at.y, at.width, at.height);
    }
}

// what draw_line needs of one line
struct Line {
    text: String,
    scroll: f32,

    // the selected part of it, in letters
    selected: Option<(usize, usize)>,

    // where the cursor is on it, if it is, with any unfinished input method text
    cursor: Option<(usize, Option<Preedit>)>,
}

// how an input measures its text, kept by its handlers after the widget is gone
#[derive(Clone, Copy)]
struct Measure {
    size: f32,
    password: bool,
    multiline: bool,
}

impl Measure {
    // the text as it is drawn, which hides a password
    fn shown(&self, text: &str) -> String {
        if !self.password {
            return String::from(text);
        }

        "•".repeat(text.chars().count())
    }

    fn width_of(&self, text: &str) -> f32 {
        Text::new(text).size(self.size).width().resolve(0.0)
    }

    fn line_height(&self) -> f32 {
        Text::new("").size(self.size).height().resolve(0.0)
    }

    // how far along the shown text a letter starts
    fn x_of(&self, shown: &str, column: usize) -> f32 {
        let before: String = shown.chars().take(column).collect();

        self.width_of(&before)
    }

    // the letter boundary nearest to x on a line
    fn column_at(&self, line: &str, x: f32) -> usize {
        let shown = self.shown(line);

        let mut nearest = 0;
        let mut distance = f32::MAX;

        for column in 0..=shown.chars().count() {
            let away = (self.x_of(&shown, column) - x).abs();

            if away < distance {
                nearest = column;
                distance = away;
            }
        }

        nearest
    }

    // the letter a point on the input is nearest to
    fn index_at(&self, field: &Field, point: Point) -> usize {
        if !self.multiline {
            return self.column_at(&field.text, point.x + field.scroll);
        }

        let lines: Vec<&str> = field.text.split('\n').collect();

        let line = (point.y / self.line_height()).floor().max(0.0) as usize;
        let line = line.min(lines.len() - 1);

        field.index_of(line, self.column_at(lines[line], point.x))
    }
}

impl Widget for TextInput {
    fn width(&self) -> Size {
        self.width
    }

    fn height(&self) -> Size {
        let lines = if self.multiline {
            fields::get(self.id).text.matches('\n').count() + 1
        } else {
            1
        };

        Size::Fixed(self.measure().line_height() * lines as f32)
    }

    fn draw(&self, renderer: &mut Renderer, area: Area) {
        let mut field = fields::get(self.id);

        let focused = focus::has(self.id);

        // the placeholder is the text color at half strength
        if field.text.is_empty() && field.preedit.is_none() {
            let faded = Color::rgba(self.color.r, self.color.g, self.color.b, self.color.a / 2);

            let mut inside = renderer.group();

            self.label(self.placeholder.as_str(), faded)
                .draw(&mut inside, area);

            if focused {
                let at = Area::new(area.x, area.y, CURSOR_WIDTH, area.height);

                inside.rectangle(at, self.color, 0.0.into());

                fields::set_cursor_area(at.x, at.y, at.width, at.height);
            }

            renderer.clip(inside, area, 0.0.into());

            return;
        }

        let measure = self.measure();

        // a one-line input scrolls sideways so the cursor stays in sight
        if !self.multiline {
            let shown = measure.shown(&field.text);

            let cursor = measure.x_of(&shown, field.cursor);
            let end = measure.width_of(&shown);

            let room = (area.width - CURSOR_WIDTH).max(0.0);

            let mut scroll = field.scroll;

            if cursor - scroll > room {
                scroll = cursor - room;
            }

            if cursor < scroll {
                scroll = cursor;
            }

            scroll = scroll.min((end - room).max(0.0)).max(0.0);

            if scroll != field.scroll {
                field.scroll = scroll;

                fields::set(self.id, field.clone());
            }
        }

        let (from, to) = field.selection();
        let (cursor_line, cursor_column) = field.line_and_column(field.cursor);

        let line_height = measure.line_height();

        // text longer than the input is cut off at its edge
        let mut inside = renderer.group();

        let mut start = 0;

        for (number, text) in field.text.split('\n').enumerate() {
            let length = text.chars().count();

            let selected = (from < to && from <= start + length && to >= start).then(|| {
                let first = from.saturating_sub(start);
                let last = (to - start).min(length);

                (first, last)
            });

            let cursor =
                (focused && number == cursor_line).then(|| (cursor_column, field.preedit.clone()));

            let line = Line {
                text: String::from(text),
                scroll: field.scroll,
                selected,
                cursor,
            };

            let row = Area::new(
                area.x,
                area.y + line_height * number as f32,
                area.width,
                line_height,
            );

            self.draw_line(&mut inside, row, line);

            start += length + 1;
        }

        renderer.clip(inside, area, 0.0.into());
    }

    fn collect_targets(&self, area: Area, targets: &mut Vec<Target>) {
        targets::collect_targets(self, area, targets);
    }
}
