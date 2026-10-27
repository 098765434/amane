use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::services::worker;
use crate::{Argument, Bus, Service};

use super::{ADAPTER, BLUEZ, Bluetooth, BluetoothError, DEVICE, PowerState};

// what a device is in the middle of
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Connecting,
    Disconnecting,
    // pairing, then connecting
    Pairing,
    Forgetting,
}

/*
 * bluez answers a connect within this on its own, unless the device keeps
 * it hanging; then amane stops waiting and asks bluez to stop too
 */
const GIVE_UP_AFTER: Duration = Duration::from_secs(45);

// how long a failure stays on its row
const FAILURE_SHOWN_FOR: Duration = Duration::from_secs(10);

// a search the radio dropped is started again at most this often
const RESTART_SCAN_EVERY: Duration = Duration::from_secs(5);

// after an rfkill unblock the radio takes a moment before it can be powered
const UNBLOCK_WAIT: Duration = Duration::from_secs(2);

static NEXT: AtomicU64 = AtomicU64::new(1);

/*
 * which run of an operation is the current one, by device path, so a run the
 * watchdog already ended doesn't end the next one when it finally answers
 */
static RUNNING: Mutex<BTreeMap<String, u64>> = Mutex::new(BTreeMap::new());

// which failure is shown, so an old timer doesn't clear a newer one
static FAILED: Mutex<BTreeMap<String, u64>> = Mutex::new(BTreeMap::new());

// pairings stopped from here, whose "canceled" isn't worth showing
static CANCELED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/*
 * a search was asked for, and is started again if the radio drops it; not
 * in the service, so a view can start one while it reads the service
 */
static SEARCHING: AtomicBool = AtomicBool::new(false);

static LAST_RESTART: Mutex<Option<Instant>> = Mutex::new(None);

impl Bluetooth {
    // turns on with rfkill unblocked first, like turning it on in airplane mode
    pub fn set_powered(powered: bool) {
        worker::run(move || {
            let Some(adapter) = adapter() else {
                return;
            };

            let result = power(&adapter, powered);

            report(&adapter, result);
        });
    }

    // new devices show up in devices() while searching, until stop_scan()
    pub fn start_scan() {
        SEARCHING.store(true, Ordering::Relaxed);

        worker::run(scan);
    }

    pub fn stop_scan() {
        SEARCHING.store(false, Ordering::Relaxed);

        worker::run(|| {
            // bluez refuses this when another program's search is the one running, that's fine
            let _ = adapter_call("StopDiscovery", &[]);
        });
    }

    /*
     * visible to phones and other computers searching, and open to their
     * pairing; bluez turns it off again after its DiscoverableTimeout
     */
    pub fn set_discoverable(discoverable: bool) {
        worker::run(move || {
            let Some(adapter) = adapter() else {
                return;
            };

            let bus = Bus::system();

            let set = |name: &str| {
                let value = Argument::from(discoverable);

                bus.try_set_property(BLUEZ, &adapter, ADAPTER, name, value)
            };

            // pairable first, so a phone that finds it can pair at once
            let order = if discoverable {
                ["Pairable", "Discoverable"]
            } else {
                ["Discoverable", "Pairable"]
            };

            for name in order {
                let _ = set(name);
            }
        });
    }

    pub fn connect(device: &str) {
        run(device, Operation::Connecting, |device, _| {
            device_call(device, "Connect")
        });
    }

    pub fn disconnect(device: &str) {
        run(device, Operation::Disconnecting, |device, _| {
            device_call(device, "Disconnect")
        });
    }

    /*
     * pairs, trusts so it reconnects by itself later, then connects, like
     * other bluetooth menus do; a device that needs a code asks through
     * the agent (serve_agent)
     */
    pub fn pair(device: &str) {
        run(device, Operation::Pairing, |device, token| {
            device_call(device, "Pair")?;

            let _ = set_device(device, "Trusted", true);

            moved_on(device, token, Operation::Connecting);

            // a phone or keyboard with nothing to connect is still paired, that's done
            match device_call(device, "Connect") {
                Err(BluetoothError::NotSupported) => Ok(()),
                result => result,
            }
        });
    }

    // stops a pairing that's waiting, like on a code nobody is going to type
    pub fn cancel_pairing(device: &str) {
        CANCELED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(device.to_string());

        let device = device.to_string();

        thread::spawn(move || {
            let _ = device_call(&device, "CancelPairing");
        });
    }

    // unpairs and removes the device, it comes back only after a new search
    pub fn forget(device: &str) {
        run(device, Operation::Forgetting, |device, _| {
            let path = Argument::Path(device.to_string());

            match adapter_call("RemoveDevice", &[path]) {
                // already gone is what was asked for
                Err(BluetoothError::Gone) => Ok(()),
                result => result,
            }
        });
    }

    // whether it reconnects by itself when it's nearby and on
    pub fn set_trusted(device: &str, trusted: bool) {
        set_later(device, "Trusted", Argument::from(trusted));
    }

    // a blocked device can't connect, and any connection it has ends
    pub fn set_blocked(device: &str, blocked: bool) {
        set_later(device, "Blocked", Argument::from(blocked));
    }

    // the name this computer shows for it, an empty one brings back its own
    pub fn rename(device: &str, name: &str) {
        set_later(device, "Alias", Argument::from(name.trim()));
    }
}

/*
 * runs `job` for `device` on a thread of its own, so a slow connect never
 * holds up another device or the radio switch; a device already busy
 * ignores the new ask, like a second click
 */
fn run(
    device: &str,
    operation: Operation,
    job: impl FnOnce(&str, u64) -> Result<(), BluetoothError> + Send + 'static,
) {
    let Some(token) = begin(device, operation) else {
        return;
    };

    let path = device.to_string();

    thread::spawn(move || {
        let result = job(&path, token);

        finish(&path, token, result);
    });

    let path = device.to_string();

    thread::spawn(move || {
        thread::sleep(GIVE_UP_AFTER);

        if !current(&path, token) {
            return;
        }

        // bluez stops a connect or pairing still going when asked to disconnect
        if operation == Operation::Pairing {
            let _ = device_call(&path, "CancelPairing");
        }

        let _ = device_call(&path, "Disconnect");

        finish(&path, token, Err(BluetoothError::TimedOut));
    });
}

fn begin(device: &str, operation: Operation) -> Option<u64> {
    let mut bluetooth = Bluetooth::write();

    if bluetooth.operations.contains_key(device) {
        bluetooth.quiet();

        return None;
    }

    let token = NEXT.fetch_add(1, Ordering::Relaxed);

    lock(&RUNNING).insert(device.to_string(), token);
    lock(&FAILED).remove(device);
    lock(&CANCELED).remove(device);

    bluetooth.operations.insert(device.to_string(), operation);
    bluetooth.failures.remove(device);

    Some(token)
}

fn current(device: &str, token: u64) -> bool {
    lock(&RUNNING).get(device) == Some(&token)
}

// the same run, on to its next step, like from pairing to connecting
fn moved_on(device: &str, token: u64, operation: Operation) {
    if !current(device, token) {
        return;
    }

    Bluetooth::write()
        .operations
        .insert(device.to_string(), operation);
}

fn finish(device: &str, token: u64, result: Result<(), BluetoothError>) {
    {
        let mut running = lock(&RUNNING);

        // the watchdog ended this run already
        if running.get(device) != Some(&token) {
            return;
        }

        running.remove(device);
    }

    let canceled_here = lock(&CANCELED).remove(device);

    let result = match result {
        Err(BluetoothError::Canceled) if canceled_here => Ok(()),
        result => result,
    };

    let mut bluetooth = Bluetooth::write();

    bluetooth.operations.remove(device);

    // a code shown for this pairing has nothing left to show
    if bluetooth
        .request
        .as_ref()
        .is_some_and(|request| request.device() == device && !request.asks())
    {
        bluetooth.request = None;
    }

    drop(bluetooth);

    if let Err(error) = result {
        fail(device, error);
    }
}

// shows `error` on `device` (or the adapter) for a while
fn fail(device: &str, error: BluetoothError) {
    let token = NEXT.fetch_add(1, Ordering::Relaxed);

    lock(&FAILED).insert(device.to_string(), token);

    Bluetooth::write()
        .failures
        .insert(device.to_string(), error);

    let device = device.to_string();

    thread::spawn(move || {
        thread::sleep(FAILURE_SHOWN_FOR);

        let mut failed = lock(&FAILED);

        if failed.get(&device) != Some(&token) {
            return;
        }

        failed.remove(&device);

        drop(failed);

        Bluetooth::write().failures.remove(&device);
    });
}

fn report(device: &str, result: Result<(), BluetoothError>) {
    if let Err(error) = result {
        fail(device, error);
    }
}

fn power(adapter: &str, powered: bool) -> Result<(), BluetoothError> {
    let bus = Bus::system();

    let set = || {
        bus.try_set_property(BLUEZ, adapter, ADAPTER, "Powered", Argument::from(powered))
            .or_else(ok_if_done)
    };

    if !powered || Bluetooth::read().power() != PowerState::Blocked {
        return set();
    }

    // a hardware switch can't be lifted from here, and then this stays blocked
    unblock().map_err(|_| BluetoothError::Blocked)?;

    let since = Instant::now();

    loop {
        match set() {
            Err(BluetoothError::Blocked) if since.elapsed() < UNBLOCK_WAIT => {
                thread::sleep(Duration::from_millis(100));
            }
            result => return result,
        }
    }
}

/*
 * lifts rfkill's soft block on every bluetooth radio, as `rfkill unblock
 * bluetooth` does: one change-all event written to /dev/rfkill, which the
 * seat's user may write
 */
fn unblock() -> std::io::Result<()> {
    // struct rfkill_event: idx (u32), type, op, soft, hard
    const TYPE_BLUETOOTH: u8 = 2;
    const OP_CHANGE_ALL: u8 = 3;

    let event = [0, 0, 0, 0, TYPE_BLUETOOTH, OP_CHANGE_ALL, 0, 0];

    OpenOptions::new()
        .write(true)
        .open("/dev/rfkill")?
        .write_all(&event)
}

fn scan() {
    // every kind of device, and each one once rather than at every advertisement
    let filter = [
        (String::from("Transport"), Argument::from("auto")),
        (String::from("DuplicateData"), Argument::from(false)),
    ];

    let _ = adapter_call("SetDiscoveryFilter", &[Argument::Map(filter.into())]);

    let _ = adapter_call("StartDiscovery", &[]);
}

pub(super) fn searching() -> bool {
    SEARCHING.load(Ordering::Relaxed)
}

// a search the radio dropped, like when it was turned off and on
pub(super) fn restart_scan() {
    let mut last = lock(&LAST_RESTART);

    if last.is_some_and(|last| last.elapsed() < RESTART_SCAN_EVERY) {
        return;
    }

    *last = Some(Instant::now());

    drop(last);

    worker::run(scan);
}

// looked up again on each call, so input handlers don't have to read the service
fn adapter() -> Option<String> {
    Bluetooth::read()
        .adapter
        .as_ref()
        .map(|adapter| adapter.path().to_string())
}

fn adapter_call(method: &str, arguments: &[Argument]) -> Result<(), BluetoothError> {
    let Some(adapter) = adapter() else {
        return Err(BluetoothError::NotReady);
    };

    Bus::system()
        .try_call(BLUEZ, &adapter, ADAPTER, method, arguments)
        .map(|_| ())
        .or_else(ok_if_done)
}

fn device_call(device: &str, method: &str) -> Result<(), BluetoothError> {
    Bus::system()
        .try_call(BLUEZ, device, DEVICE, method, &[])
        .map(|_| ())
        .or_else(ok_if_done)
}

fn set_device(device: &str, name: &str, value: impl Into<Argument>) -> Result<(), BluetoothError> {
    Bus::system()
        .try_set_property(BLUEZ, device, DEVICE, name, value.into())
        .or_else(ok_if_done)
}

fn set_later(device: &str, name: &'static str, value: Argument) {
    let device = device.to_string();

    worker::run(move || {
        let result = set_device(&device, name, value);

        report(&device, result);
    });
}

// a call that failed only because it was already so counts as done
fn ok_if_done(error: crate::BusError) -> Result<(), BluetoothError> {
    match BluetoothError::from_bus(&error) {
        None => Ok(()),
        Some(error) => Err(error),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
