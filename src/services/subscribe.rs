#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo, missing_docs)]

//! Change subscriptions: [`Service::subscribe`] calls back after a write that changed the
//! service, so one service can follow another without polling and without side effects in
//! views. Callbacks run on one thread of their own, in the order the changes came; a type
//! already waiting for its callbacks isn't queued twice, so a burst of writes costs one call.

use std::any::TypeId;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, LazyLock, Mutex, OnceLock, PoisonError};
use std::thread;

use crate::Service;

use super::store;

type Callback = Arc<dyn Fn() + Send + Sync>;

// each service's callbacks, by subscription id
type Subscribers = HashMap<TypeId, Vec<(u64, Callback)>>;

static SUBSCRIBERS: LazyLock<Mutex<Subscribers>> = LazyLock::new(|| Mutex::new(HashMap::new()));

// whether anything subscribed at all, so a write without subscribers takes no lock
static ANY: AtomicBool = AtomicBool::new(false);

// types whose callbacks are queued and haven't started
static PENDING: LazyLock<Mutex<HashSet<TypeId>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

static QUEUE: OnceLock<Mutex<Sender<TypeId>>> = OnceLock::new();

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

thread_local! {
    // the service whose callback this thread is running
    static IN_CALLBACK: Cell<Option<TypeId>> = const { Cell::new(None) };
}

/// A callback added with [`Service::subscribe`]. Dropping it removes the callback.
#[must_use = "dropping a Subscription removes its callback; call .forever() to keep it"]
pub struct Subscription {
    service: TypeId,
    id: u64,
}

impl Subscription {
    /// Keeps the callback for as long as the program runs.
    pub fn forever(self) {
        std::mem::forget(self);
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let mut subscribers = SUBSCRIBERS.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(callbacks) = subscribers.get_mut(&self.service) {
            callbacks.retain(|(id, _)| *id != self.id);
            if callbacks.is_empty() {
                subscribers.remove(&self.service);
            }
        }
    }
}

pub(crate) fn add<S: Service>(callback: impl Fn(&S) + Send + Sync + 'static) -> Subscription {
    let service = TypeId::of::<S>();
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);

    let callback: Callback = Arc::new(move || {
        // read as the callback's, not as a window's: it doesn't keep the service polling
        let value = store::find::<S>()
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        IN_CALLBACK.set(Some(service));
        callback(&value);
        IN_CALLBACK.set(None);
    });

    SUBSCRIBERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry(service)
        .or_default()
        .push((id, callback));
    ANY.store(true, Ordering::Release);

    Subscription { service, id }
}

/// After a write to `S` that changed it: queues its callbacks.
pub(crate) fn changed<S: Service>() {
    if !ANY.load(Ordering::Acquire) {
        return;
    }

    let service = TypeId::of::<S>();
    let subscribed = SUBSCRIBERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .contains_key(&service);
    if !subscribed {
        return;
    }

    let queued = !PENDING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(service);
    if queued {
        return;
    }

    let queue = QUEUE.get_or_init(|| {
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name(String::from("amane-subscriptions"))
            .spawn(move || run(receiver))
            .expect("failed to start the subscription thread");
        Mutex::new(sender)
    });
    let _ = queue
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .send(service);
}

fn run(receiver: mpsc::Receiver<TypeId>) {
    for service in receiver {
        // a write from here on queues the callbacks again
        PENDING
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&service);

        let callbacks: Vec<Callback> = SUBSCRIBERS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&service)
            .map(|callbacks| callbacks.iter().map(|(_, c)| Arc::clone(c)).collect())
            .unwrap_or_default();

        for callback in callbacks {
            if panic::catch_unwind(AssertUnwindSafe(|| callback())).is_err() {
                IN_CALLBACK.set(None);
                eprintln!("amane: a service subscription panicked");
            }
        }
    }
}

/// A callback that writes the service it was called for would wait on its own read.
pub(crate) fn check_not_in_callback<S: Service>() {
    debug_assert!(
        IN_CALLBACK.get() != Some(TypeId::of::<S>()),
        "{} written from its own subscription, which holds a read of it",
        std::any::type_name::<S>()
    );
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::Duration;

    use super::*;

    #[derive(Default)]
    struct Source(u32);

    impl Service for Source {
        fn new() -> Self {
            Self::default()
        }

        fn listen() {}
    }

    #[derive(Default)]
    struct Follower(u32);

    impl Service for Follower {
        fn new() -> Self {
            Self::default()
        }

        fn listen() {}
    }

    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn a_change_reaches_its_subscribers_and_a_quiet_write_does_not() {
        let (sent, seen) = mpsc::channel();
        let subscription = Source::subscribe(move |source| {
            // one service following another
            Follower::write().0 = source.0 * 10;
            let _ = sent.send(source.0);
        });

        Source::write().0 = 4;
        assert_eq!(seen.recv_timeout(WAIT), Ok(4));
        assert_eq!(Follower::read().0, 40);

        let mut quiet = Source::write();
        quiet.0 = 5;
        quiet.quiet();
        drop(quiet);
        assert_eq!(
            seen.recv_timeout(Duration::from_millis(200)),
            Err(RecvTimeoutError::Timeout)
        );

        drop(subscription);
        Source::write().0 = 6;
        assert!(seen.recv_timeout(Duration::from_millis(200)).is_err());
    }
}
