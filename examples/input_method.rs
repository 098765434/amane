// this app as the seat's input method: a popup by the focused field in any app, which fills
// it through niri-secure-fill. Run it beside the shell (which isn't an input method unless its
// fill setting is on), then focus a text field in another app, say gtk4-demo's entries.
//
// nix develop -c cargo run --example input_method

use amane::{
    App, Center, Color, Column, FillError, FocusedField, Pointer, Popup, Rectangle, Service, Text,
    Window, children,
};
use zeroize::Zeroizing;

// what the last fill did, shown in the window
#[derive(Default)]
struct Status {
    last: String,
}

impl Service for Status {
    fn new() -> Self {
        Self::default()
    }

    fn listen() {}
}

fn main() {
    App::new()
        .app_id("amane-input-method-example")
        .without_ipc()
        .normal_window("main", status)
        .input_method(popup)
        .run();
}

fn status() -> Window {
    let field = FocusedField::read();
    let status = Status::read();

    let owner = match field.owner() {
        Some(owner) => format!(
            "pid {:?}, {}, app id \"{}\"{}{}",
            owner.pid,
            owner
                .exe
                .as_ref()
                .map_or(String::from("exe unknown"), |exe| exe.display().to_string()),
            owner.app_id,
            if owner.sandboxed { ", sandboxed" } else { "" },
            if owner.takes_text {
                ""
            } else {
                ", no text input"
            },
        ),
        None => String::from("owner not known yet"),
    };

    let lines = [
        format!(
            "input method: {}, fill: {}",
            if field.available() { "yes" } else { "no" },
            if field.can_fill() { "yes" } else { "no" },
        ),
        format!(
            "field: {}",
            if field.active() {
                format!(
                    "{:?}{}{}",
                    field.purpose(),
                    if field.is_password() {
                        " (password)"
                    } else {
                        ""
                    },
                    if field.own() { " (ours)" } else { "" },
                )
            } else {
                String::from("none")
            }
        ),
        owner,
        format!("last fill: {}", status.last),
    ];

    Window::new()
        .title("input method")
        .size(520.0, 160.0)
        .child(
            Rectangle::new()
                .width(amane::Parent)
                .height(amane::Parent)
                .fill("#1e1e2e")
                .align_child(Center, Center)
                .child(Column::new(
                    lines
                        .into_iter()
                        .map(|line| Box::new(Text::new(&line).color(Color::WHITE)) as _)
                        .collect(),
                )),
        )
}

// shown by any field that isn't ours; a click fills it
fn popup() -> Popup {
    let field = FocusedField::read();

    let shown = field.active() && !field.own();

    Popup::new("amane.example.input-method")
        .size(220, 76)
        .anchor_rect(0, 0, 1, 1)
        .visible(shown)
        .child(
            Rectangle::new()
                .width(amane::Parent)
                .height(amane::Parent)
                .fill("#313244")
                .radius(8.0)
                .align_child(Center, Center)
                .child(Column::new(children![
                    row("Fill \"hello\"", "hello"),
                    row("Fill \"Password123\"", "Password123"),
                ])),
        )
}

fn row(label: &str, text: &'static str) -> Rectangle {
    Rectangle::new()
        .width(200.0)
        .height(30.0)
        .fill("#45475a")
        .radius(6.0)
        .cursor(Pointer)
        .align_child(Center, Center)
        .on_click(move |_| {
            let generation = FocusedField::read().generation();

            FocusedField::fill(
                Zeroizing::new(String::from(text)),
                generation,
                |result: Result<(), FillError>| {
                    Status::write().last = match result {
                        Ok(()) => String::from("filled"),
                        Err(error) => format!("refused: {error:?}"),
                    };
                },
            );
        })
        .child(Text::new(label).color(Color::WHITE))
}
