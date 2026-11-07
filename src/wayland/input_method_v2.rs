#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo)]

use wayland_client::protocol::{wl_seat::WlSeat, wl_surface::WlSurface};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_misc::zwp_input_method_v2::client::{
    zwp_input_method_manager_v2::ZwpInputMethodManagerV2,
    zwp_input_method_v2::{self, ZwpInputMethodV2},
    zwp_input_popup_surface_v2::{self, ZwpInputPopupSurfaceV2},
};
use zeroize::Zeroize;

use super::{
    WaylandState,
    surface::{Role, View},
};
use crate::Service;
use crate::services::focused_field::{FocusedField, Hints, Purpose};

/*
 * this app as the seat's input method (zwp_input_method_v2), only after App::input_method:
 * it hears what kind of field has the keyboard in any app and shows a popup by it, and never
 * types through it; text goes in through niri-secure-fill, which checks the field first
 */
pub struct ImeServer {
    method: ZwpInputMethodV2,

    // what came since the last done, applied together as the protocol asks
    pending: Pending,

    // whether a field has the keyboard, as of the last done
    active: bool,

    // another input method has the seat; this one is inert for good
    unavailable: bool,
}

#[derive(Default)]
struct Pending {
    // an activate or deactivate came; the state before it is gone
    activate: Option<bool>,
    content: Option<(u32, u32)>,
    empty: Option<bool>,
}

impl ImeServer {
    pub fn new(
        manager: &ZwpInputMethodManagerV2,
        seat: &WlSeat,
        qh: &QueueHandle<WaylandState>,
    ) -> Self {
        Self {
            method: manager.get_input_method(seat, qh, ()),
            pending: Pending::default(),
            active: false,
            unavailable: false,
        }
    }
}

impl Drop for ImeServer {
    fn drop(&mut self) {
        self.method.destroy();
    }
}

// the popup by the focused field: a surface of the input method's own, placed by the compositor
pub struct InputPopupSurface {
    pub popup: ZwpInputPopupSurfaceV2,
    pub surface: WlSurface,
}

impl Drop for InputPopupSurface {
    fn drop(&mut self) {
        self.popup.destroy();
        self.surface.destroy();
    }
}

impl WaylandState {
    // after each turn of the loop: the popup shows while a field has the keyboard and the view wants it
    pub(crate) fn reconcile_input_popup(&mut self) {
        let Some(view) = self.input_popup_view.clone() else {
            return;
        };

        let active = self
            .ime
            .as_ref()
            .is_some_and(|ime| ime.active && !ime.unavailable);

        let popup = view();
        let wanted = active && popup.visible;

        let live = self
            .windows
            .iter()
            .position(|window| matches!(window.role, Role::InputPopup(_)));

        match (wanted, live) {
            (true, None) => self.open_input_popup(view, popup.size),

            (true, Some(index)) => {
                let window = &mut self.windows[index];
                let (width, height) = (popup.size.0.max(1), popup.size.1.max(1));

                if (window.width, window.height) != (width, height) {
                    window.resize(width, height);
                }
            }

            (false, Some(index)) => {
                let surface = self.windows[index].role.wl_surface().clone();

                self.close(&surface);
            }

            (false, None) => {}
        }
    }

    fn open_input_popup(&mut self, view: crate::popup::PopupView, size: (u32, u32)) {
        let Some(ime) = &self.ime else {
            return;
        };

        let surface = self.compositor.create_surface(&self.qh);
        let popup = ime.method.get_input_popup_surface(&surface, &self.qh, ());

        self.add(
            View::Popup(view),
            None,
            Role::InputPopup(InputPopupSurface { popup, surface }),
        );

        if let Some(window) = self.windows.last_mut() {
            window.resize(size.0.max(1), size.1.max(1));
        }
    }

    // a field's state is complete: FocusedField hears it, and a new field gets a new target
    fn apply_ime_done(&mut self) {
        let Some(ime) = &mut self.ime else {
            return;
        };

        let pending = std::mem::take(&mut ime.pending);

        if let Some(active) = pending.activate {
            ime.active = active;
        }

        let active = ime.active;

        // amane's own windows have a text input too, so its fields reach its own input method
        let own = self.keyboard_focus.is_some();

        let generation = {
            let mut field = FocusedField::write();

            if pending.activate.is_some() {
                field.clear();
            }

            if active {
                let (hint, purpose) = pending.content.unwrap_or_else(|| {
                    let hints = field.hints().0;

                    (hints, purpose_to_wire(field.purpose()))
                });

                let empty = pending.empty.or(if pending.activate.is_some() {
                    None
                } else {
                    field.empty()
                });

                field.set_field(Purpose::from_wire(purpose), Hints(hint), empty);
                field.set_own(own);
            }

            field.generation()
        };

        if !active {
            self.drop_fill_target();

            return;
        }

        if self.fill_target.as_ref().map(|target| target.generation()) != Some(generation) {
            self.take_fill_target(generation);
        }
    }
}

fn purpose_to_wire(purpose: Purpose) -> u32 {
    match purpose {
        Purpose::Normal => 0,
        Purpose::Alpha => 1,
        Purpose::Digits => 2,
        Purpose::Number => 3,
        Purpose::Phone => 4,
        Purpose::Url => 5,
        Purpose::Email => 6,
        Purpose::Name => 7,
        Purpose::Password => 8,
        Purpose::Pin => 9,
        Purpose::Date => 10,
        Purpose::Time => 11,
        Purpose::Datetime => 12,
        Purpose::Terminal => 13,
    }
}

impl Dispatch<ZwpInputMethodManagerV2, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ZwpInputMethodManagerV2,
        _: <ZwpInputMethodManagerV2 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpInputMethodV2, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _: &ZwpInputMethodV2,
        event: zwp_input_method_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(ime) = &mut state.ime else {
            return;
        };

        match event {
            zwp_input_method_v2::Event::Activate => {
                ime.pending = Pending {
                    activate: Some(true),
                    ..Pending::default()
                };
            }

            zwp_input_method_v2::Event::Deactivate => {
                ime.pending = Pending {
                    activate: Some(false),
                    ..Pending::default()
                };
            }

            // only whether there is text is kept; the text itself is wiped at once
            zwp_input_method_v2::Event::SurroundingText { mut text, .. } => {
                ime.pending.empty = Some(text.is_empty());

                text.zeroize();
            }

            zwp_input_method_v2::Event::ContentType { hint, purpose } => {
                let hint = match hint {
                    wayland_client::WEnum::Value(hint) => hint.bits(),
                    wayland_client::WEnum::Unknown(hint) => hint,
                };

                let purpose = match purpose {
                    wayland_client::WEnum::Value(purpose) => purpose as u32,
                    wayland_client::WEnum::Unknown(purpose) => purpose,
                };

                ime.pending.content = Some((hint, purpose));
            }

            zwp_input_method_v2::Event::TextChangeCause { .. } => {}

            zwp_input_method_v2::Event::Done => state.apply_ime_done(),

            zwp_input_method_v2::Event::Unavailable => {
                ime.unavailable = true;
                ime.active = false;

                eprintln!("amane: another input method has the seat, fill in apps is off");

                state.drop_fill_target();

                FocusedField::write().set_available(false, false);
            }

            _ => {}
        }
    }
}

impl Dispatch<ZwpInputPopupSurfaceV2, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ZwpInputPopupSurfaceV2,
        event: zwp_input_popup_surface_v2::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwp_input_popup_surface_v2::Event::TextInputRectangle {
            x,
            y,
            width,
            height,
        } = event
        {
            FocusedField::write().set_field_rect((x, y, width, height));
        }
    }
}
