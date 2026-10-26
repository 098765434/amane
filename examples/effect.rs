use amane::{
    App, Center, Color, Column, Effect, Input, LayerWindow, Parent, Rectangle, Text, Weight,
    children,
};

fn main() {
    App::new().window(view).run();
}

/*
 * run from the project folder, the shader paths are relative to it; the
 * drawing is blurred across, then down, and the glow pass lays the drawing
 * over its tinted blur; saving a shader shows the change straight away
 */
fn view() -> LayerWindow {
    let glow = |blur: f32, tint: [f32; 4]| {
        Effect::new()
            .pass("examples/shaders/blur_x.wgsl", [Input::Content])
            .pass("examples/shaders/blur_y.wgsl", [Input::Pass(0)])
            .pass(
                "examples/shaders/glow.wgsl",
                [Input::Content, Input::Pass(1)],
            )
            .values(vec![[blur, 0.0, 0.0, 0.0], tint])
            .margin(blur * 3.0)
    };

    LayerWindow::new().width(420.0).height(300.0).child(
        Rectangle::new()
            .width(Parent)
            .height(Parent)
            .fill("#1e1e2e")
            .align_child(Center, Center)
            .child(Column::new(children![
                Rectangle::new()
                    .width(360.0)
                    .height(110.0)
                    .align_child(Center, Center)
                    .effect(glow(8.0, [0.54, 0.71, 0.98, 1.0]))
                    .child(
                        Text::new("amane")
                            .size(72.0)
                            .weight(Weight::Bold)
                            .color(Color::WHITE)
                    ),
                Rectangle::new()
                    .width(360.0)
                    .height(110.0)
                    .align_child(Center, Center)
                    .child(
                        Rectangle::new()
                            .width(200.0)
                            .height(56.0)
                            .radius(28.0)
                            .fill("#f5c2e7")
                            .effect(glow(14.0, [0.95, 0.55, 0.8, 0.9]))
                    ),
            ])),
    )
}
