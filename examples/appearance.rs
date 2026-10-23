use amane::{
    App, Appearance, Color, Full, Layer, LayerWindow, Parent, Rectangle, Row, Scheme, Service,
    Text, Vertical, children,
};

/*
 * follows the desktop's light/dark setting, try it with
 * xdg-desktop-portal-gnome or -gtk running:
 * dconf write /org/gnome/desktop/interface/color-scheme "'prefer-dark'"
 * dconf reset /org/gnome/desktop/interface/color-scheme
 */
fn main() {
    App::new().window(view).run();
}

fn view() -> LayerWindow {
    let appearance = Appearance::read();

    let (background, foreground) = if appearance.prefers_dark() {
        (Color::BLACK, Color::WHITE)
    } else {
        (Color::WHITE, Color::BLACK)
    };

    let scheme = match appearance.scheme() {
        Scheme::Dark => "dark",
        Scheme::Light => "light",
        Scheme::NoPreference => "no preference",
    };

    let label = if appearance.present() {
        format!("scheme {scheme}")
    } else {
        String::from("no xdg-desktop-portal")
    };

    // a grey swatch when the portal backend knows no accent
    let accent = appearance.accent().unwrap_or(Color::rgb(0x80, 0x80, 0x80));

    LayerWindow::new()
        .width(Full)
        .height(30.0)
        .anchor_vertical(Vertical::Top)
        .layer(Layer::Top)
        .child(
            Rectangle::new()
                .width(Parent)
                .height(Parent)
                .fill(background)
                .child(
                    Row::new(children![
                        Rectangle::new().width(30.0).height(Parent).fill(accent),
                        Text::new(&label).size(16.0).color(foreground),
                    ])
                    .gap(8.0),
                ),
        )
}
