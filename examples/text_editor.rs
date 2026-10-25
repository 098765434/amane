use amane::{
    App, Color, Column, Key, Parent, Rectangle, ScrollArea, Service, Text, TextInput, Window,
    children, close_window, modifiers,
};

// a small editor: many lines, selection, undo, the clipboard and an input method
struct Document {
    text: String,
}

impl Service for Document {
    fn new() -> Self {
        Self {
            text: String::new(),
        }
    }

    // only typing changes it
    fn listen() {}
}

fn main() {
    App::new()
        .app_id("amane-text-editor")
        .without_ipc()
        .normal_window("editor", editor)
        .run();
}

fn editor() -> Window {
    let document = Document::read();

    let lines = document.text.lines().count().max(1);
    let words = document.text.split_whitespace().count();

    Window::new()
        .title("text editor")
        .size(560.0, 400.0)
        .min_size(300.0, 200.0)
        .on_shortcut(|key| {
            let quit = modifiers().ctrl && key == Key::Character('q');

            if quit {
                close_window("editor");
            }

            quit
        })
        .child(
            Rectangle::new()
                .width(Parent)
                .height(Parent)
                .fill("#1e1e2e")
                .padding(16.0)
                .child(
                    Column::new(children![
                        Text::new(format!("{lines} lines, {words} words, Ctrl+Q quits"))
                            .size(13.0)
                            .color(Color::rgb(166, 173, 200)),
                        ScrollArea::new(
                            "document",
                            TextInput::new("document")
                                .multiline(true)
                                .focused()
                                .size(16.0)
                                .color(Color::WHITE)
                                .placeholder("Type here. Shift selects, Ctrl+Z undoes.")
                                .on_change(|text| Document::write().text = text),
                        )
                        .width(Parent)
                        .height(Parent),
                    ])
                    .gap(12.0)
                    .width(Parent)
                    .height(Parent),
                ),
        )
}
