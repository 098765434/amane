use amane::{
    App, Audio, Button, Color, Full, Layer, LayerWindow, Parent, Rectangle, Row, Scroll, Service,
    Text, Vertical, Widget, children,
};

const BUTTON: Color = Color::rgb(0x31, 0x32, 0x44);

// scroll over the bar to change the volume, click to mute, click an output to switch to it
fn main() {
    App::new().window(view).run();
}

fn view() -> LayerWindow {
    let audio = Audio::read();

    let label = if audio.muted() {
        String::from("muted")
    } else {
        format!("volume {}%", audio.volume())
    };

    let mut items: Vec<Box<dyn Widget>> = vec![Box::new(
        Rectangle::new()
            .width(200.0)
            .height(Parent)
            .fill(Color::BLACK)
            .on_click(clicked)
            .on_scroll(scrolled)
            .child(Text::new(label).size(20.0).color(Color::WHITE)),
    )];

    for output in audio.outputs() {
        let name = String::from(output.name());

        // the default one is marked, and the kind says what it is, like headphones
        let mark = if output.default() { "> " } else { "" };

        let label = format!("{mark}{} ({:?})", output.description(), output.kind());

        items.push(Box::new(
            Rectangle::new()
                .width(400.0)
                .height(Parent)
                .fill(BUTTON)
                .on_click(move |_| Audio::set_output(&name))
                .child(Text::new(label).size(16.0).color(Color::WHITE)),
        ));
    }

    LayerWindow::new()
        .width(Full)
        .height(30.0)
        .anchor_vertical(Vertical::Top)
        .layer(Layer::Top)
        .child(Row::new(items).gap(8.0))
}

fn clicked(_: Button) {
    Audio::toggle_mute();
}

// each wheel step moves the volume by 5
fn scrolled(scroll: Scroll) {
    let volume = i32::from(Audio::read().volume());

    let step = if scroll.y < 0.0 { 5 } else { -5 };

    let changed = (volume + step).clamp(0, 100);

    Audio::set_volume(changed as u8);
}
