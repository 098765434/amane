//! Reading, placing and trimming paths.

use vello::kurbo::{BezPath, ParamCurve, ParamCurveArclen};

use super::trim::LENGTH_ACCURACY;
use super::*;

// the digits of a clock drawn in one stroke each, in a 60 by 100 box
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

fn length(path: &BezierPath) -> f64 {
    path.to_kurbo()
        .segments()
        .map(|piece| piece.arclen(LENGTH_ACCURACY))
        .sum()
}

fn square() -> BezierPath {
    BezierPath::from_svg("M0 0 L10 0 L10 10 L0 10 Z").unwrap()
}

#[test]
fn every_digit_parses_and_ends_where_it_ends() {
    for digit in DIGITS {
        let path = BezierPath::from_svg(digit).unwrap();

        let end = path.to_kurbo().segments().last().unwrap().end();

        let parsed_end = BezPath::from_svg(digit)
            .unwrap()
            .segments()
            .last()
            .unwrap()
            .end();

        assert!((end - parsed_end).hypot() < 1e-3, "{digit}");
    }

    assert!(BezierPath::from_svg("M0 0 Q").is_none());
}

#[test]
fn a_trimmed_part_is_its_share_of_the_length() {
    // the closing side counts too
    let square = square();

    assert!((length(&square) - 40.0).abs() < 0.01);

    let half = square.trimmed(0.25, 0.75).unwrap();

    assert!((length(&half) - 20.0).abs() < 0.2);
    assert_eq!(half.segments[0], Segment::MoveTo(10.0, 0.0));

    for digit in DIGITS {
        let path = BezierPath::from_svg(digit).unwrap();

        let total = length(&path);

        let first = path.trimmed(0.0, 0.4).unwrap();
        let rest = path.trimmed(0.4, 1.0).unwrap();

        assert!(
            (length(&first) + length(&rest) - total).abs() < 0.5,
            "{digit}"
        );
    }
}

#[test]
fn the_whole_length_is_the_path_and_none_is_nothing() {
    let square = square();

    assert_eq!(square.trimmed(0.0, 1.0), Some(square.clone()));
    assert_eq!(square.trimmed(-1.0, 2.0), Some(square.clone()));
    assert_eq!(square.trimmed(0.5, 0.5), None);
    assert_eq!(square.trimmed(0.8, 0.2), None);
}

#[test]
fn placing_scales_then_moves() {
    let placed = square().placed(2.0, 5.0, 1.0);

    assert_eq!(placed.segments[2], Segment::LineTo(25.0, 21.0));
}

fn ring() -> BezierPath {
    let mut builder = PathBuilder::new();

    builder.arc(50.0, 50.0, 40.0, 90.0, 180.0);

    builder.finish().unwrap()
}

#[test]
fn an_arc_alone_is_known_as_one_through_placing_trimming_and_closing_a_circle() {
    let arc = ring().arc.unwrap();

    assert_eq!(
        (arc.center_x, arc.radius, arc.start, arc.sweep),
        (50.0, 40.0, 90.0, 180.0)
    );

    let placed = ring().placed(2.0, 1.0, 0.0).arc.unwrap();

    assert_eq!(
        (placed.center_x, placed.center_y, placed.radius),
        (101.0, 100.0, 80.0)
    );

    let part = ring().trimmed(0.25, 0.75).unwrap();
    let part_arc = part.arc.unwrap();

    assert_eq!((part_arc.start, part_arc.sweep), (135.0, 90.0));
    assert!((length(&part) - std::f64::consts::PI * 40.0 / 2.0).abs() < 0.1);
    assert_eq!(ring().trimmed(0.5, 0.5), None);

    let mut circle = PathBuilder::new();
    circle.arc(0.0, 0.0, 10.0, 0.0, 360.0);
    circle.close();
    assert!(circle.finish().unwrap().arc.unwrap().is_whole());

    let mut half = PathBuilder::new();
    half.arc(0.0, 0.0, 10.0, 0.0, 180.0);
    half.close();
    assert_eq!(half.finish().unwrap().arc, None);

    let mut joined = PathBuilder::new();
    joined.move_to(0.0, 0.0);
    joined.arc(0.0, 0.0, 10.0, 0.0, 90.0);
    assert_eq!(joined.finish().unwrap().arc, None);

    let mut followed = PathBuilder::new();
    followed.arc(0.0, 0.0, 10.0, 0.0, 90.0);
    followed.line_to(5.0, 5.0);
    assert_eq!(followed.finish().unwrap().arc, None);

    assert_eq!(square().arc, None);
}
