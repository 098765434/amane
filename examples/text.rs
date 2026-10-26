use amane::{
    App, Case, Color, Column, Decoration, LayerWindow, Parent, Rectangle, Size, Text, TextAlign,
    TextDirection, Weight, children,
};

fn main() {
    App::new().window(view).run();
}

fn view() -> LayerWindow {
    let long = "the quick brown fox jumps over the lazy dog, then keeps running far past the edge";

    // a fixed width, so alignment has room to show
    let boxed = |text: Text| {
        Rectangle::new()
            .width(Size::Fixed(240.0))
            .height(Size::Fixed(64.0))
            .child(text)
    };

    LayerWindow::new().width(260.0).height(640.0).child(
        Rectangle::new()
            .width(Parent)
            .height(Parent)
            .fill(Color::WHITE)
            .child(Column::new(children![
                Text::new("regular"),
                Text::new("bold").weight(Weight::Bold),
                Text::new("light, picked by number").weight(300),
                Text::new("650, between named weights").weight(650),
                Text::new("italic").italic(true),
                Text::new("tracked out").letter_spacing(2.0),
                Text::new("tracked in, kerned AVATAR").letter_spacing(-0.5),
                Text::new("upper case").case(Case::Upper),
                Text::new("capitalized words").case(Case::Capitalize),
                Text::new("underlined").decoration(Decoration::Underline),
                Text::new("struck through").decoration(Decoration::StrikeThrough),
                Text::new(long).elide(),
                Text::new(long).wrap().max_lines(2).elide(),
                Text::new(long).wrap().line_height(1.6).max_lines(2).elide(),
                boxed(Text::new(long).wrap().align(TextAlign::Center)),
                boxed(Text::new(long).wrap().align(TextAlign::Justify)),
                Text::new("مرحبا بالعالم").direction(TextDirection::RightToLeft),
                Text::new(long).wrap()
            ])),
    )
}
