/*
 * a vibrant window's drawing split three ways for niri-vibrancy: its
 * translucent white content goes on a layer the compositor adds to what's
 * behind it (plus-lighter), its translucent black content on one it takes
 * away from it (plus-darker), and everything else (colour, other greys,
 * images, shadows, effects) stays on the window; groups and clips are kept around whatever of theirs
 * goes on each layer;
 *
 * the compositor subtracts the darker layer as it is drawn, so that layer
 * holds how far each pixel darkens: a grey c at alpha a goes on it as
 * 1 - c at alpha a, which darkens by a(1 - c), Apple's plus-darker
 */

use super::Color;
use super::renderer::Command;

// how far a channel may be from pure white or black and still count as it
const NEAR: u8 = 7;

#[derive(Default)]
pub struct Layers {
    pub window: Vec<Command>,
    pub lighter: Vec<Command>,
    pub darker: Vec<Command>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Layer {
    Window,
    Lighter,
    Darker,
}

pub fn split(commands: Vec<Command>) -> Layers {
    let mut layers = Layers::default();

    for command in commands {
        match command {
            Command::Group { commands, opacity } => {
                let inner = split(commands);

                let wrap = |commands: Vec<Command>| Command::Group { commands, opacity };

                push_wrapped(&mut layers, inner, wrap);
            }

            Command::Clip {
                area,
                radius,
                transform,
                commands,
            } => {
                let inner = split(commands);

                let wrap = |commands: Vec<Command>| Command::Clip {
                    area,
                    radius,
                    transform,
                    commands,
                };

                push_wrapped(&mut layers, inner, wrap);
            }

            command => match layer_of(&command) {
                Layer::Window => layers.window.push(command),
                Layer::Lighter => layers.lighter.push(command),
                Layer::Darker => layers.darker.push(inverted(command)),
            },
        }
    }

    layers
}

// what each layer got of a group or clip goes on it wrapped the same way
fn push_wrapped(layers: &mut Layers, inner: Layers, wrap: impl Fn(Vec<Command>) -> Command) {
    for (into, commands) in [
        (&mut layers.window, inner.window),
        (&mut layers.lighter, inner.lighter),
        (&mut layers.darker, inner.darker),
    ] {
        if !commands.is_empty() {
            into.push(wrap(commands));
        }
    }
}

// only flat shapes and letters in translucent white or black are vibrant: Apple's
// vibrant labels and fills. A material's tint or an opaque background is grey too,
// but it is painted, not blended
fn layer_of(command: &Command) -> Layer {
    let color = match command {
        Command::Glyph { color, .. }
        | Command::Rectangle { color, .. }
        | Command::Border { color, .. }
        | Command::Fill { color, .. }
        | Command::Stroke { color, .. } => *color,

        _ => return Layer::Window,
    };

    grey_layer(color)
}

// a darker-layer command with its colour turned into how far it darkens
fn inverted(mut command: Command) -> Command {
    if let Command::Glyph { color, .. }
    | Command::Rectangle { color, .. }
    | Command::Border { color, .. }
    | Command::Fill { color, .. }
    | Command::Stroke { color, .. } = &mut command
    {
        *color = Color::rgba(
            255 - color.red(),
            255 - color.green(),
            255 - color.blue(),
            color.alpha(),
        );
    }

    command
}

// whether `faded` goes on the same layer as `color`, so fading by it looks the same
pub(crate) fn same_layer(color: Color, faded: Color) -> bool {
    grey_layer(color) == grey_layer(faded)
}

fn grey_layer(color: Color) -> Layer {
    let channels = [color.red(), color.green(), color.blue()];

    let lowest = channels.iter().copied().min().unwrap_or(0);
    let highest = channels.iter().copied().max().unwrap_or(0);

    if color.alpha() == 0 || color.alpha() == 255 {
        Layer::Window
    } else if lowest >= 255 - NEAR {
        Layer::Lighter
    } else if highest <= NEAR {
        Layer::Darker
    } else {
        Layer::Window
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::{Area, Corners, Transform};

    fn rectangle(color: Color) -> Command {
        Command::Rectangle {
            area: Area {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            radius: Corners::from(0.0),
            transform: Transform::IDENTITY,
            color,
        }
    }

    #[test]
    fn greys_go_on_the_layer_their_lightness_picks() {
        let layers = split(vec![
            rectangle(Color::rgba(255, 255, 255, 140)),
            rectangle(Color::rgba(0, 0, 0, 128)),
            rectangle(Color::rgb(10, 132, 255)),
        ]);

        assert_eq!(layers.lighter.len(), 1);
        assert_eq!(layers.darker.len(), 1);
        assert_eq!(layers.window.len(), 1);
    }

    #[test]
    fn tints_and_opaque_greys_stay_on_the_window() {
        for color in [
            Color::rgba(20, 20, 20, 199),
            Color::rgba(246, 246, 246, 204),
            Color::rgb(255, 255, 255),
            Color::rgb(0, 0, 0),
        ] {
            assert_eq!(grey_layer(color), Layer::Window, "{color:?}");
        }
    }

    #[test]
    fn the_darker_layer_holds_how_far_it_darkens() {
        let layers = split(vec![rectangle(Color::rgba(0, 0, 0, 128))]);

        let [Command::Rectangle { color, .. }] = layers.darker.as_slice() else {
            panic!("the darker layer has the rectangle");
        };
        assert_eq!(*color, Color::rgba(255, 255, 255, 128));
    }

    #[test]
    fn a_transparent_shape_stays_on_the_window() {
        assert_eq!(grey_layer(Color::TRANSPARENT), Layer::Window);
    }

    #[test]
    fn groups_are_kept_around_each_layers_share() {
        let layers = split(vec![Command::Group {
            commands: vec![
                rectangle(Color::rgba(255, 255, 255, 140)),
                rectangle(Color::rgb(200, 30, 30)),
            ],
            opacity: 0.5,
        }]);

        let [Command::Group { commands, opacity }] = layers.lighter.as_slice() else {
            panic!("the lighter layer has the group");
        };
        assert_eq!((commands.len(), *opacity), (1, 0.5));
        assert_eq!(layers.window.len(), 1);
        assert!(layers.darker.is_empty());
    }
}
