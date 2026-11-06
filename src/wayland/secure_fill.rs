use std::sync::{Mutex, PoisonError};

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use super::WaylandState;
use crate::Service;
use crate::services::focused_field::{FILLS, FieldOwner, FillDone, FillError, FocusedField};

pub mod protocol {
    #![allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
    #![allow(non_upper_case_globals, non_snake_case, unused_imports)]
    #![allow(missing_docs, clippy::all)]

    use wayland_client;
    use wayland_client::protocol::*;

    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/niri-secure-fill-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/niri-secure-fill-v1.xml");
}

use protocol::niri_secure_fill_manager_v1::NiriSecureFillManagerV1;
use protocol::niri_secure_fill_target_v1::{self, NiriSecureFillTargetV1, Refusal};

const FLAG_WINDOW: u32 = 1;
const FLAG_LAYER: u32 = 2;
const FLAG_SANDBOXED: u32 = 4;
const FLAG_TEXT_INPUT: u32 = 8;
const FLAG_UNKNOWN_CREDENTIALS: u32 = 16;

/*
 * the target taken for the field that has the keyboard (niri-secure-fill): who owns it, as
 * the compositor knows it, and what a fill into it waits for
 */
pub struct Target {
    target: NiriSecureFillTargetV1,
    generation: u64,
    taken: std::time::Instant,
}

// niri keeps a target two minutes; one older than this is taken again right before a fill
const FRESH: std::time::Duration = std::time::Duration::from_secs(60);

impl Target {
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

impl Drop for Target {
    fn drop(&mut self) {
        self.target.destroy();
    }
}

// what the target's one answer runs, once it comes
#[derive(Default)]
pub struct TargetData {
    generation: u64,
    done: Mutex<Option<FillDone>>,
}

impl WaylandState {
    // a new field has the keyboard: a new target, so its owner is known and a fill checked
    pub(crate) fn take_fill_target(&mut self, generation: u64) {
        let Some(manager) = &self.secure_fill else {
            return;
        };

        let target = manager.get_target(
            &self.qh,
            TargetData {
                generation,
                done: Mutex::new(None),
            },
        );

        self.fill_target = Some(Target {
            target,
            generation,
            taken: std::time::Instant::now(),
        });
    }

    pub(crate) fn drop_fill_target(&mut self) {
        self.fill_target = None;
    }

    // the fills FocusedField::fill asked for; each goes to its own field's target, or nowhere
    pub(crate) fn handle_fill_requests(&mut self) {
        let requests = std::mem::take(&mut *FILLS.lock().unwrap_or_else(PoisonError::into_inner));

        for request in requests {
            // a field focused long ago: a fresh snapshot of it, which niri checks the fill against
            // in the same order; nothing about the field changed, or its generation would have
            if let Some(target) = &self.fill_target
                && target.generation == request.generation
                && target.taken.elapsed() > FRESH
            {
                self.take_fill_target(request.generation);
            }

            let Some(target) = self
                .fill_target
                .take()
                .filter(|target| target.generation == request.generation)
            else {
                let error = if self.secure_fill.is_none() || self.ime.is_none() {
                    FillError::Unsupported
                } else {
                    FillError::Changed
                };

                (request.done)(Err(error));

                continue;
            };

            if let Some(data) = target.target.data::<TargetData>() {
                *data.done.lock().unwrap_or_else(PoisonError::into_inner) = Some(request.done);
            }

            // the text is copied onto the wire; this copy is wiped when the request drops
            target.target.fill(String::from(request.text.as_str()));

            // a target is filled once: it waits for its answer, the next fill takes the next one
            let generation = target.generation;
            self.spent_fill_targets.push(target);

            self.take_fill_target(generation);
        }
    }
}

impl Dispatch<NiriSecureFillManagerV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &NiriSecureFillManagerV1,
        _: <NiriSecureFillManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<NiriSecureFillTargetV1, TargetData> for WaylandState {
    fn event(
        state: &mut Self,
        proxy: &NiriSecureFillTargetV1,
        event: niri_secure_fill_target_v1::Event,
        data: &TargetData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            niri_secure_fill_target_v1::Event::Identity {
                pid,
                exe,
                app_id,
                window_id_hi,
                window_id_lo,
                flags,
            } => {
                let flags = match flags {
                    wayland_client::WEnum::Value(flags) => flags.bits(),
                    wayland_client::WEnum::Unknown(flags) => flags,
                };

                let window_id = (u64::from(window_id_hi) << 32) | u64::from(window_id_lo);

                let known = flags & FLAG_UNKNOWN_CREDENTIALS == 0 && pid > 0;

                let owner = FieldOwner {
                    pid: known.then_some(pid as u32),
                    exe: (known && !exe.is_empty()).then(|| exe.into()),
                    app_id,
                    window_id: (flags & FLAG_WINDOW != 0 && window_id != 0).then_some(window_id),
                    in_window: flags & FLAG_WINDOW != 0,
                    in_layer: flags & FLAG_LAYER != 0,
                    sandboxed: flags & FLAG_SANDBOXED != 0,
                    takes_text: flags & FLAG_TEXT_INPUT != 0,
                };

                FocusedField::write().set_owner(data.generation, owner);
            }

            niri_secure_fill_target_v1::Event::Filled => {
                if let Some(done) = data
                    .done
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take()
                {
                    done(Ok(()));
                }

                state
                    .spent_fill_targets
                    .retain(|spent| spent.target != *proxy);
            }

            niri_secure_fill_target_v1::Event::Refused { reason } => {
                let error = match reason {
                    wayland_client::WEnum::Value(Refusal::Changed) => FillError::Changed,
                    wayland_client::WEnum::Value(Refusal::NoTextInput) => FillError::NoTextInput,
                    wayland_client::WEnum::Value(Refusal::Locked) => FillError::Locked,
                    wayland_client::WEnum::Value(Refusal::Expired) => FillError::Expired,
                    wayland_client::WEnum::Value(Refusal::Used) => FillError::Used,
                    wayland_client::WEnum::Value(Refusal::TooSoon) => FillError::TooSoon,
                    wayland_client::WEnum::Value(Refusal::InvalidText) => FillError::InvalidText,
                    _ => FillError::Changed,
                };

                if let Some(done) = data
                    .done
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take()
                {
                    done(Err(error));
                }

                state
                    .spent_fill_targets
                    .retain(|spent| spent.target != *proxy);
            }
        }
    }
}
