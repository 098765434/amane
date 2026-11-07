//! Which of the fork's shapes the quads draw, and how they are taken apart.

use crate::graphics::{BezierPath, Color, Gradient, PathBuilder};

use super::super::quads::Paint;
use super::shapes::*;
use super::{route, *};

fn is_quads(command: &Command) -> bool {
    matches!(route(command), Route::Quads)
}

#[test]
fn video_frames_always_go_to_the_quads() {
    use std::sync::Arc;

    use crate::video::{Colors, Frame, Picture};

    let video = Command::Video {
        picture: Picture {
            feed: 1,
            number: 1,
            frame: Arc::new(Frame {
                width: 2,
                height: 2,
                y: vec![0; 4],
                uv: vec![128; 2],
                colors: Colors::Bt709,
                full_range: false,
            }),
        },
        transform: Transform::from_scale(1.5, 1.5),
        area: Area::new(0.0, 0.0, 3.0, 3.0),
        radius: Corners::from(1.0),
        clip_transform: Transform::IDENTITY,
    };

    assert!(is_quads(&video));
}

fn bubble() -> BezierPath {
    path(|p| {
        p.move_to(10.0, 0.0);
        p.line_to(40.0, 0.0);
        p.cubic_to(50.0, 0.0, 50.0, 20.0, 40.0, 20.0);
        p.cubic_to(44.0, 26.0, 48.0, 27.0, 50.0, 27.0);
        p.line_to(10.0, 20.0);
        p.cubic_to(0.0, 20.0, 0.0, 0.0, 10.0, 0.0);
        p.close();
    })
}

#[test]
fn small_curved_outlines_are_quads_and_big_ones_vello() {
    let corners = outline_corners(&bubble(), 2.0).unwrap();
    assert!(corners.len() > 8 && corners.len() <= OUTLINE_CORNERS);
    assert_ne!(corners.first(), corners.last());

    let fill = |path: BezierPath| Command::Fill {
        path,
        transform: Transform::from_scale(2.0, 2.0),
        color: Color::BLACK,
    };
    assert!(is_quads(&fill(bubble())));

    // two outlines in one path, or a thousand corners, stay with vello
    let two = path(|p| {
        p.move_to(0.0, 0.0);
        p.line_to(10.0, 0.0);
        p.line_to(5.0, 8.0);
        p.close();
        p.move_to(20.0, 0.0);
        p.line_to(30.0, 0.0);
        p.line_to(25.0, 8.0);
        p.close();
    });
    assert!(!is_quads(&fill(two)));
    let jagged = path(|p| {
        p.move_to(0.0, 0.0);
        for i in 0..1000 {
            p.line_to(i as f32, (i % 2) as f32);
        }
        p.close();
    });
    assert!(!is_quads(&fill(jagged)));
}

#[test]
fn two_stop_linear_gradients_are_quads_and_the_rest_vello() {
    let gradient = |gradient: Gradient| Command::Gradient {
        path: bubble(),
        area: Area::new(0.0, 0.0, 50.0, 27.0),
        transform: Transform::IDENTITY,
        gradient,
    };

    assert!(is_quads(&gradient(Gradient::linear(
        180.0,
        [(0.0, Color::WHITE), (1.0, Color::BLACK)]
    ))));
    assert!(!is_quads(&gradient(Gradient::linear(
        180.0,
        [(0.0, Color::WHITE), (0.5, Color::BLUE), (1.0, Color::BLACK)]
    ))));
    assert!(!is_quads(&gradient(Gradient::radial([
        (0.0, Color::WHITE),
        (1.0, Color::BLACK)
    ]))));
}

#[test]
fn a_gradient_runs_as_vello_lays_it() {
    // 180 degrees runs top to bottom over the area's height
    let paint = two_stops(
        &Gradient::linear(180.0, [(0.0, Color::WHITE), (1.0, Color::BLACK)]),
        Area::new(0.0, 10.0, 40.0, 20.0),
    )
    .unwrap();

    let expected = Paint::linear(Color::WHITE, Color::BLACK, (20.0, 10.0), (20.0, 30.0));
    for (got, want) in paint.line().iter().zip(expected.line()) {
        assert!((got - want).abs() < 0.001, "{paint:?} isn't {expected:?}");
    }
}

fn path(build: impl FnOnce(&mut PathBuilder)) -> BezierPath {
    let mut builder = PathBuilder::new();

    build(&mut builder);

    builder.finish().unwrap()
}

#[test]
fn straight_lines_are_polylines_and_curves_are_not() {
    let zigzag = path(|p| {
        p.move_to(0.0, 0.0);
        p.line_to(10.0, 5.0);
        p.line_to(20.0, 0.0);
        p.move_to(30.0, 30.0);
        p.line_to(40.0, 40.0);
        p.move_to(50.0, 50.0);
    });

    assert_eq!(
        polylines(&zigzag),
        Some(vec![
            vec![(0.0, 0.0), (10.0, 5.0), (20.0, 0.0)],
            vec![(30.0, 30.0), (40.0, 40.0)],
        ])
    );

    let triangle = path(|p| {
        p.move_to(0.0, 0.0);
        p.line_to(10.0, 0.0);
        p.line_to(5.0, 8.0);
        p.close();
    });

    assert_eq!(polylines(&triangle).unwrap()[0].len(), 4);

    let curved = path(|p| {
        p.move_to(0.0, 0.0);
        p.quad_to(5.0, 5.0, 10.0, 0.0);
    });

    assert_eq!(polylines(&curved), None);
    assert_eq!(polylines(&path(|p| p.move_to(1.0, 1.0))), None);
}

#[test]
fn the_area_under_a_line_is_known_and_other_outlines_are_not() {
    let under = path(|p| {
        p.move_to(0.0, 20.0);
        p.line_to(0.0, 10.0);
        p.line_to(10.0, 5.0);
        p.line_to(20.0, 12.0);
        p.line_to(30.0, 20.0);
        p.close();
    });

    assert_eq!(
        under_polyline(&under),
        Some((
            vec![
                (0.0, 20.0),
                (0.0, 10.0),
                (10.0, 5.0),
                (20.0, 12.0),
                (30.0, 20.0)
            ],
            20.0
        ))
    );

    // going back left, or below the baseline, or not closed, or not ending on it
    let back = path(|p| {
        p.move_to(0.0, 20.0);
        p.line_to(10.0, 5.0);
        p.line_to(5.0, 8.0);
        p.line_to(30.0, 20.0);
        p.close();
    });
    assert_eq!(under_polyline(&back), None);

    let below = path(|p| {
        p.move_to(0.0, 20.0);
        p.line_to(10.0, 25.0);
        p.line_to(30.0, 20.0);
        p.close();
    });
    assert_eq!(under_polyline(&below), None);

    let open = path(|p| {
        p.move_to(0.0, 20.0);
        p.line_to(10.0, 5.0);
        p.line_to(30.0, 20.0);
    });
    assert_eq!(under_polyline(&open), None);

    let hanging = path(|p| {
        p.move_to(0.0, 20.0);
        p.line_to(10.0, 5.0);
        p.line_to(30.0, 15.0);
        p.close();
    });
    assert_eq!(under_polyline(&hanging), None);
}
