use smithay_client_toolkit::seat::pointer::{
    AxisScroll, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, CursorIcon, PointerEvent, PointerEventKind,
    PointerHandler,
};
use wayland_client::{
    Connection, QueueHandle,
    protocol::wl_pointer::{AxisSource, WlPointer},
};

use crate::input::PIXELS_PER_LINE;
use crate::scale::ScaleFactor;
use crate::{Button, Cursor, Scroll, ScrollSource};

use super::WaylandState;

impl PointerHandler for WaylandState {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlPointer,
        events: &[PointerEvent],
    ) {
        let mut handled_on = Vec::new();

        for event in events {
            if self.handle_pointer(event) {
                handled_on.push(event.surface.clone());
            }
        }

        /*
         * a service a handler changed already wakes the windows that read it;
         * only the window the pointer is on redraws for its own state, like a
         * scroll offset, so hovering never redraws every window
         */
        for surface in handled_on {
            self.request_frame_on(&surface);
        }
    }
}

impl WaylandState {
    // returns whether a handler ran
    fn handle_pointer(&mut self, event: &PointerEvent) -> bool {
        // Grab any serial off the event first so we don't double-borrow self.
        // xdg_popup.grab() needs a recent input serial; any pointer event works.
        match &event.kind {
            PointerEventKind::Enter { serial, .. }
            | PointerEventKind::Leave { serial, .. }
            | PointerEventKind::Press { serial, .. }
            | PointerEventKind::Release { serial, .. } => {
                self.last_input_serial = Some(*serial);
            }
            _ => {}
        }

        if let PointerEventKind::Press { serial, .. } = &event.kind {
            self.last_press_serial = Some(*serial);
        }

        // every event names the surface it happened on
        let Some(window) = self.window(&event.surface) else {
            return false;
        };

        let pointer = &mut window.pointer;

        let (x, y) = event.position;

        match &event.kind {
            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                crate::input::set_pointer_position(x as f32, y as f32);

                let hovered = pointer.move_to(x as f32, y as f32);

                // a widget that can be dragged out hands over its data once the pointer moved far enough
                if let Some(data) = pointer.take_drag_out() {
                    let surface = event.surface.clone();

                    self.start_drag_out(&surface, data);

                    return true;
                }

                let dragged = pointer.drag();
                let moved = pointer.report_motion();

                let cursor = pointer.cursor();

                self.show_cursor(cursor);

                hovered || dragged || moved
            }

            PointerEventKind::Leave { .. } => {
                let left = pointer.leave();

                self.forget_cursor();

                left
            }

            PointerEventKind::Press { button, .. } => {
                pointer.press();

                // only the left button drags
                if to_button(*button) != Some(Button::Left) {
                    return false;
                }

                pointer.arm_drag_out();

                pointer.start_drag()
            }

            PointerEventKind::Release { button, .. } => {
                // side buttons like back and forward have no amane button yet
                let Some(button) = to_button(*button) else {
                    return false;
                };

                pointer.disarm_drag_out();

                // the drop comes before the click, so a widget can tell one from the other
                let dropped = button == Button::Left && pointer.stop_drag();

                pointer.release(button) || dropped
            }

            PointerEventKind::Axis {
                horizontal,
                vertical,
                source,
                ..
            } => {
                let source = to_source(*source);

                pointer.scroll(to_scroll(horizontal, vertical, source, window.scale_factor))
            }
        }
    }

    /*
     * uses cursor-shape when the compositor has it,
     * otherwise the pointer draws the icon from the cursor theme
     */
    pub fn show_cursor(&mut self, cursor: Cursor) {
        if self.cursor_shown == Some(cursor) {
            return;
        }

        let Some(pointer) = &self.pointer_device else {
            return;
        };

        // a missing icon in the theme leaves the last cursor in place
        if pointer.set_cursor(&self.connection, icon(cursor)).is_err() {
            return;
        }

        self.cursor_shown = Some(cursor);
    }

    // the compositor puts its own cursor back once the pointer leaves
    pub fn forget_cursor(&mut self) {
        self.cursor_shown = None;
    }
}

fn to_button(code: u32) -> Option<Button> {
    match code {
        BTN_LEFT => Some(Button::Left),
        BTN_RIGHT => Some(Button::Right),
        BTN_MIDDLE => Some(Button::Middle),
        _ => None,
    }
}

fn to_scroll(
    horizontal: &AxisScroll,
    vertical: &AxisScroll,
    source: ScrollSource,
    scale_factor: ScaleFactor,
) -> Scroll {
    Scroll {
        x: lines(horizontal, scale_factor),
        y: lines(vertical, scale_factor),
        source,
        stop: horizontal.stop || vertical.stop,
    }
}

// a frame without a source (older compositors) counts as a wheel
fn to_source(source: Option<AxisSource>) -> ScrollSource {
    match source {
        Some(AxisSource::Finger) => ScrollSource::Finger,
        Some(AxisSource::Continuous) => ScrollSource::Continuous,
        Some(AxisSource::WheelTilt) => ScrollSource::Tilt,
        _ => ScrollSource::Wheel,
    }
}

fn lines(axis: &AxisScroll, scale_factor: ScaleFactor) -> f32 {
    // a wheel reports its steps in 120ths, which is exact
    if axis.value120 != 0 {
        return axis.value120 as f32 / 120.0;
    }

    // older compositors report whole steps instead
    if axis.discrete != 0 {
        return axis.discrete as f32;
    }

    // a touchpad only reports pixels
    let pixels = scale_factor.logical(axis.absolute as f32);

    pixels / PIXELS_PER_LINE
}

fn icon(cursor: Cursor) -> CursorIcon {
    match cursor {
        Cursor::Default => CursorIcon::Default,
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::Text => CursorIcon::Text,
        Cursor::Grab => CursorIcon::Grab,
        Cursor::Grabbing => CursorIcon::Grabbing,
        Cursor::Move => CursorIcon::Move,
        Cursor::NotAllowed => CursorIcon::NotAllowed,
        Cursor::Wait => CursorIcon::Wait,
        Cursor::Crosshair => CursorIcon::Crosshair,

        Cursor::ResizeTop => CursorIcon::NResize,
        Cursor::ResizeBottom => CursorIcon::SResize,
        Cursor::ResizeLeft => CursorIcon::WResize,
        Cursor::ResizeRight => CursorIcon::EResize,
        Cursor::ResizeTopLeft => CursorIcon::NwResize,
        Cursor::ResizeTopRight => CursorIcon::NeResize,
        Cursor::ResizeBottomLeft => CursorIcon::SwResize,
        Cursor::ResizeBottomRight => CursorIcon::SeResize,
        Cursor::ResizeHorizontal => CursorIcon::EwResize,
        Cursor::ResizeVertical => CursorIcon::NsResize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_converts_touchpad_pixels_but_preserves_wheel_steps() {
        let factor = ScaleFactor::new(2.0);
        let touchpad = AxisScroll {
            absolute: 30.0,
            ..AxisScroll::default()
        };
        let wheel = AxisScroll {
            absolute: 30.0,
            value120: 120,
            ..AxisScroll::default()
        };
        let old_wheel = AxisScroll {
            absolute: 30.0,
            discrete: 1,
            ..AxisScroll::default()
        };

        assert_eq!(lines(&touchpad, factor), 1.0);
        assert_eq!(lines(&touchpad, ScaleFactor::default()), 2.0);
        assert_eq!(lines(&wheel, factor), 1.0);
        assert_eq!(lines(&old_wheel, factor), 1.0);
    }

    #[test]
    fn a_finger_lift_is_a_stop_from_the_touchpad() {
        let lifted = AxisScroll {
            stop: true,
            ..AxisScroll::default()
        };
        let scroll = to_scroll(
            &AxisScroll::default(),
            &lifted,
            to_source(Some(AxisSource::Finger)),
            ScaleFactor::default(),
        );

        assert!(scroll.stop);
        assert_eq!(scroll.source, ScrollSource::Finger);
        assert_eq!((scroll.x, scroll.y), (0.0, 0.0));
    }

    #[test]
    fn a_frame_without_a_source_is_a_wheel() {
        assert_eq!(to_source(None), ScrollSource::Wheel);
        assert_eq!(to_source(Some(AxisSource::Wheel)), ScrollSource::Wheel);
    }
}
