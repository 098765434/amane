use std::any::{self, Any, TypeId};
use std::collections::{HashMap, HashSet};
use std::panic;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock, PoisonError, RwLock};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

use crate::Service;

static SERVICES: LazyLock<Mutex<HashMap<TypeId, &'static (dyn Any + Send + Sync)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A service and what's known of its use, kept together so a read finds both at once.
pub(crate) struct Slot<S> {
    pub(crate) lock: RwLock<S>,
    pub(crate) use_: Use,
}

pub fn find<S: Service>() -> &'static RwLock<S> {
    &slot::<S>().lock
}

pub(crate) fn slot<S: Service>() -> &'static Slot<S> {
    let id = TypeId::of::<S>();

    if let Some(service) = SERVICES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&id)
    {
        return service.downcast_ref().expect("failed to find service");
    }

    // A constructor can read other services, so run it without holding the store mutex.
    let value = S::new();
    let mut services = SERVICES.lock().unwrap_or_else(PoisonError::into_inner);
    // Another thread may have initialized this type while its constructor was running.
    if let Some(service) = services.get(&id) {
        return service.downcast_ref().expect("failed to find service");
    }

    /*
     * services stay until the program exits,
     * so leaking gives a reference that is valid forever
     */
    let service = Box::leak(Box::new(Slot {
        lock: RwLock::new(value),
        use_: Use::new(id),
    }));
    USES.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(id, &service.use_);

    services.insert(id, service);
    drop(services);

    // stored first, so the thread's own reads and writes find this same service
    thread::spawn(keep_listening::<S>);

    service
}

/// Whether a service is watched, so a polling service can stop while nothing is: a window
/// whose last frame read it is open, or something read it lately. A window that draws nothing
/// new reads nothing either, which is why open windows' reads count and not only reads.
pub(crate) struct Use {
    service: TypeId,
    // milliseconds since START
    last_read: AtomicU64,
    // its listening thread is parked until the next read
    parked: AtomicBool,
    listener: OnceLock<Thread>,
}

static START: LazyLock<Instant> = LazyLock::new(Instant::now);

fn now_ms() -> u64 {
    u64::try_from(START.elapsed().as_millis()).unwrap_or(u64::MAX)
}

// every service's use, to wake one that a window starts reading
static USES: LazyLock<Mutex<HashMap<TypeId, &'static Use>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// the services some open window's last frame read, as the event loop last published them
static WATCHED: LazyLock<Mutex<HashSet<TypeId>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// The event loop's list of services open windows read; a service that joins it wakes.
pub(crate) fn set_watched(services: HashSet<TypeId>) {
    let mut watched = WATCHED.lock().unwrap_or_else(PoisonError::into_inner);
    if *watched == services {
        return;
    }
    let joined: Vec<TypeId> = services.difference(&watched).copied().collect();
    *watched = services;
    drop(watched);

    let uses = USES.lock().unwrap_or_else(PoisonError::into_inner);
    for id in joined {
        if let Some(use_) = uses.get(&id) {
            use_.wake();
        }
    }
}

fn is_watched(service: TypeId) -> bool {
    WATCHED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .contains(&service)
}

impl Use {
    fn new(service: TypeId) -> Self {
        Self {
            service,
            last_read: AtomicU64::new(now_ms()),
            parked: AtomicBool::new(false),
            listener: OnceLock::new(),
        }
    }

    /// A read: wakes the listener if it stopped for want of one.
    pub(crate) fn touch(&self) {
        self.last_read.store(now_ms(), Ordering::Relaxed);
        self.wake();
    }

    fn wake(&self) {
        if self.parked.load(Ordering::Acquire)
            && let Some(listener) = self.listener.get()
        {
            listener.unpark();
        }
    }

    fn unwatched(&self, idle: Duration) -> bool {
        let unread = now_ms().saturating_sub(self.last_read.load(Ordering::Relaxed));
        Duration::from_millis(unread) >= idle && !is_watched(self.service)
    }

    /// Parks the calling thread while the service is unwatched for `idle`.
    fn wait_while_unread(&self, idle: Duration) -> bool {
        if !self.unwatched(idle) {
            return false;
        }
        let me = self.listener.get_or_init(thread::current);
        if me.id() != thread::current().id() {
            // only the service's own listener stops; anyone else carries on
            return false;
        }
        self.parked.store(true, Ordering::Release);
        // a read between the check above and here has unparked already, so park returns
        while self.unwatched(idle) {
            thread::park();
        }
        self.parked.store(false, Ordering::Release);
        true
    }
}

/// For a service's own polling loop: returns at once while the service is watched (an open
/// window's last frame read it, or something read it within `idle`), and otherwise blocks
/// until it is watched again.
/// Returns whether it waited, so the loop can poll straight away instead of after its
/// interval.
pub fn wait_while_unread<S: Service>(idle: Duration) -> bool {
    slot::<S>().use_.wait_while_unread(idle)
}

// how long a service that panicked waits before it listens again
const RESTART_DELAY: Duration = Duration::from_secs(5);

/*
 * a listen that panics, like on a bus that went away, starts again after a
 * pause instead of leaving the service frozen; one that returns is done
 */
fn keep_listening<S: Service>() {
    while panic::catch_unwind(S::listen).is_err() {
        eprintln!(
            "amane: {} stopped, starting it again",
            any::type_name::<S>()
        );

        thread::sleep(RESTART_DELAY);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Barrier;

    use super::*;

    static CONSTRUCTORS: Barrier = Barrier::new(2);

    struct Concurrent;

    impl Service for Concurrent {
        fn new() -> Self {
            // Force both callers past the initial lookup before either publishes a value.
            CONSTRUCTORS.wait();
            Self
        }

        fn listen() {}
    }

    struct Polled;

    impl Service for Polled {
        fn new() -> Self {
            Self
        }

        fn listen() {}
    }

    #[test]
    fn an_unread_service_waits_until_it_is_read() {
        let idle = Duration::from_millis(50);
        let listener = thread::spawn(move || {
            // read on first use, so it carries on until idle
            assert!(!wait_while_unread::<Polled>(idle));
            thread::sleep(idle * 2);
            let waited_from = Instant::now();
            assert!(wait_while_unread::<Polled>(idle));
            waited_from.elapsed()
        });

        thread::sleep(idle * 6);
        slot::<Polled>().use_.touch();
        let waited = listener.join().unwrap();
        assert!(waited >= idle * 2, "it waited {waited:?} for the read");
    }

    struct Shown;

    impl Service for Shown {
        fn new() -> Self {
            Self
        }

        fn listen() {}
    }

    #[test]
    fn a_service_an_open_window_read_keeps_polling() {
        let idle = Duration::from_millis(30);
        slot::<Shown>();
        set_watched(HashSet::from([TypeId::of::<Shown>()]));
        thread::sleep(idle * 2);
        assert!(!wait_while_unread::<Shown>(idle));

        // the window closed: it stops, until a window reads it again
        set_watched(HashSet::new());
        let listener = thread::spawn(move || wait_while_unread::<Shown>(idle));
        thread::sleep(idle * 3);
        set_watched(HashSet::from([TypeId::of::<Shown>()]));
        assert!(listener.join().unwrap());
    }

    #[test]
    fn concurrent_first_use_returns_the_same_service() {
        let first = thread::spawn(find::<Concurrent>);
        let second = thread::spawn(find::<Concurrent>);
        let first = first.join().unwrap();
        let second = second.join().unwrap();
        assert!(std::ptr::eq(first, second));
        assert!(std::ptr::eq(first, find::<Concurrent>()));
    }
}
