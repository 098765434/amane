use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, Once, PoisonError};
use std::thread;

use crate::services::worker;
use crate::{Argument, Bus, BusError};

use super::protocol::{
    ActivityContent, ActivityEvent, BUS_NAME, Dismiss, INTERFACE, ItemEvent, ItemSpec, PATH, encode,
};

type Handler<T> = Arc<dyn Fn(T) + Send + Sync>;

struct Activity {
    content: ActivityContent,

    // the bar's number for it, none until it answered or while no bar runs
    handle: Option<u32>,

    // ended here: it isn't started again when the bar comes back
    ended: bool,

    on_event: Option<Handler<ActivityEvent>>,
}

#[derive(Default)]
struct Client {
    items: HashMap<String, ItemSpec>,
    item_handlers: HashMap<String, Handler<ItemEvent>>,

    activities: HashMap<u64, Activity>,
    next: u64,
}

// what this program has in the bar, kept so it can be sent again when the bar restarts
static CLIENT: LazyLock<Mutex<Client>> = LazyLock::new(Mutex::default);

static LISTENING: Once = Once::new();

fn client() -> MutexGuard<'static, Client> {
    CLIENT.lock().unwrap_or_else(PoisonError::into_inner)
}

fn call(method: &str, arguments: &[Argument]) -> Result<crate::Value, BusError> {
    Bus::session().try_call(BUS_NAME, PATH, INTERFACE, method, arguments)
}

/*
 * a program's own item in the bar:
 *
 * let item = StatusItem::register(ItemSpec::new("org.amane.drop", icon));
 * item.on_event(|event| ...);
 *
 * calls go out on amane's worker in order, and events come in on a thread of
 * their own, so a handler that touches a window writes a Service and lets
 * that redraw. When the bar restarts, every item is registered again
 */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusItem {
    id: String,
}

impl StatusItem {
    pub fn register(spec: ItemSpec) -> Self {
        listen();

        let id = spec.id.clone();

        client().items.insert(id.clone(), spec.clone());

        worker::run(move || {
            let _ = call("RegisterItem", &[Argument::Text(encode(&spec))]);
        });

        Self { id }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    // the whole item again; its id stays the one it was registered with
    pub fn update(&self, mut spec: ItemSpec) {
        spec.id = self.id.clone();

        client().items.insert(self.id.clone(), spec.clone());

        worker::run(move || {
            let _ = call("UpdateItem", &[Argument::Text(encode(&spec))]);
        });
    }

    // what it shows now, to change a part of it
    pub fn spec(&self) -> Option<ItemSpec> {
        client().items.get(&self.id).cloned()
    }

    pub fn on_event(&self, handler: impl Fn(ItemEvent) + Send + Sync + 'static) {
        client()
            .item_handlers
            .insert(self.id.clone(), Arc::new(handler));
    }

    pub fn remove(self) {
        {
            let mut client = client();

            client.items.remove(&self.id);
            client.item_handlers.remove(&self.id);
        }

        let id = self.id;

        worker::run(move || {
            let _ = call("RemoveItem", &[Argument::Text(id)]);
        });
    }
}

/*
 * an activity in the bar, something going on now: started, updated as it goes,
 * and ended. An update that alerts briefly shows the expanded view
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveActivity {
    local: u64,
}

impl LiveActivity {
    pub fn start(content: ActivityContent) -> Self {
        listen();

        let local = {
            let mut client = client();

            client.next += 1;

            let local = client.next;

            client.activities.insert(
                local,
                Activity {
                    content: content.clone(),
                    handle: None,
                    ended: false,
                    on_event: None,
                },
            );

            local
        };

        worker::run(move || start_on_bar(local, &content));

        Self { local }
    }

    pub fn update(&self, content: ActivityContent, alert: bool) {
        {
            let mut client = client();

            let Some(activity) = client.activities.get_mut(&self.local) else {
                return;
            };

            if activity.ended {
                return;
            }

            activity.content = content.clone();
        }

        let local = self.local;

        // after start's call on the same worker, so its handle is known by now
        worker::run(move || {
            let Some(handle) = handle_of(local) else {
                return;
            };

            let _ = call(
                "UpdateActivity",
                &[
                    Argument::Unsigned(handle),
                    Argument::Text(encode(&content)),
                    Argument::Bool(alert),
                ],
            );
        });
    }

    // what it shows now, to change a part of it; none once it's gone
    pub fn content(&self) -> Option<ActivityContent> {
        let client = client();

        let activity = client.activities.get(&self.local)?;

        (!activity.ended).then(|| activity.content.clone())
    }

    pub fn on_event(&self, handler: impl Fn(ActivityEvent) + Send + Sync + 'static) {
        if let Some(activity) = client().activities.get_mut(&self.local) {
            activity.on_event = Some(Arc::new(handler));
        }
    }

    // its Dismissed event still comes, once it has left the bar
    pub fn end(self, dismiss: Dismiss) {
        {
            let mut client = client();

            let Some(activity) = client.activities.get_mut(&self.local) else {
                return;
            };

            activity.ended = true;
        }

        let local = self.local;

        worker::run(move || {
            let Some(handle) = handle_of(local) else {
                client().activities.remove(&local);

                return;
            };

            let _ = call(
                "EndActivity",
                &[Argument::Unsigned(handle), Argument::Text(encode(&dismiss))],
            );
        });
    }
}

fn handle_of(local: u64) -> Option<u32> {
    client().activities.get(&local)?.handle
}

fn start_on_bar(local: u64, content: &ActivityContent) {
    let Ok(reply) = call("StartActivity", &[Argument::Text(encode(content))]) else {
        return;
    };

    let handle = reply.number() as u32;

    if let Some(activity) = client().activities.get_mut(&local) {
        activity.handle = Some(handle);
    }
}

// everything this program had in the bar, sent again to a bar that just started
fn register_again() {
    let (items, activities) = {
        let mut client = client();

        let items: Vec<ItemSpec> = client.items.values().cloned().collect();

        client.activities.retain(|_, activity| !activity.ended);

        let activities: Vec<(u64, ActivityContent)> = client
            .activities
            .iter_mut()
            .map(|(local, activity)| {
                activity.handle = None;

                (*local, activity.content.clone())
            })
            .collect();

        (items, activities)
    };

    for spec in items {
        let _ = call("RegisterItem", &[Argument::Text(encode(&spec))]);
    }

    for (local, content) in activities {
        start_on_bar(local, &content);
    }
}

// the bar quit: nothing it numbered means anything now
fn forget_handles() {
    let mut client = client();

    client.activities.retain(|_, activity| !activity.ended);

    for activity in client.activities.values_mut() {
        activity.handle = None;
    }
}

fn listen() {
    LISTENING.call_once(|| {
        thread::spawn(|| {
            for signal in Bus::session().signals_from(BUS_NAME) {
                if signal.interface() != INTERFACE {
                    continue;
                }

                let arguments = signal.arguments();

                match (signal.name(), arguments) {
                    ("ItemEvent", [id, event]) => item_event(id.text(), event.text()),
                    ("ActivityEvent", [handle, event]) => {
                        activity_event(handle.number() as u32, event.text());
                    }
                    _ => {}
                }
            }
        });

        thread::spawn(|| {
            for signal in Bus::session().owner_changes(BUS_NAME) {
                let new_owner = signal
                    .arguments()
                    .get(2)
                    .map(|owner| owner.text().to_owned())
                    .unwrap_or_default();

                if new_owner.is_empty() {
                    worker::run(forget_handles);
                } else {
                    worker::run(register_again);
                }
            }
        });
    });
}

fn item_event(id: &str, json: &str) {
    let Ok(event) = ItemEvent::decode(json) else {
        return;
    };

    let handler = client().item_handlers.get(id).cloned();

    // run without the lock held, so a handler may update its item
    if let Some(handler) = handler {
        handler(event);
    }
}

fn activity_event(handle: u32, json: &str) {
    let Ok(event) = ActivityEvent::decode(json) else {
        return;
    };

    let handler = {
        let mut client = client();

        let Some((&local, activity)) = client
            .activities
            .iter()
            .find(|(_, activity)| activity.handle == Some(handle))
        else {
            return;
        };

        let handler = activity.on_event.clone();

        if event == ActivityEvent::Dismissed {
            client.activities.remove(&local);
        }

        handler
    };

    if let Some(handler) = handler {
        handler(event);
    }
}
