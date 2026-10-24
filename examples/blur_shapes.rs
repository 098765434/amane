use amane::{
    App, BlurShape, Color, Full, Gradient, Layer, LayerWindow, Parent, Rectangle, Row, children,
};

fn main() {
    // `--wallpaper` draws only the stripes, to put behind another program's glass
    if std::env::args().any(|arg| arg == "--wallpaper") {
        App::new().window(wallpaper).run();
    } else {
        App::new().window(wallpaper).window(view).run();
    }
}

const CARD: f32 = 160.0;
const RADIUS: f32 = 28.0;
const GAP: f32 = 20.0;

// two glass cards with the compositor's blur in exactly their shape, the second half faded
fn view() -> LayerWindow {
    let tint = Color::rgba(20, 24, 32, 120);

    LayerWindow::new()
        .width(CARD * 2.0 + GAP * 3.0)
        .height(CARD + GAP * 2.0)
        .click_through()
        .blur_shapes(vec![
            BlurShape::new(GAP, GAP, CARD, CARD, RADIUS),
            BlurShape::new(GAP * 2.0 + CARD, GAP, CARD, CARD, RADIUS).alpha(0.5),
        ])
        .child(
            Rectangle::new()
                .width(Parent)
                .height(Parent)
                .fill(Color::TRANSPARENT)
                .padding(GAP)
                .child(Row::new(children![
                Rectangle::new()
                    .width(CARD)
                    .height(CARD)
                    .radius(RADIUS)
                    .fill(tint),
                Rectangle::new()
                    .width(CARD)
                    .height(CARD)
                    .radius(RADIUS)
                    .fill(tint)
                    .opacity(0.5),
            ])
            .gap(GAP)),
        )
}

// sharp stripes under everything, so the blur has something to soften
fn wallpaper() -> LayerWindow {
    let stripe = |from: &'static str, to: &'static str| {
        Rectangle::new()
            .width(60.0)
            .height(Parent)
            .fill(Gradient::linear(0.0, [(0.0, from), (1.0, to)]))
    };

    LayerWindow::new()
        .width(Full)
        .height(Full)
        .layer(Layer::Background)
        .click_through()
        .child(
            Row::new(children![
                stripe("#f38ba8", "#1e1e2e"),
                stripe("#f9e2af", "#89b4fa"),
                stripe("#a6e3a1", "#f5c2e7"),
                stripe("#89b4fa", "#fab387"),
                stripe("#f5c2e7", "#a6e3a1"),
                stripe("#fab387", "#f38ba8"),
                stripe("#1e1e2e", "#f9e2af"),
                stripe("#f38ba8", "#89b4fa"),
                stripe("#f9e2af", "#1e1e2e"),
                stripe("#a6e3a1", "#fab387"),
                stripe("#89b4fa", "#f5c2e7"),
                stripe("#f5c2e7", "#f9e2af"),
                stripe("#fab387", "#a6e3a1"),
                stripe("#1e1e2e", "#f38ba8"),
            ])
            .width(Parent)
            .height(Parent),
        )
}
