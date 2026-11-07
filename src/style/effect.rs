#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo, missing_docs)]

use std::path::PathBuf;

use crate::graphics::VALUE_ROWS;

// how many textures one pass can read
pub const INPUTS: usize = 4;

/// A chain of wgsl fragment shaders run over what a rectangle drew: each pass
/// reads the drawing or earlier passes' results and the last one is shown in
/// the drawing's place, see examples/effect.rs and docs/fork/effects.md.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Effect {
    pub(crate) passes: Vec<Pass>,
    pub(crate) values: Vec<[f32; 4]>,
    pub(crate) margin: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Pass {
    pub shader: PathBuf,
    pub inputs: Vec<Input>,
}

/// What a pass reads as input0, input1 and so on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    /// The rectangle's fill and children, drawn on their own.
    Content,

    /// What an earlier pass wrote, counted from 0.
    Pass(usize),

    /// What the window drew before the rectangle, under it and around it;
    /// inside a faded group or a clip, only what that group drew.
    Backdrop,

    /// An image file at its own size, blurred by `blur` pixels; clear until it's decoded.
    Image {
        /// The image file.
        path: PathBuf,
        /// How far it's blurred, in pixels of the image; 0 for not at all.
        blur: u32,
    },
}

impl Input {
    /// An image file at its own size, not blurred.
    pub fn image(path: impl Into<PathBuf>) -> Self {
        Self::Image {
            path: path.into(),
            blur: 0,
        }
    }

    /// Blurred once when it's decoded, which costs less than blurring in a pass every frame.
    pub fn blurred_image(path: impl Into<PathBuf>, blur: u32) -> Self {
        Self::Image {
            path: path.into(),
            blur,
        }
    }
}

impl Effect {
    // the image files the passes read, in the order they come
    pub(crate) fn images(&self) -> impl Iterator<Item = (&PathBuf, u32)> {
        self.passes
            .iter()
            .flat_map(|pass| &pass.inputs)
            .filter_map(|input| match input {
                Input::Image { path, blur } => Some((path, *blur)),
                _ => None,
            })
    }

    pub(crate) fn shaders(&self) -> impl Iterator<Item = &std::path::Path> {
        self.passes.iter().map(|pass| pass.shader.as_path())
    }

    pub(crate) fn reads_backdrop(&self) -> bool {
        self.passes
            .iter()
            .any(|pass| pass.inputs.contains(&Input::Backdrop))
    }

    /// An effect with no passes yet, which draws the rectangle as it is.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a pass running the wgsl file `shader` over `inputs`, read as input0 to input3.
    /// Panics on more than four inputs, or on an `Input::Pass` that isn't before this one.
    pub fn pass(mut self, shader: impl Into<PathBuf>, inputs: impl Into<Vec<Input>>) -> Self {
        let inputs = inputs.into();

        assert!(
            inputs.len() <= INPUTS,
            "an effect pass reads at most {INPUTS} inputs"
        );

        let index = self.passes.len();

        for input in &inputs {
            if let Input::Pass(earlier) = input {
                assert!(
                    *earlier < index,
                    "effect pass {index} can only read passes before it, not {earlier}"
                );
            }
        }

        self.passes.push(Pass {
            shader: shader.into(),
            inputs,
        });

        self
    }

    /// Numbers every pass reads as `values`, like Rectangle::shader_values.
    pub fn values(mut self, values: Vec<[f32; 4]>) -> Self {
        assert!(
            values.len() <= VALUE_ROWS,
            "an effect takes at most {VALUE_ROWS} rows of values"
        );

        self.values = values;

        self
    }

    /// Logical pixels the passes reach past the rectangle, for glows and blurs that spill out.
    pub fn margin(mut self, margin: f32) -> Self {
        self.margin = margin.max(0.0);

        self
    }
}
