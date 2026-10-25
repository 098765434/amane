use smithay_client_toolkit::reexports::protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    zwp_text_input_v3::{self, ChangeCause, ContentHint, ContentPurpose, ZwpTextInputV3},
};
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    protocol::{wl_seat::WlSeat, wl_surface::WlSurface},
};

use crate::input::focus;
use crate::widgets::text_input::{self, Preedit};

use super::WaylandState;

// the compositor only takes this much surrounding text, in bytes
const SURROUNDING_LIMIT: usize = 4000;

// a seat's input method, which types text a keyboard can't, like Chinese or Japanese
pub struct InputMethod {
    input: ZwpTextInputV3,

    // the surface the input method is on, which has the keyboard
    entered: Option<WlSurface>,

    enabled: bool,

    // what was last sent, so only a change is sent again
    sent: Option<Sent>,

    // what the input method sent since its last done
    pending: Pending,
}

#[derive(Clone, PartialEq)]
struct Sent {
    surrounding: (String, i32, i32),
    cursor: (i32, i32, i32, i32),
    kind: (bool, bool),
}

#[derive(Default)]
struct Pending {
    preedit: Option<(String, i32)>,
    commit: Option<String>,
    delete: (u32, u32),
}

impl InputMethod {
    pub fn new(
        manager: &ZwpTextInputManagerV3,
        seat: &WlSeat,
        qh: &QueueHandle<WaylandState>,
    ) -> Self {
        Self {
            input: manager.get_text_input(seat, qh, ()),
            entered: None,
            enabled: false,
            sent: None,
            pending: Pending::default(),
        }
    }
}

impl Drop for InputMethod {
    fn drop(&mut self) {
        self.input.destroy();
    }
}

impl WaylandState {
    /*
     * after each turn of the loop: the input method is on while a text input has the
     * keyboard, and hears where its text and cursor are
     */
    pub fn sync_input_method(&mut self) {
        let scale = self.scale_factor.get();

        let Some(method) = &mut self.input_method else {
            return;
        };

        let focused = focus::current().filter(|_| method.entered.is_some());

        let Some(id) = focused else {
            if method.enabled {
                method.input.disable();
                method.input.commit();

                method.enabled = false;
                method.sent = None;
            }

            return;
        };

        let field = text_input::field(id);

        let (x, y, width, height) = text_input::cursor_area();

        let sent = Sent {
            surrounding: surrounding(&field.text, field.cursor, field.anchor),
            cursor: (
                (x * scale) as i32,
                (y * scale) as i32,
                (width * scale).ceil() as i32,
                (height * scale).ceil() as i32,
            ),
            kind: text_input::kind(id),
        };

        if method.enabled && method.sent.as_ref() == Some(&sent) {
            return;
        }

        if !method.enabled {
            method.input.enable();

            method.enabled = true;
        }

        let (text, cursor, anchor) = sent.surrounding.clone();

        method.input.set_surrounding_text(text, cursor, anchor);
        method.input.set_text_change_cause(ChangeCause::Other);

        let (cursor_x, cursor_y, cursor_width, cursor_height) = sent.cursor;

        method
            .input
            .set_cursor_rectangle(cursor_x, cursor_y, cursor_width, cursor_height);

        let (password, multiline) = sent.kind;

        let (mut hint, purpose) = if password {
            (
                ContentHint::SensitiveData | ContentHint::HiddenText,
                ContentPurpose::Password,
            )
        } else {
            (ContentHint::None, ContentPurpose::Normal)
        };

        if multiline {
            hint |= ContentHint::Multiline;
        }

        method.input.set_content_type(hint, purpose);
        method.input.commit();

        method.sent = Some(sent);
    }

    // what the input method sent since its last done goes into the focused input
    fn apply_input_method(&mut self) {
        let Some(method) = &mut self.input_method else {
            return;
        };

        let pending = std::mem::take(&mut method.pending);

        let Some(id) = focus::current() else {
            return;
        };

        text_input::change(id, |field| {
            // the lengths are bytes around the cursor, the field counts letters
            let before: String = field.text.chars().take(field.cursor).collect();
            let after: String = field.text.chars().skip(field.cursor).collect();

            let before = letters_in(
                before.as_bytes(),
                before.len().saturating_sub(pending.delete.0 as usize)..,
            );
            let after = letters_in(
                after.as_bytes(),
                ..(pending.delete.1 as usize).min(after.len()),
            );

            let changed =
                field.commit_input_method(before, after, pending.commit.as_deref().unwrap_or(""));

            field.preedit = pending.preedit.map(|(text, cursor)| {
                // a cursor of -1 hides it, and it is in bytes like the rest
                let cursor = usize::try_from(cursor)
                    .ok()
                    .map(|cursor| letters_in(text.as_bytes(), ..cursor.min(text.len())));

                Preedit { text, cursor }
            });

            changed
        });
    }
}

// how many letters a byte range of some text holds, cut at whole letters
fn letters_in(bytes: &[u8], range: impl std::slice::SliceIndex<[u8], Output = [u8]>) -> usize {
    String::from_utf8_lossy(bytes.get(range).unwrap_or_default())
        .chars()
        .filter(|letter| *letter != char::REPLACEMENT_CHARACTER)
        .count()
}

// the text around the cursor, with the cursor and selection end in bytes, kept under the limit
fn surrounding(text: &str, cursor: usize, anchor: Option<usize>) -> (String, i32, i32) {
    let letters: Vec<char> = text.chars().collect();

    let anchor = anchor.unwrap_or(cursor);

    // a long text is cut to some lines around the cursor
    let (start, end) = if text.len() < SURROUNDING_LIMIT {
        (0, letters.len())
    } else {
        let start = cursor.min(anchor).saturating_sub(400);

        (start, (cursor.max(anchor) + 400).min(letters.len()))
    };

    let bytes = |at: usize| -> i32 {
        letters[start..at.clamp(start, end)]
            .iter()
            .map(|letter| letter.len_utf8())
            .sum::<usize>() as i32
    };

    let shown: String = letters[start..end].iter().collect();

    (shown, bytes(cursor), bytes(anchor))
}

impl Dispatch<ZwpTextInputV3, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _: &ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(method) = &mut state.input_method else {
            return;
        };

        match event {
            zwp_text_input_v3::Event::Enter { surface } => {
                method.entered = Some(surface);
            }

            zwp_text_input_v3::Event::Leave { .. } => {
                method.entered = None;

                // leaving disables it on the compositor's side already
                method.enabled = false;
                method.sent = None;
            }

            zwp_text_input_v3::Event::PreeditString {
                text, cursor_begin, ..
            } => {
                method.pending.preedit = text.map(|text| (text, cursor_begin));
            }

            zwp_text_input_v3::Event::CommitString { text } => {
                method.pending.commit = text;
            }

            zwp_text_input_v3::Event::DeleteSurroundingText {
                before_length,
                after_length,
            } => {
                method.pending.delete = (before_length, after_length);
            }

            zwp_text_input_v3::Event::Done { .. } => {
                state.apply_input_method();

                // the text changed under the input method, which wants to hear it
                if let Some(method) = &mut state.input_method {
                    method.sent = None;
                }
            }

            _ => {}
        }
    }
}

// the manager sends nothing
impl Dispatch<ZwpTextInputManagerV3, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ZwpTextInputManagerV3,
        _: <ZwpTextInputManagerV3 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surrounding_text_counts_bytes() {
        // "é" takes two bytes, so the cursor after "hé" is at byte 3
        assert_eq!(surrounding("héllo", 2, None), (String::from("héllo"), 3, 3));
        assert_eq!(
            surrounding("héllo", 2, Some(0)),
            (String::from("héllo"), 3, 0)
        );
    }

    #[test]
    fn byte_lengths_become_letters() {
        assert_eq!(letters_in("你好".as_bytes(), 3..), 1);
        assert_eq!(letters_in("ab".as_bytes(), ..1), 1);
    }
}
