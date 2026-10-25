mod appearance;
mod apps;
mod audio;
mod battery;
mod bluetooth;
mod brightness;
mod cpu;
mod gestures;
mod idle;
mod lock;
mod media;
mod memory;
mod network;
mod notifications;
mod overview;
mod palette;
mod pam;
mod power_profile;
pub mod store;
mod window_info;
mod windows;
mod worker;
mod workspace;
mod workspaces;
mod write;

use std::any::TypeId;
use std::sync::{PoisonError, RwLockReadGuard};
use std::thread;
use std::time::Duration;

use crate::changes;

pub use appearance::{Appearance, Scheme};
pub use apps::{Apps, DesktopAction, DesktopApp};
pub use audio::{Audio, AudioDevice, AudioKind};
pub use battery::{Battery, BatteryState, DeviceKind, PowerDevice, WarningLevel};
pub use bluetooth::{Bluetooth, BluetoothDevice};
pub use brightness::Brightness;
pub use cpu::Cpu;
pub use gestures::Gestures;
pub use idle::Idle;
pub use lock::Lock;
pub use media::{Media, MediaPlayer};
pub use memory::Memory;
pub use network::{AccessPoint, Link, Network};
pub use notifications::{Action, Notification, Notifications, Timeout, Urgency};
pub use overview::Overview;
pub use palette::Palette;
pub use pam::{Pam, PamConfig, PamError, PamMessage, PamMessageKind, PamResult};
pub use power_profile::PowerProfile;
pub use window_info::WindowInfo;
pub use windows::Windows;
pub use workspace::Workspace;
pub use workspaces::Workspaces;
pub use write::Write;

pub trait Service: Send + Sync + Sized + 'static {
    fn new() -> Self;

    fn interval() -> Duration {
        Duration::from_secs(1)
    }

    // reads the source again and says whether anything a window shows changed
    fn update(&mut self) -> bool {
        false
    }

    /*
     * runs on the service's own thread, so waiting here never
     * freezes drawing; event-driven services replace it with their
     * own loop, and services that only change through input with
     * an empty one
     */
    fn listen() {
        loop {
            thread::sleep(Self::interval());

            let mut service = Self::write();

            // a poll that found nothing new redraws nothing
            if !service.update() {
                service.quiet();
            }
        }
    }

    fn read() -> RwLockReadGuard<'static, Self> {
        // remembered, so a later change redraws only the windows that read it
        changes::note_read(TypeId::of::<Self>());

        store::find::<Self>()
            .read()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /*
     * for input handlers: inside view() a read of the same
     * service is still held, so this would wait forever
     */
    fn write() -> Write<Self> {
        let guard = store::find::<Self>()
            .write()
            .unwrap_or_else(PoisonError::into_inner);

        Write {
            guard,
            quiet: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;

    #[derive(Default)]
    struct Counter(u32);

    impl Service for Counter {
        fn new() -> Self {
            Self::default()
        }

        fn listen() {}
    }

    #[test]
    fn reads_after_a_write_panicked() {
        let failed = thread::spawn(|| {
            let _counter = Counter::write();

            panic!("a write that fails halfway");
        });

        assert!(failed.join().is_err());

        assert_eq!(Counter::read().0, 0);
    }
}
