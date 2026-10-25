use amane::{
    App, Button, Center, Clipboard, Color, Column, ConstraintAdjustment, DragData, FileDialog,
    Gravity, Key, Notify, Point, Pointer, Popup, PopupAnchor, PrimarySelection, Rectangle, Service,
    Text, TextInput, Window, children, close_window, current_window, minimize_window, open_popup,
    open_window, open_window_with, pointer_position, spawn_activated, window_size, window_state,
};

// an app on its own, beside the shell, with the window features a regular app has
struct Editor {
    edits: u32,
    saved: u32,
    maximized: bool,
    documents: u32,
    menu_open: bool,
    notes: String,
    released: String,
    pasted: String,
}

impl Service for Editor {
    fn new() -> Self {
        Self {
            edits: 0,
            saved: 0,
            maximized: false,
            documents: 0,
            menu_open: false,
            notes: String::new(),
            released: String::new(),
            pasted: String::new(),
        }
    }

    // only input changes it
    fn listen() {}
}

fn main() {
    // one copy runs: launching it again hands the arguments to this one
    App::new()
        .app_id("amane-example")
        .normal_window("main", main_window)
        .ipc("edits", |_| Editor::read().edits.to_string())
        .on_activate(|arguments| {
            for name in arguments {
                let name = name.clone();

                open_window_with(name.clone(), move || document(&name));
            }
        })
        .run();
}

fn main_window() -> Window {
    let editor = Editor::read();
    let state = window_state();
    let (width, height) = window_size();

    let unsaved = editor.edits != editor.saved;

    // an unfocused window greys out, like most apps do
    let background = if state.focused { "#313244" } else { "#24273a" };

    Window::new()
        .title(&format!("amane example{}", if unsaved { " •" } else { "" }))
        .size(520.0, 360.0)
        .min_size(360.0, 260.0)
        .maximized(editor.maximized)
        .on_shortcut(shortcut)
        .on_key_release(|key| Editor::write().released = format!("{key:?}"))
        .on_close(|| {
            let editor = Editor::read();

            // the first close only warns while there are unsaved edits
            if editor.edits != editor.saved {
                open_window("unsaved", unsaved_dialog);

                return false;
            }

            true
        })
        .child(
            Rectangle::new()
                .width(amane::Parent)
                .height(amane::Parent)
                .fill(background)
                .align_child(Center, Center)
                .on_click(|button| {
                    if button == Button::Right {
                        open_menu();
                    }
                })
                .child(
                    Column::new(children![
                        label(format!("{width}x{height}, right click for a menu (Ctrl+K)")),
                        label(format!(
                            "focused {} maximized {} tiled {} decorated {}",
                            state.focused, state.maximized, state.tiled, state.server_decorated
                        )),
                        label(format!("{} edits, {} saved", editor.edits, editor.saved)),
                        label(format!(
                            "notes: {:?}, last key up: {}",
                            editor.notes, editor.released
                        )),
                        TextInput::new("notes")
                            .width(240.0)
                            .placeholder("type, hold Backspace")
                            .focused()
                            .on_change(|text| Editor::write().notes = text),
                        label(format!(
                            "pasted: {:?} (Ctrl+V, Ctrl+B for primary)",
                            editor.pasted
                        )),
                        Rectangle::new()
                            .width(240.0)
                            .height(30.0)
                            .fill("#45475a")
                            .radius(6.0)
                            .cursor(amane::Grab)
                            .align_child(Center, Center)
                            .drag_out(|| DragData::Text(Editor::read().notes.clone()))
                            .on_text_drop(|text| Editor::write().pasted = text)
                            .child(label(String::from("drag the notes out, or text in"))),
                        button("edit (Ctrl+E)", |_| edit()),
                        button("save (Ctrl+S)", |_| save()),
                        button("toggle maximized (Ctrl+M)", |_| toggle_maximized()),
                        button("minimize", |_| minimize_window("main")),
                        button("new document (Ctrl+N)", |_| new_document()),
                    ])
                    .gap(8.0),
                ),
        )
}

// runs before the text field, so these work while typing
fn shortcut(key: Key) -> bool {
    if !amane::modifiers().ctrl {
        return false;
    }

    match key {
        Key::Character('e') => edit(),
        Key::Character('s') => save(),
        Key::Character('m') => toggle_maximized(),
        Key::Character('n') => new_document(),
        Key::Character('k') => open_menu(),
        Key::Character('o') => FileDialog::open("Open a file")
            .filter("Text", &["*.txt", "*.md"])
            .filter("Everything", &["*"])
            .show(|files| Editor::write().pasted = format!("{files:?}")),
        Key::Character('l') => spawn_activated(&format!("{} launched-again", program())),
        Key::Character('c') => Clipboard::copy(Editor::read().notes.clone()),
        Key::Character('v') => Clipboard::paste(|text| Editor::write().pasted = text),
        Key::Character('p') => PrimarySelection::copy(Editor::read().notes.clone()),
        Key::Character('b') => PrimarySelection::paste(|text| Editor::write().pasted = text),
        _ => return false,
    }

    true
}

fn edit() {
    Editor::write().edits += 1;
}

fn save() {
    let mut editor = Editor::write();

    editor.saved = editor.edits;

    Notify::new("Saved")
        .body(&format!("{} edits", editor.edits))
        .icon("document-save")
        .send();
}

fn toggle_maximized() {
    let mut editor = Editor::write();

    editor.maximized = !editor.maximized;
}

fn new_document() {
    let number = {
        let mut editor = Editor::write();

        editor.documents += 1;

        editor.documents
    };

    let name = format!("document {number}");

    open_window_with(name.clone(), move || document(&name));
}

// this program, to launch it again
fn program() -> String {
    std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_default()
}

// a context menu where the pointer is, over the main window
fn open_menu() {
    let at = pointer_position();

    Editor::write().menu_open = true;

    open_popup(move || menu(at));
}

fn menu(at: Point) -> Popup {
    let open = Editor::read().menu_open;

    Popup::new("menu")
        .size(200, 120)
        .anchor_rect(at.x as i32, at.y as i32, 1, 1)
        .parent_window("main")
        .anchor(PopupAnchor::TopLeft)
        .gravity(Gravity::BottomRight)
        .constraint_adjustment(ConstraintAdjustment::FLIP_X | ConstraintAdjustment::FLIP_Y)
        .grab()
        .visible(open)
        .on_dismiss(close_menu)
        .on_key(|key| {
            if key == Key::Escape {
                close_menu();
            }
        })
        .child(
            Rectangle::new()
                .width(amane::Parent)
                .height(amane::Parent)
                .fill("#1e1e2e")
                .radius(8.0)
                .align_child(Center, Center)
                .child(
                    Column::new(children![
                        menu_item("edit", edit),
                        menu_item("new document", new_document),
                        menu_item("minimize", || minimize_window("main")),
                    ])
                    .gap(4.0),
                ),
        )
}

fn menu_item(text: &str, action: impl Fn() + 'static) -> Rectangle {
    Rectangle::new()
        .width(180.0)
        .height(30.0)
        .fill("#313244")
        .radius(6.0)
        .cursor(Pointer)
        .align_child(Center, Center)
        .on_click(move |_| {
            close_menu();

            action();
        })
        .child(label(String::from(text)))
}

fn close_menu() {
    Editor::write().menu_open = false;
}

// one view, many windows, each knowing its own name
fn document(name: &str) -> Window {
    let me = current_window();

    Window::new().title(name).size(300.0, 160.0).child(
        Rectangle::new()
            .width(amane::Parent)
            .height(amane::Parent)
            .fill("#1e1e2e")
            .align_child(Center, Center)
            .child(
                Column::new(children![
                    label(format!("I am {me}")),
                    button("close", move |_| close_window(&me)),
                ])
                .gap(8.0),
            ),
    )
}

// over the main window, keeping the user out of it until answered
fn unsaved_dialog() -> Window {
    Window::new()
        .title("unsaved changes")
        .size(300.0, 140.0)
        .resizable(false)
        .parent("main")
        .modal(true)
        .child(
            Rectangle::new()
                .width(amane::Parent)
                .height(amane::Parent)
                .fill("#45475a")
                .align_child(Center, Center)
                .child(
                    Column::new(children![
                        label(String::from("there are unsaved edits")),
                        button("save and close", |_| {
                            save();

                            close_window("unsaved");
                            close_window("main");
                        }),
                        button("keep editing", |_| close_window("unsaved")),
                    ])
                    .gap(8.0),
                ),
        )
}

fn label(text: String) -> Text {
    Text::new(text).size(15.0).color(Color::WHITE)
}

fn button(text: &str, on_click: impl Fn(amane::Button) + 'static) -> Rectangle {
    Rectangle::new()
        .width(240.0)
        .height(30.0)
        .fill("#585b70")
        .radius(6.0)
        .cursor(Pointer)
        .align_child(Center, Center)
        .on_click(on_click)
        .child(label(String::from(text)))
}
