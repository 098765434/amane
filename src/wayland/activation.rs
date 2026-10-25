use std::mem;
use std::sync::PoisonError;

use smithay_client_toolkit::activation::{ActivationHandler, RequestData};

use crate::process::{ACTIVATED, spawn_with_token};

use super::{WaylandState, surface::Role};

impl WaylandState {
    /*
     * the app was launched again: its activate handler runs with that launch's arguments,
     * or without one its window comes back, and the token lets the window take the focus
     */
    pub fn activate(&mut self, arguments: &[String], on_activate: Option<fn(&[String])>) {
        // the first argument is the token, empty when the launch had none
        let (token, arguments) = match arguments.split_first() {
            Some((token, arguments)) => (token.as_str(), arguments),
            None => ("", arguments),
        };

        match on_activate {
            Some(on_activate) => on_activate(arguments),

            None => {
                let open = self
                    .windows
                    .iter()
                    .any(|window| matches!(window.role, Role::Normal { .. }));

                // a closed app that kept running opens its windows again
                if !open {
                    for (key, view) in self.normal_views.clone() {
                        self.open_normal(key, view);
                    }
                }
            }
        }

        // what the handler opened is opened now, so the token goes to it
        self.handle_window_requests();

        if !token.is_empty() {
            self.pending_token = Some(String::from(token));
        }

        self.use_token();
    }

    /*
     * focuses the newest normal window with the token waiting, once it is on screen;
     * one still waiting for its first configure gets it then
     */
    pub fn use_token(&mut self) {
        let Some(activation) = &self.activation else {
            self.pending_token = None;

            return;
        };

        let newest = self
            .windows
            .iter()
            .rev()
            .find(|window| matches!(window.role, Role::Normal { .. }));

        let Some(newest) = newest else {
            return;
        };

        if newest.width == 0 {
            return;
        }

        let Some(token) = self.pending_token.take() else {
            return;
        };

        activation.activate::<Self>(newest.role.wl_surface(), token);
    }

    // the programs spawn_activated asked for, each started once the compositor gives a token
    pub fn handle_spawn_requests(&mut self) {
        let mut queue = ACTIVATED.lock().unwrap_or_else(PoisonError::into_inner);

        let commands = mem::take(&mut *queue);

        drop(queue);

        for command in commands {
            let Some(activation) = &self.activation else {
                spawn_with_token(&command, None);

                continue;
            };

            // the token stands for the input that asked, on the window that has the keyboard
            let seat = self.seat.seats().next();

            let seat_and_serial = seat.zip(self.last_input_serial);

            let surface = self.keyboard_focus.clone().or_else(|| {
                self.windows
                    .first()
                    .map(|window| window.role.wl_surface().clone())
            });

            activation.request_token(
                &self.qh,
                RequestData {
                    app_id: Some(self.app_id.clone()),
                    seat_and_serial,
                    surface,
                    udata: command,
                },
            );
        }
    }
}

impl ActivationHandler for WaylandState {
    // the command to start with the token
    type RequestUdata = String;

    fn new_token(&mut self, token: String, data: &RequestData<String>) {
        spawn_with_token(&data.udata, Some(&token));
    }
}
