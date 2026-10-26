use std::path::PathBuf;

use crate::{Button, Cursor, DragData, Scroll};

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

    // the target files from another program are held over
    files_over: Option<usize>,

    // the target the left button went down on that can be dragged out, and where
    drag_out: Option<(usize, f32, f32)>,
}

// how far the pointer moves with the button down before it counts as dragging out
const DRAG_OUT_DISTANCE: f32 = 8.0;

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
        self.drag_out = None;

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

    /*
     * tells the innermost target under the pointer where the pointer is on
     * it; true when that should redraw, false when it was only tracked
     */
    pub fn report_motion(&self) -> bool {
        let Some(index) = self.find_topmost(|target| {
            target.handlers.motion.is_some() || target.handlers.track.is_some()
        }) else {
            return false;
        };

        let target = &self.targets[index];

        let point = target.point(self.x, self.y);

        if let Some(track) = &target.handlers.track {
            track(point);
        }

        let Some(motion) = &target.handlers.motion else {
            return false;
        };

        motion(point);

        true
    }

    // the look asked for by the innermost target under the pointer; while a
    // drag is in flight, by the target being dragged, wherever the pointer is
    // (a resize handle doesn't follow the pointer, and the cursor shouldn't
    // flip back and forth as the pointer runs off it)
    pub fn cursor(&self) -> Cursor {
        let dragged = self
            .dragged
            .and_then(|index| self.targets.get(index))
            .and_then(|target| target.handlers.cursor);
        if let Some(cursor) = dragged {
            return cursor;
        }

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

    // drag out

    // the left button went down; a target that can be dragged out waits for the pointer to move
    pub fn arm_drag_out(&mut self) {
        self.drag_out = self
            .find_topmost(|target| target.handlers.drag_out.is_some())
            .map(|index| (index, self.x, self.y));
    }

    // what to drag out, once the pointer has moved far enough with the button down
    pub fn take_drag_out(&mut self) -> Option<DragData> {
        let (index, x, y) = self.drag_out?;

        if (self.x - x).hypot(self.y - y) < DRAG_OUT_DISTANCE {
            return None;
        }

        self.drag_out = None;

        // the compositor takes the pointer over, so the press won't end in a click
        self.pressed = None;

        let handler = self.targets.get(index)?.handlers.drag_out.as_ref()?;

        Some(handler())
    }

    pub fn disarm_drag_out(&mut self) {
        self.drag_out = None;
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

    /*
     * files held at (x, y): tells the target under them where they are, and
     * the one they were over before that they left; whether they can be let
     * go there, none when no target there asks to be told
     */
    pub fn files_over(&mut self, x: f32, y: f32, files: &[PathBuf]) -> Option<bool> {
        self.x = x;
        self.y = y;

        let over = self.find_topmost(|target| target.handlers.files_over.is_some());

        if self.files_over != over {
            self.files_left(files);
        }

        self.files_over = over;

        let target = &self.targets[over?];

        let handler = target.handlers.files_over.as_ref()?;

        Some(handler(Some(target.point(x, y)), files))
    }

    // the held files left the window or were let go; returns whether a handler ran
    pub fn files_left(&mut self, files: &[PathBuf]) -> bool {
        let Some(index) = self.files_over.take() else {
            return false;
        };

        let Some(handler) = self
            .targets
            .get(index)
            .and_then(|target| target.handlers.files_over.as_ref())
        else {
            return false;
        };

        handler(None, files);

        true
    }

    // text dragged in from another program let go at (x, y); returns whether a handler ran
    pub fn drop_text(&mut self, x: f32, y: f32, text: String) -> bool {
        self.x = x;
        self.y = y;

        let Some(index) = self.find_topmost(|target| target.handlers.text_drop.is_some()) else {
            return false;
        };

        let Some(drop) = &self.targets[index].handlers.text_drop else {
            return false;
        };

        drop(text);

        true
    }

    // returns whether a handler ran
    pub fn drop_files(&mut self, x: f32, y: f32, files: Vec<PathBuf>) -> bool {
        self.files_left(&files);

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
    fn held_files_are_told_where_they_are_and_when_they_leave() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let told = seen.clone();
        let handlers = Handlers {
            // only the left half takes them
            files_over: Some(Rc::new(move |at: Option<Point>, files: &[PathBuf]| {
                told.borrow_mut().push((at, files.len()));
                at.is_some_and(|at| at.x < 25.0)
            })),
            ..Handlers::default()
        };
        let mut pointer = Pointer::default();
        pointer.set_targets(vec![Target::new(
            Area::new(10.0, 10.0, 50.0, 50.0),
            handlers,
        )]);
        let files = [PathBuf::from("/tmp/a.md")];

        assert_eq!(pointer.files_over(20.0, 20.0, &files), Some(true));
        assert_eq!(pointer.files_over(50.0, 20.0, &files), Some(false));
        // outside every target that asks, nobody says
        assert_eq!(pointer.files_over(100.0, 100.0, &files), None);
        assert_eq!(
            seen.borrow().as_slice(),
            [
                (Some(Point { x: 10.0, y: 10.0 }), 1),
                (Some(Point { x: 40.0, y: 10.0 }), 1),
                (None, 1)
            ]
        );

        // let go over it, it hears they've gone before the drop
        pointer.files_over(20.0, 20.0, &files);
        pointer.drop_files(20.0, 20.0, files.to_vec());
        assert_eq!(seen.borrow().last(), Some(&(None, 1)));
    }

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
    fn the_dragged_target_keeps_the_cursor_until_the_drop() {
        let handlers = Handlers {
            drag: Some(Rc::new(|_| {})),
            cursor: Some(Cursor::ResizeBottomRight),
            ..Handlers::default()
        };
        let mut pointer = Pointer::default();
        pointer.set_targets(vec![Target::new(Area::new(0.0, 0.0, 10.0, 10.0), handlers)]);

        pointer.move_to(5.0, 5.0);
        assert_eq!(pointer.cursor(), Cursor::ResizeBottomRight);
        assert!(pointer.start_drag());
        pointer.move_to(50.0, 50.0);
        assert_eq!(pointer.cursor(), Cursor::ResizeBottomRight);
        pointer.stop_drag();
        assert_eq!(pointer.cursor(), Cursor::Default);
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
