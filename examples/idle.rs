use std::time::Duration;

use amane::{
    App, Color, Full, Idle, Layer, LayerWindow, Parent, Rectangle, Service, Text, Vertical,
};

const DIM: Duration = Duration::from_secs(5);
const LOCK: Duration = Duration::from_secs(10);

/*
 * leave the mouse and keyboard alone for 5 and 10 seconds, then move them;
 * hold off idle with: busctl --user call org.freedesktop.ScreenSaver
 * /org/freedesktop/ScreenSaver org.freedesktop.ScreenSaver Inhibit ss test why
 */
fn main() {
    Idle::set_levels(&[DIM, LOCK]);

    App::new().window(view).run();
}

fn view() -> LayerWindow {
    let idle = Idle::read();

    let state = if idle.crossed(LOCK) {
        "would lock"
    } else if idle.crossed(DIM) {
        "would dim"
    } else {
        "active"
    };

    let mut label = format!("{state}   inhibited {}", idle.inhibited());

    for (application, reason) in idle.inhibitors() {
        label.push_str(&format!("   {application}: {reason}"));
    }

    if !idle.running() {
        label.push_str("   (another program serves org.freedesktop.ScreenSaver)");
    }

    // an inhibit holds the dimming off, like a shell should
    let fill = if idle.is_idle() && !idle.inhibited() {
        Color::rgb(0x30, 0x30, 0x30)
    } else {
        Color::BLACK
    };

    LayerWindow::new()
        .width(Full)
        .height(30.0)
        .anchor_vertical(Vertical::Top)
        .layer(Layer::Top)
        .child(
            Rectangle::new()
                .width(Parent)
                .height(Parent)
                .fill(fill)
                .child(Text::new(&label).size(16.0).color(Color::WHITE)),
        )
}
