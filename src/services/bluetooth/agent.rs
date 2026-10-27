use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, Once, PoisonError};
use std::thread;

use crate::services::worker;
use crate::{Argument, Bus, Method, Service, Value};

use super::{BLUEZ, Bluetooth};

/*
 * amane's pairing agent: bluez asks it for codes and confirmations while a
 * device pairs, here or from the device's side; it is on bluez's system
 * bus under amane's own connection
 */
const PATH: &str = "/org/amane/bluetooth/agent";
const AGENT: &str = "org.bluez.Agent1";
const AGENT_MANAGER: &str = "org.bluez.AgentManager1";

// can show a code and take one typed, so every kind of device can pair
const CAPABILITY: &str = "KeyboardDisplay";

const REJECTED: &str = "org.bluez.Error.Rejected";
const CANCELED: &str = "org.bluez.Error.Canceled";

static NEXT: AtomicU64 = AtomicU64::new(1);

// the call bluez is waiting on an answer to, with the id of its request
static WAITING: Mutex<Option<(u64, Method)>> = Mutex::new(None);

// what bluez is asking about one device
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingRequest {
    pub(crate) id: u64,

    // the device's path
    pub(crate) device: String,

    pub(crate) kind: PairingKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingKind {
    // both sides show this six-digit number, pair if they match
    Confirm(u32),

    // a PIN to type, for older devices; often 0000
    EnterPin,

    // the six-digit number the device shows, to type here
    EnterPasskey,

    // a PIN to type on the device, like a keyboard
    DisplayPin(String),

    // a number to type on the device, and how many digits it has so far
    DisplayPasskey { passkey: u32, entered: u16 },

    // a device asking to pair with no code at all
    Authorize,

    // a device that isn't trusted asking to use a service, by its UUID
    AuthorizeService(String),
}

pub enum PairingAnswer {
    Accept,
    Reject,
    Pin(String),
    Passkey(u32),
}

impl PairingRequest {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn device(&self) -> &str {
        &self.device
    }

    pub fn kind(&self) -> &PairingKind {
        &self.kind
    }

    // whether it waits for an answer, rather than only showing a code
    pub fn asks(&self) -> bool {
        !matches!(
            self.kind,
            PairingKind::DisplayPin(_) | PairingKind::DisplayPasskey { .. }
        )
    }
}

impl Bluetooth {
    /*
     * answers bluez's pairing questions through pairing_request() and
     * answer(); only one program should, so a shell calls this once and
     * other amane programs leave it alone
     */
    pub fn serve_agent() {
        static SERVE: Once = Once::new();

        SERVE.call_once(|| {
            thread::spawn(serve);
        });
    }

    // answers the request with this id; an old id, already answered or canceled, does nothing
    pub fn answer(id: u64, answer: PairingAnswer) {
        let waiting = {
            let mut waiting = WAITING.lock().unwrap_or_else(PoisonError::into_inner);

            match waiting.as_ref() {
                Some((waiting_id, _)) if *waiting_id == id => waiting.take(),
                _ => None,
            }
        };

        // off the caller's thread, which may be drawing with the service read
        worker::run(move || clear(id));

        let Some((_, method)) = waiting else {
            return;
        };

        match answer {
            PairingAnswer::Accept => method.reply(&[]),
            PairingAnswer::Reject => method.reply_error(REJECTED, "Rejected by the user"),
            PairingAnswer::Pin(pin) => method.reply(&[Argument::from(pin)]),
            PairingAnswer::Passkey(passkey) => method.reply(&[Argument::from(passkey)]),
        }
    }
}

fn serve() {
    // watched before registering, so bluez's first call isn't missed
    let methods = Bus::system().methods(PATH, AGENT);

    register();

    for method in methods {
        handle(method);
    }
}

// bluez forgets its agents when it restarts
pub(super) fn bluez_restarted() {
    let waiting = WAITING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();

    drop(waiting);

    Bluetooth::write().request = None;

    if SERVING.load(Ordering::Relaxed) {
        register();
    }
}

static SERVING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn register() {
    SERVING.store(true, Ordering::Relaxed);

    let bus = Bus::system();

    let path = Argument::Path(String::from(PATH));

    // AlreadyExists after a restart of amane's side only, the registration stands
    let _ = bus.try_call(
        BLUEZ,
        "/org/bluez",
        AGENT_MANAGER,
        "RegisterAgent",
        &[path.clone(), Argument::from(CAPABILITY)],
    );

    let _ = bus.try_call(
        BLUEZ,
        "/org/bluez",
        AGENT_MANAGER,
        "RequestDefaultAgent",
        &[path],
    );
}

fn handle(method: Method) {
    let arguments = method.arguments();

    let device = arguments
        .first()
        .map(Value::text)
        .unwrap_or_default()
        .to_string();

    let kind = match (method.name(), arguments) {
        ("RequestConfirmation", [_, passkey, ..]) => PairingKind::Confirm(passkey.number() as u32),
        ("RequestPinCode", _) => PairingKind::EnterPin,
        ("RequestPasskey", _) => PairingKind::EnterPasskey,
        ("RequestAuthorization", _) => PairingKind::Authorize,
        ("AuthorizeService", [_, uuid, ..]) => {
            // bluez only asks for devices it doesn't trust; a paired one is let in
            let paired = Bluetooth::read()
                .device(&device)
                .is_some_and(|device| device.paired());

            if paired {
                method.reply(&[]);

                return;
            }

            PairingKind::AuthorizeService(uuid.text().to_string())
        }
        ("DisplayPinCode", [_, pin, ..]) => PairingKind::DisplayPin(pin.text().to_string()),
        ("DisplayPasskey", [_, passkey, entered, ..]) => PairingKind::DisplayPasskey {
            passkey: passkey.number() as u32,
            entered: entered.number() as u16,
        },
        ("Cancel", _) => {
            cancel();

            method.reply(&[]);

            return;
        }
        // bluez let go of the agent, like when another took over
        ("Release", _) => {
            cancel();

            SERVING.store(false, Ordering::Relaxed);

            method.reply(&[]);

            return;
        }
        _ => {
            method.reply_error(REJECTED, "Unknown request");

            return;
        }
    };

    show(method, device, kind);
}

fn show(method: Method, device: String, kind: PairingKind) {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);

    let request = PairingRequest { id, device, kind };

    let mut waiting = WAITING.lock().unwrap_or_else(PoisonError::into_inner);

    // a code only to show needs no answer; while bluez waits on one, a second request waits its turn
    if !request.asks() {
        method.reply(&[]);
    } else if waiting.is_some() {
        method.reply_error(REJECTED, "Another pairing is waiting");

        return;
    } else {
        *waiting = Some((id, method));
    }

    drop(waiting);

    Bluetooth::write().request = Some(request);
}

// bluez stopped asking, like when the device gave up or pairing timed out
fn cancel() {
    let waiting = WAITING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();

    if let Some((_, method)) = waiting {
        method.reply_error(CANCELED, "Canceled");
    }

    let mut bluetooth = Bluetooth::write();

    if bluetooth.request.is_none() {
        bluetooth.quiet();
    }

    bluetooth.request = None;
}

fn clear(id: u64) {
    let mut bluetooth = Bluetooth::write();

    if bluetooth
        .request
        .as_ref()
        .is_some_and(|request| request.id == id)
    {
        bluetooth.request = None;
    } else {
        bluetooth.quiet();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_code_to_show_goes_unanswered() {
        let request = |kind| PairingRequest {
            id: 1,
            device: String::new(),
            kind,
        };

        assert!(request(PairingKind::Confirm(123456)).asks());
        assert!(request(PairingKind::EnterPin).asks());
        assert!(!request(PairingKind::DisplayPin("0000".into())).asks());
        assert!(
            !request(PairingKind::DisplayPasskey {
                passkey: 1,
                entered: 0
            })
            .asks()
        );
    }
}
