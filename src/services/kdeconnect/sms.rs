/*
 * the phone's text messages, through kdeconnect's conversations api on the
 * device object: the latest message of every thread, each thread's history
 * as it's asked for, replies and new conversations, with attachments both
 * ways; new messages come by the daemon's signals
 *
 * a service of its own, so the shell, which never reads it, doesn't follow
 * every message; only the messages app does
 *
 * the phone can't be woken: a sleeping one answers when it wakes, and a
 * thread's history fills in as the phone sends it
 */

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::services::worker;
use crate::{Argument, Bus, Service, Value};

use super::contacts::{self, Contacts};
use super::{KDECONNECT, device_path, files, owner};

const CONVERSATIONS: &str = "org.kde.kdeconnect.device.conversations";
const CONTACTS: &str = "org.kde.kdeconnect.device.contacts";

// how many messages of a thread are asked for at a time
pub const PAGE: i32 = 50;

// a message sent from here that the phone hasn't sent back by then didn't go
const SEND_TIMEOUT: Duration = Duration::from_secs(120);

// a phone that sent no conversation this long after being asked isn't answering
const ANSWER_TIMEOUT: Duration = Duration::from_secs(20);

// a sent message the phone hasn't echoed back yet has a negative id, never a
// real one, and so has a conversation started here until the phone makes it
static PENDING: AtomicI64 = AtomicI64::new(0);

// each time the phone is asked for its conversations, so an old wait can tell it's stale
static ASKED: AtomicU64 = AtomicU64::new(0);

fn next_pending() -> i64 {
    PENDING.fetch_sub(1, Ordering::Relaxed) - 1
}

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

// how far the phone has answered
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SmsState {
    // nothing asked yet
    #[default]
    Idle,

    // asked, and nothing has come yet
    Syncing,

    // conversations came
    Ready,

    // asked, and nothing came in time: asleep, locked, or nothing to send
    NoAnswer,
}

// a picture, video or file in a message
#[derive(Debug, Clone, PartialEq)]
pub struct SmsAttachment {
    // the phone's ids for it, which fetch the whole file; -1 for one sent from here
    pub(crate) part: i64,
    pub(crate) unique: String,

    pub(crate) mime: String,

    // the small picture the phone sends with the message, written out
    pub(crate) thumbnail: Option<PathBuf>,

    // the file on this computer, for one sent from here
    pub(crate) local: Option<PathBuf>,
}

impl SmsAttachment {
    pub fn part(&self) -> i64 {
        self.part
    }

    pub fn unique(&self) -> &str {
        &self.unique
    }

    pub fn mime(&self) -> &str {
        &self.mime
    }

    pub fn thumbnail(&self) -> Option<&Path> {
        self.thumbnail.as_deref()
    }

    pub fn local(&self) -> Option<&Path> {
        self.local.as_deref()
    }

    pub fn image(&self) -> bool {
        self.mime.starts_with("image/")
    }

    pub fn video(&self) -> bool {
        self.mime.starts_with("video/")
    }

    // a file on this computer, to be sent
    pub(crate) fn from_file(path: &Path) -> Self {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let mime = match extension.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "heic" => "image/heic",
            "mp4" | "m4v" => "video/mp4",
            "3gp" => "video/3gpp",
            "webm" => "video/webm",
            "mp3" => "audio/mpeg",
            "m4a" | "aac" => "audio/aac",
            "ogg" | "opus" => "audio/ogg",
            "vcf" => "text/vcard",
            "pdf" => "application/pdf",
            _ => "application/octet-stream",
        };

        let image = mime.starts_with("image/") && mime != "image/heic";

        Self {
            part: -1,
            unique: path.display().to_string(),
            mime: mime.to_string(),
            thumbnail: image.then(|| path.to_path_buf()),
            local: Some(path.to_path_buf()),
        }
    }

    // one attachment as kdeconnect sends it, the struct (xsss): part, mime, thumbnail, id
    fn from_value(value: &Value) -> Option<Self> {
        let fields = value.list();

        if fields.len() < 4 {
            return None;
        }

        let unique = fields[3].text().to_string();

        let data = fields[2].text();

        let thumbnail = (!data.is_empty() && !unique.is_empty())
            .then(|| {
                files::write_once(
                    "thumbnails",
                    &format!("{}.jpg", files::name_for(&unique)),
                    data,
                )
            })
            .flatten();

        Some(Self {
            part: fields[0].number() as i64,
            unique,
            mime: fields[1].text().to_string(),
            thumbnail,
            local: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SmsMessage {
    // the phone's id for it, negative while it's one sent from here and not echoed yet
    pub(crate) uid: i64,

    pub(crate) thread: i64,

    pub(crate) body: String,

    // the other people in the conversation, as numbers; for one received
    // in a group, the one who sent it comes first
    pub(crate) addresses: Vec<String>,

    // when it was sent or received, in milliseconds since 1970
    pub(crate) date: i64,

    pub(crate) kind: SmsKind,

    pub(crate) read: bool,

    pub(crate) attachments: Vec<SmsAttachment>,

    // sent from here, and the phone never sent it back
    pub(crate) lost: bool,
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

    // who sent it: the first address of one received
    pub fn sender(&self) -> Option<&str> {
        (!self.outgoing())
            .then(|| self.addresses.first().map(String::as_str))
            .flatten()
    }

    pub fn date(&self) -> i64 {
        self.date
    }

    // a message sent from here that never came back reads as failed
    pub fn kind(&self) -> SmsKind {
        if self.lost {
            SmsKind::Failed
        } else {
            self.kind
        }
    }

    pub fn outgoing(&self) -> bool {
        self.kind.outgoing()
    }

    pub fn read(&self) -> bool {
        self.read
    }

    pub fn attachments(&self) -> &[SmsAttachment] {
        &self.attachments
    }

    // sent from here and not seen on the phone yet
    pub fn pending(&self) -> bool {
        self.uid < 0 && !self.lost
    }

    // sent from here: still waiting, or given up on
    pub fn local(&self) -> bool {
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

        let attachments = fields
            .get(9)
            .map(|list| {
                list.list()
                    .iter()
                    .filter_map(SmsAttachment::from_value)
                    .collect()
            })
            .unwrap_or_default();

        Some(Self {
            uid: fields[7].number() as i64,
            thread: fields[6].number() as i64,
            body: fields[1].text().to_string(),
            addresses,
            date: fields[3].number() as i64,
            kind: SmsKind::from_code(fields[4].number()),
            read: fields[5].number() != 0.0,
            attachments,
            lost: false,
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

    // more than one other person in it
    pub fn group(&self) -> bool {
        self.latest.addresses.len() > 1
    }

    // started here, and the phone hasn't made it a conversation yet
    pub fn started_here(&self) -> bool {
        self.id() < 0
    }
}

// a whole attachment asked for
#[derive(Debug, Clone, PartialEq)]
enum Download {
    Waiting,
    Done(PathBuf),
}

#[derive(Default)]
pub struct PhoneMessages {
    // the phone whose messages these are
    device: Option<String>,

    state: SmsState,

    // the latest message of each thread, newest first
    threads: Vec<SmsThread>,

    // what's been sent of each thread, oldest first, by thread id
    history: BTreeMap<i64, Vec<SmsMessage>>,

    // threads whose history was asked for and hasn't all come yet
    loading: BTreeSet<i64>,

    // conversations started here, and the phone's thread each became
    moved: BTreeMap<i64, i64>,

    // whole attachments, by their unique id, and those to open once they come
    downloads: BTreeMap<String, Download>,
    opening: BTreeSet<String>,

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
                // the file's path, and its name, which is the attachment's unique id
                "attachmentReceived" => {
                    let path = arguments.first().map_or("", Value::text);
                    let unique = arguments.get(1).map_or("", Value::text);

                    if !path.is_empty() {
                        PhoneMessages::write().downloaded_to(unique, PathBuf::from(path));
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

    pub fn state(&self) -> SmsState {
        self.state
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

    // the phone's thread a conversation started here became, once it has
    pub fn moved(&self, from: i64) -> Option<i64> {
        self.moved.get(&from).copied()
    }

    // the contact's name for a number, or the number
    pub fn name<'a>(&'a self, address: &'a str) -> &'a str {
        self.contacts.name(address).unwrap_or(address)
    }

    // the contact's photo for a number, where the phone has one
    pub fn photo(&self, address: &str) -> Option<&Path> {
        self.contacts.photo(address)
    }

    // every contact's name and number, one pair per number, for picking whom to write to
    pub fn contacts(&self) -> Vec<(String, String)> {
        self.contacts
            .people()
            .iter()
            .flat_map(|person| {
                person
                    .numbers
                    .iter()
                    .map(|number| (person.name.clone(), number.clone()))
            })
            .collect()
    }

    // the conversation with exactly these people, if there is one
    pub fn thread_with(&self, addresses: &[String]) -> Option<i64> {
        self.threads
            .iter()
            .find(|thread| same_people(thread.addresses(), addresses))
            .map(SmsThread::id)
    }

    // whether the whole attachment is being fetched
    pub fn downloading(&self, unique: &str) -> bool {
        matches!(self.downloads.get(unique), Some(Download::Waiting))
    }

    pub fn downloaded(&self, unique: &str) -> Option<&Path> {
        match self.downloads.get(unique) {
            Some(Download::Done(path)) => Some(path),
            _ => None,
        }
    }

    /*
     * reads `device`'s messages: what the daemon has at once, then the
     * phone's latest of every thread as it sends them
     */
    pub fn open(device: &str) {
        let device = device.to_string();

        let asked = ASKED.fetch_add(1, Ordering::Relaxed) + 1;

        {
            let mut messages = PhoneMessages::write();

            if messages.device.as_deref() != Some(device.as_str()) {
                *messages = PhoneMessages {
                    device: Some(device.clone()),
                    ..PhoneMessages::default()
                };
            }

            if messages.threads.is_empty() {
                messages.state = SmsState::Syncing;
            }
        }

        // nothing in time: say so, rather than wait on a phone that sleeps
        thread::spawn(move || {
            thread::sleep(ANSWER_TIMEOUT);

            if ASKED.load(Ordering::Relaxed) != asked {
                return;
            }

            if PhoneMessages::read().state == SmsState::Syncing {
                PhoneMessages::write().state = SmsState::NoAnswer;
            }
        });

        worker::run(move || {
            let contacts = Contacts::load(&device);

            let path = device_path(&device);

            let bus = Bus::session();

            let active = bus.call(KDECONNECT, &path, CONVERSATIONS, "activeConversations", &[]);

            {
                let mut messages = PhoneMessages::write();

                if messages.device.as_deref() != Some(device.as_str()) {
                    return;
                }

                messages.set_contacts(contacts);

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
        if thread < 0 {
            return;
        }

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
     * sends `text` and `files` in `thread`; it shows at once as pending,
     * until the phone sends it back as sent (usually within a minute), or
     * as failed when it doesn't
     */
    pub fn reply(thread: i64, text: &str, files: &[PathBuf]) {
        let text = text.trim().to_string();

        if text.is_empty() && files.is_empty() {
            return;
        }

        let (device, addresses) = {
            let mut messages = PhoneMessages::write();

            let Some(device) = messages.device.clone() else {
                return;
            };

            let addresses = messages
                .thread(thread)
                .map(|thread| thread.addresses().to_vec())
                .unwrap_or_default();

            messages.add_pending(thread, &text, files, addresses.clone());

            (device, addresses)
        };

        send(device, Some(thread), addresses, text, files.to_vec());
    }

    /*
     * writes to `addresses`: in their conversation if there's one, else in
     * a new one, which shows at once and becomes the phone's when it sends
     * the message back (`moved`); the thread to show
     */
    pub fn new_conversation(addresses: &[String], text: &str, files: &[PathBuf]) -> Option<i64> {
        let text = text.trim().to_string();

        if addresses.is_empty() || (text.is_empty() && files.is_empty()) {
            return None;
        }

        let existing = PhoneMessages::read().thread_with(addresses);

        if let Some(thread) = existing {
            Self::reply(thread, &text, files);

            return Some(thread);
        }

        let thread = next_pending();

        let device = {
            let mut messages = PhoneMessages::write();

            let device = messages.device.clone()?;

            messages.add_pending(thread, &text, files, addresses.to_vec());

            device
        };

        send(device, None, addresses.to_vec(), text, files.to_vec());

        Some(thread)
    }

    // sends a failed message again
    pub fn retry(uid: i64) {
        let Some(message) = PhoneMessages::write().take_local(uid) else {
            return;
        };

        let files: Vec<PathBuf> = message
            .attachments
            .iter()
            .filter_map(|attachment| attachment.local.clone())
            .collect();

        if message.thread < 0 {
            let _ = Self::new_conversation(&message.addresses, &message.body, &files);
        } else {
            Self::reply(message.thread, &message.body, &files);
        }
    }

    // forgets a failed message
    pub fn discard(uid: i64) {
        let _ = PhoneMessages::write().take_local(uid);
    }

    /*
     * fetches an attachment's whole file from the phone and opens it once
     * it's here; the daemon keeps it, so the second time it opens at once
     */
    pub fn download(attachment: &SmsAttachment) {
        if let Some(local) = &attachment.local {
            open_file(local);

            return;
        }

        let unique = attachment.unique.clone();

        let part = attachment.part;

        let device = {
            let mut messages = PhoneMessages::write();

            if let Some(Download::Done(path)) = messages.downloads.get(&unique) {
                open_file(path);

                return;
            }

            let Some(device) = messages.device.clone() else {
                return;
            };

            messages.downloads.insert(unique.clone(), Download::Waiting);
            messages.opening.insert(unique.clone());

            device
        };

        worker::run(move || {
            let result = Bus::session().try_call(
                KDECONNECT,
                &device_path(&device),
                CONVERSATIONS,
                "requestAttachmentFile",
                &[Argument::Long(part), Argument::from(unique.clone())],
            );

            if result.is_err() {
                let mut messages = PhoneMessages::write();

                messages.downloads.remove(&unique);
                messages.opening.remove(&unique);
            }
        });
    }

    fn downloaded_to(&mut self, unique: &str, path: PathBuf) {
        if self.opening.remove(unique) {
            open_file(&path);
        }

        self.downloads
            .insert(unique.to_string(), Download::Done(path));
    }

    // a message sent from here, shown until the phone sends it back
    fn add_pending(&mut self, thread: i64, text: &str, files: &[PathBuf], addresses: Vec<String>) {
        let uid = next_pending();

        self.ingest(SmsMessage {
            uid,
            thread,
            body: text.to_string(),
            addresses,
            date: now_ms(),
            kind: SmsKind::Outbox,
            read: true,
            attachments: files
                .iter()
                .map(|path| SmsAttachment::from_file(path))
                .collect(),
            lost: false,
        });

        // not back in time: it failed, and can be sent again
        thread::spawn(move || {
            thread::sleep(SEND_TIMEOUT);

            let waiting = PhoneMessages::read()
                .history
                .values()
                .flatten()
                .any(|message| message.uid == uid && message.pending());

            if waiting {
                PhoneMessages::write().lose(uid);
            }
        });
    }

    fn lose(&mut self, uid: i64) {
        let Some(thread) = self.history.iter_mut().find_map(|(thread, history)| {
            let message = history.iter_mut().find(|message| message.uid == uid)?;

            message.lost = true;

            Some(*thread)
        }) else {
            return;
        };

        self.relist(thread);
    }

    // takes a message sent from here out of its thread
    fn take_local(&mut self, uid: i64) -> Option<SmsMessage> {
        if uid >= 0 {
            return None;
        }

        let (thread, message) = self.history.iter_mut().find_map(|(thread, history)| {
            let at = history.iter().position(|message| message.uid == uid)?;

            Some((*thread, history.remove(at)))
        })?;

        self.relist(thread);

        Some(message)
    }

    // the thread list after its history changed: its latest message, or gone when it's empty
    fn relist(&mut self, thread: i64) {
        let latest = self
            .history
            .get(&thread)
            .and_then(|history| history.last().cloned());

        self.threads.retain(|old| old.id() != thread);

        match latest {
            Some(latest) => {
                let title = self.title(&latest.addresses);

                let at = self
                    .threads
                    .partition_point(|old| old.latest.date >= latest.date);

                self.threads.insert(at, SmsThread { latest, title });
            }
            None => {
                self.history.remove(&thread);
            }
        }
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
     * phone's echo of a message sent from here takes the pending one's
     * place, and the first echo of a conversation started here moves it to
     * the phone's thread
     */
    pub(crate) fn ingest(&mut self, message: SmsMessage) {
        if !message.local() {
            self.state = SmsState::Ready;

            if message.outgoing() {
                self.match_echo(&message);
            }
        }

        let thread = message.thread;

        let history = self.history.entry(thread).or_default();

        if !message.local() {
            history.retain(|old| old.uid != message.uid);
        }

        let at = history.partition_point(|old| old.date <= message.date);

        history.insert(at, message);

        self.relist(thread);
    }

    // the message sent from here that `echo` is: dropped, its thread moved when it was started here
    fn match_echo(&mut self, echo: &SmsMessage) {
        let same = |old: &SmsMessage| {
            old.local()
                && (old.body == echo.body || (old.body.is_empty() && !echo.attachments.is_empty()))
        };

        if let Some(history) = self.history.get_mut(&echo.thread)
            && let Some(at) = history.iter().position(&same)
        {
            history.remove(at);

            return;
        }

        let started = self.history.iter_mut().find_map(|(thread, history)| {
            if *thread >= 0 {
                return None;
            }

            let at = history
                .iter()
                .position(|old| same(old) && same_people(&old.addresses, &echo.addresses))?;

            history.remove(at);

            Some(*thread)
        });

        if let Some(started) = started {
            // what else was written there before the phone answered goes along
            let rest = self.history.remove(&started).unwrap_or_default();

            self.threads.retain(|old| old.id() != started);

            self.moved.insert(started, echo.thread);

            let history = self.history.entry(echo.thread).or_default();

            for mut message in rest {
                message.thread = echo.thread;

                let at = history.partition_point(|old| old.date <= message.date);

                history.insert(at, message);
            }
        }
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

// the same people, in any order, their numbers written any way
fn same_people(a: &[String], b: &[String]) -> bool {
    a.len() == b.len()
        && a.iter().all(|one| {
            let one = contacts::digits(one);

            b.iter()
                .any(|other| contacts::same_number(&one, &contacts::digits(other)))
        })
}

// to the phone: a reply in `thread`, or a new conversation with `addresses`
fn send(
    device: String,
    thread: Option<i64>,
    addresses: Vec<String>,
    text: String,
    files: Vec<PathBuf>,
) {
    // the daemon reads each attachment as a plain path
    let files = Argument::Variants(
        files
            .iter()
            .map(|path| Argument::from(path.display().to_string()))
            .collect(),
    );

    worker::run(move || {
        let path = device_path(&device);

        let bus = Bus::session();

        let _ = match thread.filter(|thread| *thread >= 0) {
            Some(thread) => bus.try_call(
                KDECONNECT,
                &path,
                CONVERSATIONS,
                "replyToConversation",
                &[Argument::from(thread), Argument::from(text), files],
            ),
            // each address is a struct of one string
            None => bus.try_call(
                KDECONNECT,
                &path,
                CONVERSATIONS,
                "sendWithoutConversation",
                &[
                    Argument::Variants(
                        addresses
                            .into_iter()
                            .map(|address| Argument::Struct(vec![Argument::from(address)]))
                            .collect(),
                    ),
                    Argument::from(text),
                    files,
                ],
            ),
        };
    });
}

fn open_file(path: &Path) {
    crate::spawn(&format!(
        "xdg-open {}",
        files::quoted(&path.display().to_string())
    ));
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

    #[test]
    fn reads_attachments() {
        let mut fields = wire(7, 3, "", 1_000, 1).list().to_vec();

        fields[9] = Value::List(vec![Value::List(vec![
            Value::Number(42.0),
            Value::Text("image/jpeg".into()),
            Value::Text(String::new()),
            Value::Text("PART_1700000000_image.jpg".into()),
        ])]);

        let message = SmsMessage::from_value(&Value::List(fields)).unwrap();

        let attachment = &message.attachments()[0];

        assert_eq!(attachment.part(), 42);
        assert_eq!(attachment.unique(), "PART_1700000000_image.jpg");
        assert!(attachment.image());
        assert_eq!(attachment.thumbnail(), None);
    }

    #[test]
    fn a_conversation_started_here_moves_to_the_phones() {
        let mut messages = PhoneMessages::default();

        let started = next_pending();

        messages.add_pending(started, "hei", &[], vec!["+47 957 23 015".into()]);

        assert!(messages.thread(started).unwrap().started_here());

        // the phone sends it back in a thread of its own, the number written its way
        messages.ingest(message(9, 77, "hei", 2_000, 2));

        assert_eq!(messages.moved(started), Some(77));
        assert!(messages.thread(started).is_none());
        assert_eq!(messages.messages(77).len(), 1);
        assert!(!messages.messages(77)[0].local());
    }

    #[test]
    fn a_message_never_sent_back_fails_and_goes_again() {
        let mut messages = PhoneMessages::default();

        messages.ingest(message(1, 10, "hei", 1_000, 1));
        messages.add_pending(10, "svar", &[], vec!["+4795723015".into()]);

        let uid = messages.messages(10)[1].uid();

        messages.lose(uid);

        let lost = &messages.messages(10)[1];

        assert_eq!(lost.kind(), SmsKind::Failed);
        assert!(!lost.pending());
        assert_eq!(
            messages.thread(10).unwrap().latest().kind(),
            SmsKind::Failed
        );

        // taken out to send again
        let taken = messages.take_local(uid).unwrap();

        assert_eq!(taken.body(), "svar");
        assert_eq!(messages.messages(10).len(), 1);
        assert_eq!(messages.thread(10).unwrap().latest().body(), "hei");

        // a late echo still takes a failed one's place
        messages.add_pending(10, "igjen", &[], Vec::new());
        let uid = messages.messages(10)[1].uid();
        messages.lose(uid);
        messages.ingest(message(5, 10, "igjen", 3_000, 2));

        assert_eq!(messages.messages(10).len(), 2);
        assert_eq!(messages.messages(10)[1].kind(), SmsKind::Sent);
    }

    #[test]
    fn the_same_people_however_their_numbers_are_written() {
        assert!(same_people(
            &["+4795723015".into(), "22334455".into()],
            &["22 33 44 55".into(), "957 23 015".into()],
        ));
        assert!(!same_people(&["95723015".into()], &["22334455".into()]));
    }

    #[test]
    fn a_file_sent_from_here_knows_its_kind() {
        let photo = SmsAttachment::from_file(Path::new("/tmp/Bilde.JPG"));

        assert!(photo.image());
        assert_eq!(photo.thumbnail(), Some(Path::new("/tmp/Bilde.JPG")));

        let other = SmsAttachment::from_file(Path::new("/tmp/notes"));

        assert_eq!(other.mime(), "application/octet-stream");
        assert_eq!(other.thumbnail(), None);
    }
}
