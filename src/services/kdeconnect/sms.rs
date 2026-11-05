/*
 * the phone's text messages, through kdeconnect's conversations api on the
 * device object: the latest message of every thread, each thread's history
 * as it's asked for, and replies; new messages come by the daemon's signals
 *
 * a service of its own, so the shell, which never reads it, doesn't follow
 * every message; only the messages app does
 *
 * the phone can't be woken: a sleeping one answers when it wakes, and a
 * thread's history fills in as the phone sends it
 */

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::services::worker;
use crate::{Argument, Bus, Service, Value};

use super::contacts::Contacts;
use super::{KDECONNECT, device_path, owner};

const CONVERSATIONS: &str = "org.kde.kdeconnect.device.conversations";
const CONTACTS: &str = "org.kde.kdeconnect.device.contacts";

// how many messages of a thread are asked for at a time
pub const PAGE: i32 = 50;

// a sent message the phone hasn't echoed back yet has a negative id, never a real one
static PENDING: AtomicI64 = AtomicI64::new(0);

// what a message is, as android numbers them
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmsKind {
    Received,
    Sent,
    Draft,
    // sending, or waiting to be sent
    Outbox,
    Failed,
    Queued,
}

impl SmsKind {
    fn from_code(code: f64) -> Self {
        match code as i32 {
            2 => Self::Sent,
            3 => Self::Draft,
            4 => Self::Outbox,
            5 => Self::Failed,
            6 => Self::Queued,
            _ => Self::Received,
        }
    }

    // written on this side of the conversation
    pub fn outgoing(self) -> bool {
        !matches!(self, Self::Received)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SmsMessage {
    // the phone's id for it, negative while it's one sent from here and not echoed yet
    pub(crate) uid: i64,

    pub(crate) thread: i64,

    pub(crate) body: String,

    // the other people in the conversation, as numbers
    pub(crate) addresses: Vec<String>,

    // when it was sent or received, in milliseconds since 1970
    pub(crate) date: i64,

    pub(crate) kind: SmsKind,

    pub(crate) read: bool,

    // pictures and other attachments, which only show as a count
    pub(crate) attachments: usize,
}

impl SmsMessage {
    pub fn uid(&self) -> i64 {
        self.uid
    }

    pub fn thread(&self) -> i64 {
        self.thread
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn addresses(&self) -> &[String] {
        &self.addresses
    }

    pub fn date(&self) -> i64 {
        self.date
    }

    pub fn kind(&self) -> SmsKind {
        self.kind
    }

    pub fn outgoing(&self) -> bool {
        self.kind.outgoing()
    }

    pub fn read(&self) -> bool {
        self.read
    }

    pub fn attachments(&self) -> usize {
        self.attachments
    }

    // sent from here and not seen on the phone yet
    pub fn pending(&self) -> bool {
        self.uid < 0
    }

    /*
     * one message as kdeconnect sends it, the struct (isa(s)xiixixa(xsss)):
     * event, body, addresses, date, type, read, thread, id, sim, attachments
     */
    pub(crate) fn from_value(value: &Value) -> Option<Self> {
        let fields = value.list();

        if fields.len() < 9 {
            return None;
        }

        let addresses = fields[2]
            .list()
            .iter()
            .map(|address| match address {
                // each address is a struct of one string
                Value::List(inner) => inner.first().map_or("", Value::text).to_string(),
                other => other.text().to_string(),
            })
            .filter(|address| !address.is_empty())
            .collect();

        Some(Self {
            uid: fields[7].number() as i64,
            thread: fields[6].number() as i64,
            body: fields[1].text().to_string(),
            addresses,
            date: fields[3].number() as i64,
            kind: SmsKind::from_code(fields[4].number()),
            read: fields[5].number() != 0.0,
            attachments: fields.get(9).map_or(0, |list| list.list().len()),
        })
    }
}

// one conversation, by its latest message
#[derive(Debug, Clone, PartialEq)]
pub struct SmsThread {
    pub(crate) latest: SmsMessage,

    // the contacts' names where the phone knows them, else the numbers
    pub(crate) title: String,
}

impl SmsThread {
    pub fn id(&self) -> i64 {
        self.latest.thread
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn latest(&self) -> &SmsMessage {
        &self.latest
    }

    // the latest message came in and hasn't been read on the phone
    pub fn unread(&self) -> bool {
        !self.latest.read && !self.latest.outgoing()
    }

    pub fn addresses(&self) -> &[String] {
        &self.latest.addresses
    }
}

#[derive(Default)]
pub struct PhoneMessages {
    // the phone whose messages these are
    device: Option<String>,

    // the latest message of each thread, newest first
    threads: Vec<SmsThread>,

    // what's been sent of each thread, oldest first, by thread id
    history: BTreeMap<i64, Vec<SmsMessage>>,

    // threads whose history was asked for and hasn't all come yet
    loading: BTreeSet<i64>,

    contacts: Contacts,
}

impl Service for PhoneMessages {
    fn new() -> Self {
        Self::default()
    }

    fn listen() {
        let bus = Bus::session();

        for signal in bus.signals_from(KDECONNECT) {
            if signal.sender() != owner() {
                continue;
            }

            let tracked = PhoneMessages::read().device.clone();

            // the phone's contacts came: names for the numbers
            if signal.interface() == CONTACTS && signal.name() == "localCacheSynchronized" {
                if let Some(device) = tracked
                    && signal.path() == format!("{}/contacts", device_path(&device))
                {
                    let contacts = Contacts::load(&device);

                    PhoneMessages::write().set_contacts(contacts);
                }

                continue;
            }

            if signal.interface() != CONVERSATIONS {
                continue;
            }

            let tracked = tracked.as_deref().map(device_path);

            if tracked.as_deref() != Some(signal.path()) {
                continue;
            }

            let arguments = signal.arguments();

            match signal.name() {
                "conversationCreated" | "conversationUpdated" => {
                    if let Some(message) = arguments.first().and_then(SmsMessage::from_value) {
                        PhoneMessages::write().ingest(message);
                    }
                }
                "conversationRemoved" => {
                    if let Some(thread) = arguments.first() {
                        PhoneMessages::write().remove(thread.number() as i64);
                    }
                }
                "conversationLoaded" => {
                    if let Some(thread) = arguments.first() {
                        PhoneMessages::write()
                            .loading
                            .remove(&(thread.number() as i64));
                    }
                }
                _ => {}
            }
        }
    }
}

impl PhoneMessages {
    // the phone whose messages are read, if one was opened
    pub fn device(&self) -> Option<&str> {
        self.device.as_deref()
    }

    pub fn threads(&self) -> &[SmsThread] {
        &self.threads
    }

    pub fn thread(&self, id: i64) -> Option<&SmsThread> {
        self.threads.iter().find(|thread| thread.id() == id)
    }

    // a thread's messages, oldest first, as far as they've come
    pub fn messages(&self, thread: i64) -> &[SmsMessage] {
        self.history.get(&thread).map_or(&[], Vec::as_slice)
    }

    pub fn loading(&self, thread: i64) -> bool {
        self.loading.contains(&thread)
    }

    // the contact's name for a number, or the number
    pub fn name<'a>(&'a self, address: &'a str) -> &'a str {
        self.contacts.name(address).unwrap_or(address)
    }

    /*
     * reads `device`'s messages: what the daemon has at once, then the
     * phone's latest of every thread as it sends them
     */
    pub fn open(device: &str) {
        let device = device.to_string();

        worker::run(move || {
            let contacts = Contacts::load(&device);

            let path = device_path(&device);

            let bus = Bus::session();

            let active = bus.call(KDECONNECT, &path, CONVERSATIONS, "activeConversations", &[]);

            {
                let mut messages = PhoneMessages::write();

                if messages.device.as_deref() != Some(device.as_str()) {
                    *messages = PhoneMessages {
                        device: Some(device.clone()),
                        ..PhoneMessages::default()
                    };
                }

                messages.contacts = contacts;

                for message in active.list().iter().filter_map(SmsMessage::from_value) {
                    messages.ingest(message);
                }
            }

            let _ = bus.try_call(
                KDECONNECT,
                &path,
                CONVERSATIONS,
                "requestAllConversationThreads",
                &[],
            );

            // and its contacts, which come by localCacheSynchronized; names show from then on
            let _ = bus.try_call(
                KDECONNECT,
                &format!("{path}/contacts"),
                CONTACTS,
                "synchronizeRemoteWithLocal",
                &[],
            );
        });
    }

    // asks the phone for every thread's latest message again
    pub fn refresh() {
        let Some(device) = PhoneMessages::read().device.clone() else {
            return;
        };

        Self::open(&device);
    }

    // asks for the newest `count` messages of `thread`
    pub fn load_thread(thread: i64, count: i32) {
        let Some(device) = PhoneMessages::write().start_loading(thread) else {
            return;
        };

        worker::run(move || {
            let result = Bus::session().try_call(
                KDECONNECT,
                &device_path(&device),
                CONVERSATIONS,
                "requestConversation",
                &[
                    Argument::from(thread),
                    Argument::from(0),
                    Argument::from(count),
                ],
            );

            // a phone that's gone sends nothing, so stop waiting
            if result.is_err() {
                PhoneMessages::write().loading.remove(&thread);
            }
        });
    }

    /*
     * sends `text` in `thread`; it shows at once as pending, until the phone
     * sends it back as sent (usually within a minute)
     */
    pub fn reply(thread: i64, text: &str) {
        let text = text.trim().to_string();

        if text.is_empty() {
            return;
        }

        let device = {
            let mut messages = PhoneMessages::write();

            let Some(device) = messages.device.clone() else {
                return;
            };

            let addresses = messages
                .thread(thread)
                .map(|thread| thread.addresses().to_vec())
                .unwrap_or_default();

            let uid = PENDING.fetch_sub(1, Ordering::Relaxed) - 1;

            messages.ingest(SmsMessage {
                uid,
                thread,
                body: text.clone(),
                addresses,
                date: now_ms(),
                kind: SmsKind::Outbox,
                read: true,
                attachments: 0,
            });

            device
        };

        worker::run(move || {
            let _ = Bus::session().try_call(
                KDECONNECT,
                &device_path(&device),
                CONVERSATIONS,
                "replyToConversation",
                &[
                    Argument::from(thread),
                    Argument::from(text),
                    Argument::Variants(Vec::new()),
                ],
            );
        });
    }

    // new names for the numbers, on every conversation already shown
    pub(crate) fn set_contacts(&mut self, contacts: Contacts) {
        self.contacts = contacts;

        let titles: Vec<String> = self
            .threads
            .iter()
            .map(|thread| self.title(thread.addresses()))
            .collect();

        for (thread, title) in self.threads.iter_mut().zip(titles) {
            thread.title = title;
        }
    }

    fn start_loading(&mut self, thread: i64) -> Option<String> {
        let device = self.device.clone()?;

        self.loading.insert(thread);

        Some(device)
    }

    /*
     * one message into its thread's history and the thread list; the
     * phone's echo of a message sent from here takes the pending one's place
     */
    pub(crate) fn ingest(&mut self, message: SmsMessage) {
        let history = self.history.entry(message.thread).or_default();

        if !message.pending() {
            if message.outgoing()
                && let Some(at) = history
                    .iter()
                    .position(|old| old.pending() && old.body == message.body)
            {
                history.remove(at);
            }

            history.retain(|old| old.uid != message.uid);
        }

        let at = history.partition_point(|old| old.date <= message.date);

        history.insert(at, message.clone());

        let latest = history.last().cloned().unwrap_or(message);

        let title = self.title(&latest.addresses);

        self.threads.retain(|thread| thread.id() != latest.thread);

        let at = self
            .threads
            .partition_point(|thread| thread.latest.date >= latest.date);

        self.threads.insert(at, SmsThread { latest, title });
    }

    fn remove(&mut self, thread: i64) {
        self.threads.retain(|old| old.id() != thread);

        self.history.remove(&thread);

        self.loading.remove(&thread);
    }

    fn title(&self, addresses: &[String]) -> String {
        if addresses.is_empty() {
            return String::from("Unknown");
        }

        addresses
            .iter()
            .map(|address| self.name(address))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    // a message the way the bus gives it: a struct is a list of its fields
    fn wire(uid: i64, thread: i64, body: &str, date: i64, kind: i32) -> Value {
        Value::List(vec![
            Value::Number(0.0),
            Value::Text(body.into()),
            Value::List(vec![Value::List(vec![Value::Text("+4795723015".into())])]),
            Value::Number(date as f64),
            Value::Number(f64::from(kind)),
            Value::Number(0.0),
            Value::Number(thread as f64),
            Value::Number(uid as f64),
            Value::Number(-1.0),
            Value::List(Vec::new()),
        ])
    }

    fn message(uid: i64, thread: i64, body: &str, date: i64, kind: i32) -> SmsMessage {
        SmsMessage::from_value(&wire(uid, thread, body, date, kind)).unwrap()
    }

    #[test]
    fn reads_the_message_struct() {
        let message = message(7, 3, "hei", 1_700_000_000_000, 1);

        assert_eq!(message.body(), "hei");
        assert_eq!(message.thread(), 3);
        assert_eq!(message.uid(), 7);
        assert_eq!(message.date(), 1_700_000_000_000);
        assert_eq!(message.addresses(), ["+4795723015"]);
        assert_eq!(message.kind(), SmsKind::Received);
        assert!(!message.read());
        assert!(!message.pending());
    }

    #[test]
    fn a_short_struct_is_no_message() {
        assert_eq!(SmsMessage::from_value(&Value::List(Vec::new())), None);
        assert_eq!(SmsMessage::from_value(&Value::Nothing), None);
    }

    #[test]
    fn threads_are_newest_first_and_messages_oldest_first() {
        let mut messages = PhoneMessages::default();

        messages.ingest(message(1, 10, "old", 1_000, 1));
        messages.ingest(message(2, 20, "new", 3_000, 1));
        messages.ingest(message(3, 10, "middle", 2_000, 2));

        let ids: Vec<i64> = messages.threads().iter().map(SmsThread::id).collect();

        assert_eq!(ids, [20, 10]);
        assert_eq!(messages.thread(10).unwrap().latest().body(), "middle");

        let bodies: Vec<&str> = messages.messages(10).iter().map(SmsMessage::body).collect();

        assert_eq!(bodies, ["old", "middle"]);
    }

    #[test]
    fn the_same_message_twice_is_kept_once() {
        let mut messages = PhoneMessages::default();

        messages.ingest(message(1, 10, "hei", 1_000, 1));
        messages.ingest(message(1, 10, "hei", 1_000, 1));

        assert_eq!(messages.messages(10).len(), 1);
    }

    #[test]
    fn the_echo_replaces_the_pending_message() {
        let mut messages = PhoneMessages::default();

        messages.ingest(message(1, 10, "hei", 1_000, 1));
        messages.ingest(SmsMessage {
            uid: -1,
            ..message(0, 10, "on my way", 2_000, 4)
        });

        assert!(messages.messages(10)[1].pending());

        messages.ingest(message(5, 10, "on my way", 2_500, 2));

        let thread = messages.messages(10);

        assert_eq!(thread.len(), 2);
        assert!(!thread[1].pending());
        assert_eq!(thread[1].uid(), 5);
    }

    #[test]
    fn contacts_that_come_later_name_the_conversations() {
        let mut messages = PhoneMessages::default();

        messages.ingest(message(1, 10, "hei", 1_000, 1));

        assert_eq!(messages.thread(10).unwrap().title(), "+4795723015");

        let mut contacts = Contacts::default();
        contacts.add_vcard("BEGIN:VCARD\nFN:Ola Nordmann\nTEL:95723015\nEND:VCARD\n");
        messages.set_contacts(contacts);

        assert_eq!(messages.thread(10).unwrap().title(), "Ola Nordmann");
    }

    #[test]
    fn unread_is_only_for_incoming() {
        let mut messages = PhoneMessages::default();

        messages.ingest(message(1, 10, "hei", 1_000, 1));

        assert!(messages.thread(10).unwrap().unread());

        messages.ingest(message(2, 10, "svar", 2_000, 2));

        assert!(!messages.thread(10).unwrap().unread());
    }
}
