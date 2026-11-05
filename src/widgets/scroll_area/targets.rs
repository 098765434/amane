use std::rc::Rc;

use crate::Scroll;
use crate::graphics::Area;
use crate::input::{Handlers, Target, clip};

use super::{ScrollArea, offsets};

pub fn collect_targets(scroll_area: &ScrollArea, area: Area, targets: &mut Vec<Target>) {
    let id = scroll_area.id;
    let farthest = scroll_area.farthest(area);

    // fingers move it one to one and it coasts on after they lift; a wheel's step eases in
    let on_scroll = move |scroll: Scroll| offsets::scroll(id, &scroll, farthest);

    let handlers = Handlers {
        scroll: Some(Rc::new(on_scroll)),
        ..Handlers::default()
    };

    // added before the child, so a child with its own on_scroll still wins
    targets.push(Target::new(area, handlers));

    let first = targets.len();

    scroll_area
        .child
        .collect_targets(scroll_area.child_area(area), targets);

    // the parts scrolled out of view don't react to the pointer
    clip(&mut targets[first..], area);
}
