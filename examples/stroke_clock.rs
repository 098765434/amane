use std::sync::OnceLock;
use std::time::Instant;

use amane::{
    App, Canvas, Cap, Circle, Easing, Join, Layer, LayerWindow, Path, Rectangle, Shape, Size,
    request_frame,
};

// each digit is one pen stroke in a 60 by 100 box
const DIGITS: [&str; 10] = [
    "M12 34 A18 18 0 0 1 48 34 L48 66 A18 18 0 0 1 12 66 Z",
    "M18 24 L34 12 L34 88",
    "M13 30 A17 17 0 0 1 47 30 C47 46 36 56 13 88 L48 88",
    "M14 24 C18 10 46 10 46 30 C46 42 38 48 28 48 C40 48 48 56 48 68 C48 92 16 94 12 78",
    "M40 88 L40 12 L12 62 L50 62",
    "M46 12 L17 12 L14 46 C20 40 26 38 31 38 C42 38 48 48 48 62 C48 82 36 88 28 88 C20 88 14 84 12 78",
    "M42 14 C30 26 14 44 14 64 A17 17 0 1 0 48 64 A17 17 0 1 0 14 64",
    "M12 12 L48 12 L24 88",
    "M30 48 C18 48 14 40 14 30 A16 18 0 0 1 46 30 C46 40 42 48 30 48 C16 48 12 58 12 68 A18 20 0 0 0 48 68 C48 58 44 48 30 48 Z",
    "M18 86 C30 74 46 56 46 36 A17 17 0 1 0 12 36 A17 17 0 1 0 46 36",
];

const GOLD: &str = "#E8C99A";
const SCALE: f32 = 1.2;

fn main() {
    App::new().window(view).run();
}

// a counter that ticks once a second, each changed digit erased along its line and the next drawn in
fn view() -> LayerWindow {
    static START: OnceLock<Instant> = OnceLock::new();

    let elapsed = START.get_or_init(Instant::now).elapsed().as_secs_f32();

    let count = elapsed as u32;
    let into = elapsed.fract();

    let now = digits(count);
    let before = digits(count.saturating_sub(1));

    let mut shapes: Vec<Box<dyn Shape>> = Vec::new();
    let mut changed = 0;

    for slot in 0..4 {
        let x = 20.0 + slot as f32 * 70.0 * SCALE + if slot >= 2 { 24.0 } else { 0.0 };

        if now[slot] == before[slot] || count == 0 {
            shapes.push(Box::new(digit(now[slot], x)));

            continue;
        }

        // later digits start a little after earlier ones, like the mockup
        let into = into - changed as f32 * 0.11;
        changed += 1;

        let erased = Easing::InOut.apply((into / 0.4).clamp(0.0, 1.0));
        let drawn = Easing::Out.apply(((into - 0.32) / 0.5).clamp(0.0, 1.0));

        shapes.push(Box::new(digit(before[slot], x).trim(erased, 1.0)));
        shapes.push(Box::new(digit(now[slot], x).trim(0.0, drawn)));
    }

    for y in [44.0, 82.0] {
        shapes.push(Box::new(
            Circle::new()
                .center(20.0 + 140.0 * SCALE + 12.0, y * SCALE)
                .radius(Size::Fixed(8.0 * SCALE))
                .fill(GOLD),
        ));
    }

    // it keeps changing every second, so it never rests
    request_frame();

    LayerWindow::new()
        .width(480.0)
        .height(200.0)
        .layer(Layer::Top)
        .child(
            Rectangle::new()
                .width(480.0)
                .height(200.0)
                .fill("#121110")
                .child(
                    Canvas::new()
                        .width(360.0 * SCALE)
                        .height(130.0 * SCALE)
                        .shapes(shapes),
                ),
        )
}

fn digits(count: u32) -> [usize; 4] {
    let n = count as usize % 10_000;

    [n / 1000, n / 100 % 10, n / 10 % 10, n % 10]
}

fn digit(value: usize, x: f32) -> Path {
    // parsed once and cloned after
    static PARSED: OnceLock<Vec<Path>> = OnceLock::new();

    let parsed = PARSED.get_or_init(|| DIGITS.iter().map(|d| Path::svg(d).unwrap()).collect());

    parsed[value]
        .clone()
        .place(x, 14.0 * SCALE, SCALE)
        .stroke(18.0 * SCALE, GOLD)
        .cap(Cap::Round)
        .join(Join::Round)
}
