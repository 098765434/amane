// how a line turns a corner
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Join {
    // the two edges carry on until they meet, so square corners stay square
    #[default]
    Miter,

    // a circle around the corner point, like a pen would draw
    Round,

    // the corner cut straight across
    Bevel,
}
