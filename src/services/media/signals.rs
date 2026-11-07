#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo, missing_docs)]

//! Following the players by their signals, so nothing polls: a player
//! appearing or quitting (`NameOwnerChanged` for `org.mpris.MediaPlayer2.*`),
//! its properties changing (`PropertiesChanged` on `/org/mpris/MediaPlayer2`)
//! and a seek (`Seeked`). Players don't announce the position while a track
//! plays, so [`MediaPlayer::position`](super::MediaPlayer::position) works it
//! out from the last reading instead.

use std::sync::PoisonError;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use zbus::MatchRule;
use zbus::message::Type;

use crate::services::store;
use crate::{Bus, Service};

use super::{Media, mpris};

// a track change sends several signals at once; they make one read
const SETTLE: Duration = Duration::from_millis(50);

/// Reads the players again after each burst of their signals, for as long as
/// the program runs.
pub(super) fn follow() {
    let Some(rules) = rules() else {
        return;
    };

    let (sender, changes) = mpsc::channel::<()>();
    for rule in rules {
        let sender = sender.clone();
        thread::spawn(move || {
            for _ in Bus::session().watch_signals(rule) {
                if sender.send(()).is_err() {
                    break;
                }
            }
        });
    }
    drop(sender);

    // a change between new() and the watches starting would otherwise wait for the next one
    refresh();

    while changes.recv().is_ok() {
        loop {
            match changes.recv_timeout(SETTLE) {
                Ok(()) => {}
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        refresh();
    }
}

// read outside the lock, so a slow player never holds up drawing
fn refresh() {
    // read as the service's own, which doesn't count as watching it
    let before = store::find::<Media>()
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .active()
        .map(|player| player.name.clone());
    let fresh = Media::fresh(before.as_deref());

    let mut media = Media::write();
    if *media == fresh {
        media.quiet();
        return;
    }
    *media = fresh;
}

fn rules() -> Option<[MatchRule<'static>; 3]> {
    let owners = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender("org.freedesktop.DBus")
        .ok()?
        .interface("org.freedesktop.DBus")
        .ok()?
        .member("NameOwnerChanged")
        .ok()?
        .arg0ns(mpris::NAMESPACE)
        .ok()?
        .build();

    let properties = MatchRule::builder()
        .msg_type(Type::Signal)
        .interface("org.freedesktop.DBus.Properties")
        .ok()?
        .member("PropertiesChanged")
        .ok()?
        .path(mpris::PATH)
        .ok()?
        .build();

    let seeked = MatchRule::builder()
        .msg_type(Type::Signal)
        .interface(mpris::INTERFACE)
        .ok()?
        .member("Seeked")
        .ok()?
        .path(mpris::PATH)
        .ok()?
        .build();

    Some([owners, properties, seeked])
}
