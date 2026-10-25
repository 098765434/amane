use amane::{
    App, Button, Center, Color, Column, ConstraintAdjustment, Gravity, Key, Point, Pointer, Popup,
    PopupAnchor, Rectangle, Service, Text, Window, children, close_window, current_window,
    minimize_window, open_popup, open_window, open_window_with, pointer_position, window_size,
    window_state,
};

// an app on its own, beside the shell, with the window features a regular app has
struct Editor {
    edits: u32,
    saved: u32,
    maximized: bool,
    documents: u32,
    menu_open: bool,
}

impl Service for Editor {
    fn new() -> Self {
        Self {
            edits: 0,
            saved: 0,
            maximized: false,
            documents: 0,
            menu_open: false,
        }
    }

    // only input changes it
    fn listen() {}
}

fn main() {
    App::new()
        .app_id("amane-example")
        .without_ipc()
        .normal_window("main", main_window)
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
        .on_key(shortcut)
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
                        label(format!("{width}x{height}, right click for a menu (c)")),
                        label(format!(
                            "focused {} maximized {} tiled {} decorated {}",
                            state.focused, state.maximized, state.tiled, state.server_decorated
                        )),
                        label(format!("{} edits, {} saved", editor.edits, editor.saved)),
                        button("edit (e)", |_| edit()),
                        button("save (s)", |_| save()),
                        button("toggle maximized (m)", |_| toggle_maximized()),
                        button("minimize", |_| minimize_window("main")),
                        button("new document (n)", |_| new_document()),
                    ])
                    .gap(8.0),
                ),
        )
}

fn shortcut(key: Key) {
    match key {
        Key::Character('e') => edit(),
        Key::Character('s') => save(),
        Key::Character('m') => toggle_maximized(),
        Key::Character('n') => new_document(),
        Key::Character('c') => open_menu(),
        _ => {}
    }
}

fn edit() {
    Editor::write().edits += 1;
}

fn save() {
    let mut editor = Editor::write();

    editor.saved = editor.edits;
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

    open_window_with(format!("document {number}"), move || document(number));
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

// one view, many windows, each knowing its own number
fn document(number: u32) -> Window {
    let me = current_window();

    Window::new()
        .title(&format!("document {number}"))
        .size(300.0, 160.0)
        .child(
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
