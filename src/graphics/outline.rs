use swash::zeno::{Command, PathData};

use super::{BezierPath, PathBuilder};

pub struct Outline {
    path: PathBuilder,
}

impl Outline {
    pub fn new() -> Self {
        Self {
            path: PathBuilder::new(),
        }
    }

    // copies a letter's outline from swash, one drawing step at a time
    pub fn read(mut self, data: impl PathData) -> Self {
        for command in data.commands() {
            match command {
                Command::MoveTo(to) => self.path.move_to(to.x, to.y),
                Command::LineTo(to) => self.path.line_to(to.x, to.y),
                Command::QuadTo(control, to) => self.path.quad_to(control.x, control.y, to.x, to.y),
                Command::CurveTo(first, second, to) => self
                    .path
                    .cubic_to(first.x, first.y, second.x, second.y, to.x, to.y),
                Command::Close => self.path.close(),
            }
        }

        self
    }

    pub fn finish(self) -> Option<BezierPath> {
        self.path.finish()
    }
}
