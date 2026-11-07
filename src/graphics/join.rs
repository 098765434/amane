#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo, missing_docs)]

/// How a line turns a corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Join {
    /// The two edges carry on until they meet, so square corners stay square.
    #[default]
    Miter,

    /// A circle around the corner point, like a pen would draw.
    Round,

    /// The corner cut straight across.
    Bevel,
}
