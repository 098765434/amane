use std::path::PathBuf;

use crate::graphics::VALUE_ROWS;

// how many textures one pass can read
pub const INPUTS: usize = 4;

/*
 * a chain of wgsl fragment shaders run over what a rectangle drew: each pass
 * reads the drawing or earlier passes' results and the last one is shown in
 * the drawing's place, see examples/effect.rs and docs/fork/effects.md
 */
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

// what a pass reads as input0, input1 and so on
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    // the rectangle's fill and children, drawn on their own
    Content,

    // what an earlier pass wrote, counted from 0
    Pass(usize),
}

impl Effect {
    pub fn new() -> Self {
        Self::default()
    }

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

    // numbers every pass reads as `values`, like Rectangle::shader_values
    pub fn values(mut self, values: Vec<[f32; 4]>) -> Self {
        assert!(
            values.len() <= VALUE_ROWS,
            "an effect takes at most {VALUE_ROWS} rows of values"
        );

        self.values = values;

        self
    }

    // logical pixels the passes reach past the rectangle, for glows and blurs that spill out
    pub fn margin(mut self, margin: f32) -> Self {
        self.margin = margin.max(0.0);

        self
    }
}
