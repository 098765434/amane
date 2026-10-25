use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::services::worker;
use crate::{Argument, Bus, Urgency};

// the app-id App::run started with, which notifications are sent under
pub(crate) static APP_NAME: OnceLock<String> = OnceLock::new();

/*
 * a desktop notification for the notification server to show, like the shell's:
 * Notify::new("Saved").body("notes.txt").send()
 */
pub struct Notify {
    summary: String,
    body: String,
    icon: String,
    urgency: Urgency,
}

impl Notify {
    pub fn new(summary: &str) -> Self {
        Self {
            summary: String::from(summary),
            body: String::new(),
            icon: String::new(),
            urgency: Urgency::Normal,
        }
    }

    pub fn body(mut self, body: &str) -> Self {
        self.body = String::from(body);

        self
    }

    // an icon theme name, like "document-save", or a file path
    pub fn icon(mut self, icon: &str) -> Self {
        self.icon = String::from(icon);

        self
    }

    pub fn urgency(mut self, urgency: Urgency) -> Self {
        self.urgency = urgency;

        self
    }

    // sent on the worker thread, so a slow server never holds up a frame
    pub fn send(self) {
        let app_name = APP_NAME
            .get()
            .cloned()
            .unwrap_or_else(|| String::from("amane"));

        let urgency = match self.urgency {
            Urgency::Low => 0,
            Urgency::Normal => 1,
            Urgency::Critical => 2,
        };

        let hints = BTreeMap::from([(String::from("urgency"), Argument::Byte(urgency))]);

        worker::run(move || {
            Bus::session().call(
                "org.freedesktop.Notifications",
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications",
                "Notify",
                &[
                    Argument::Text(app_name),
                    Argument::Unsigned(0),
                    Argument::Text(self.icon),
                    Argument::Text(self.summary),
                    Argument::Text(self.body),
                    Argument::TextList(Vec::new()),
                    Argument::Map(hints),
                    // the server decides how long it stays
                    Argument::Int(-1),
                ],
            );
        });
    }
}
