use std::path::PathBuf;

use crate::{Button, Cursor, Scroll};

use super::Target;

#[derive(Default)]
pub struct Pointer {
    // the hit areas of the last drawn frame
    targets: Vec<Target>,

    x: f32,
    y: f32,

    // positions in targets, which stay the same across redraws of an unchanged layout
    hovered: Vec<usize>,
    pressed: Option<usize>,
    dragged: Option<usize>,
}

impl Pointer {
    // the hit areas of a new frame replace the last frame's
    pub fn set_targets(&mut self, targets: Vec<Target>) {
        self.targets = targets;
    }

    // the innermost target under the pointer that has the handler asked for
    fn find_topmost(&self, has_handler: fn(&Target) -> bool) -> Option<usize> {
        let mut topmost = None;

        // children are collected after their parents, so the last match is the innermost
        for (index, target) in self.targets.iter().enumerate() {
            if target.contains(self.x, self.y) && has_handler(target) {
                topmost = Some(index);
            }
        }

        topmost
    }

    // hover

    pub fn move_to(&mut self, x: f32, y: f32) -> bool {
        self.x = x;
        self.y = y;

        let mut inside = Vec::new();

        for (index, target) in self.targets.iter().enumerate() {
            if target.handlers.hover.is_some() && target.contains(x, y) {
                inside.push(index);
            }
        }

        self.update_hovered(inside)
    }

    pub fn leave(&mut self) -> bool {
        // a button let go outside the window is not a click
        self.pressed = None;

        // the release will not reach this window, so the drag ends here
        let released = self.end_drag();

        self.update_hovered(Vec::new()) || released
    }

    // tells the targets the pointer entered or left, and returns whether any were told
    fn update_hovered(&mut self, inside: Vec<usize>) -> bool {
        let mut changed = false;

        for &index in &self.hovered {
            if !inside.contains(&index) {
                changed |= self.notify(index, false);
            }
        }

        for &index in &inside {
            if !self.hovered.contains(&index) {
                changed |= self.notify(index, true);
            }
        }

        self.hovered = inside;

        changed
    }

    fn notify(&self, index: usize, inside: bool) -> bool {
        // a redraw since the pointer entered may have removed this target
        let Some(target) = self.targets.get(index) else {
            return false;
        };

        let Some(hover) = &target.handlers.hover else {
            return false;
        };

        hover(inside);

        true
    }

    // motion

    // tells the innermost target under the pointer where the pointer is on it
    pub fn report_motion(&self) -> bool {
        let Some(index) = self.find_topmost(|target| target.handlers.motion.is_some()) else {
            return false;
        };

        let target = &self.targets[index];

        let Some(motion) = &target.handlers.motion else {
            return false;
        };

        let point = target.point(self.x, self.y);

        motion(point);

        true
    }

    // the look asked for by the innermost target under the pointer
    pub fn cursor(&self) -> Cursor {
        let Some(index) = self.find_topmost(|target| target.handlers.cursor.is_some()) else {
            return Cursor::Default;
        };

        self.targets[index]
            .handlers
            .cursor
            .unwrap_or(Cursor::Default)
    }

    // click

    pub fn press(&mut self) {
        self.pressed = self.find_topmost(|target| target.handlers.click.is_some());
    }

    // a click only counts when the button is let go over the widget it was pressed on
    pub fn release(&mut self, button: Button) -> bool {
        let Some(pressed) = self.pressed.take() else {
            return false;
        };

        let released = self.find_topmost(|target| target.handlers.click.is_some());

        if released != Some(pressed) {
            return false;
        }

        let Some(click) = &self.targets[pressed].handlers.click else {
            return false;
        };

        click(button);

        true
    }

    // drag

    pub fn start_drag(&mut self) -> bool {
        self.dragged = self.find_topmost(|target| target.handlers.drag.is_some());

        self.drag()
    }

    // tells the dragged target the button was let go, and returns whether it listened
    pub fn stop_drag(&mut self) -> bool {
        self.end_drag()
    }

    fn end_drag(&mut self) -> bool {
        let Some(index) = self.dragged.take() else {
            return false;
        };

        // a redraw since the press may have removed this target
        let Some(target) = self.targets.get(index) else {
            return false;
        };

        let Some(release) = &target.handlers.release else {
            return false;
        };

        release(target.point(self.x, self.y));

        true
    }

    // keeps reporting to the target pressed on, even after the pointer leaves it
    pub fn drag(&self) -> bool {
        let Some(index) = self.dragged else {
            return false;
        };

        // a redraw since the press may have removed this target
        let Some(target) = self.targets.get(index) else {
            return false;
        };

        let Some(drag) = &target.handlers.drag else {
            return false;
        };

        let point = target.point(self.x, self.y);

        drag(point);

        true
    }

    // scroll

    pub fn scroll(&self, scroll: Scroll) -> bool {
        let Some(topmost) = self.find_topmost(|target| target.handlers.scroll.is_some()) else {
            return false;
        };

        let Some(handler) = &self.targets[topmost].handlers.scroll else {
            return false;
        };

        handler(scroll);

        true
    }

    // drop

    // returns whether a handler ran
    pub fn drop_files(&mut self, x: f32, y: f32, files: Vec<PathBuf>) -> bool {
        self.x = x;
        self.y = y;

        let Some(index) = self.find_topmost(|target| target.handlers.drop.is_some()) else {
            return false;
        };

        let Some(drop) = &self.targets[index].handlers.drop else {
            return false;
        };

        drop(files);

        true
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::Point;
    use crate::graphics::Area;
    use crate::input::{Handlers, Target};

    #[test]
    fn a_drag_ends_with_one_release_where_the_pointer_is() {
        let released = Rc::new(RefCell::new(Vec::new()));
        let seen = released.clone();
        let handlers = Handlers {
            drag: Some(Rc::new(|_| {})),
            release: Some(Rc::new(move |point: Point| seen.borrow_mut().push(point))),
            ..Handlers::default()
        };
        let mut pointer = Pointer::default();
        pointer.set_targets(vec![Target::new(
            Area::new(10.0, 10.0, 50.0, 50.0),
            handlers,
        )]);

        pointer.move_to(20.0, 20.0);
        assert!(pointer.start_drag());
        pointer.move_to(100.0, 100.0);

        assert!(pointer.stop_drag());
        assert_eq!(released.borrow().as_slice(), [Point { x: 90.0, y: 90.0 }]);

        // the drag has ended, so a second release goes nowhere
        assert!(!pointer.stop_drag());
        assert_eq!(released.borrow().len(), 1);
    }

    #[test]
    fn leaving_the_window_ends_the_drag_too() {
        let count = Rc::new(RefCell::new(0));
        let seen = count.clone();
        let handlers = Handlers {
            drag: Some(Rc::new(|_| {})),
            release: Some(Rc::new(move |_| *seen.borrow_mut() += 1)),
            ..Handlers::default()
        };
        let mut pointer = Pointer::default();
        pointer.set_targets(vec![Target::new(Area::new(0.0, 0.0, 10.0, 10.0), handlers)]);

        pointer.move_to(5.0, 5.0);
        assert!(pointer.start_drag());
        assert!(pointer.leave());
        assert_eq!(*count.borrow(), 1);
        assert!(!pointer.stop_drag());
    }
}
