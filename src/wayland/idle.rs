use std::time::Duration;

use smithay_client_toolkit::reexports::protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1::{self, ExtIdleNotificationV1},
    ext_idle_notifier_v1::{self, ExtIdleNotifierV1},
};
use wayland_client::{Connection, Dispatch, QueueHandle};

use crate::Idle;

use super::WaylandState;

impl WaylandState {
    // runs on every wake, since that is when new levels from Idle::set_levels show up
    pub fn update_idle_if_asked(&mut self) {
        if !Idle::take_request() {
            return;
        }

        self.make_idle_notifications();
    }

    // one timer per seat and level, all made again so each starts from zero
    pub fn make_idle_notifications(&mut self) {
        // a compositor without ext-idle-notify never says the user is idle, which is not an error
        let Some(notifier) = &self.idle_notifier else {
            return;
        };

        if !Idle::wanted() {
            return;
        }

        for notification in self.idle_notifications.drain(..) {
            notification.destroy();
        }

        let levels = Idle::current_levels();

        for seat in self.seat.seats() {
            for &level in &levels {
                let timeout = u32::try_from(level.as_millis()).unwrap_or(u32::MAX);

                let notification = notifier.get_idle_notification(timeout, &seat, &self.qh, level);

                self.idle_notifications.push(notification);
            }
        }

        Idle::reset_crossed();
    }
}

impl Dispatch<ExtIdleNotificationV1, Duration> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        level: &Duration,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_idle_notification_v1::Event::Idled => Idle::idled(*level),
            ext_idle_notification_v1::Event::Resumed => Idle::resumed(*level),

            _ => {}
        }
    }
}

// the notifier never sends events, but wayland-client still needs somewhere to send them
impl Dispatch<ExtIdleNotifierV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ExtIdleNotifierV1,
        _: ext_idle_notifier_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
