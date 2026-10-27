use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, Once, PoisonError};
use std::thread;

use crate::services::worker;
use crate::{Argument, Bus, Method, Service, Value};

use super::Polkit;
use super::helper::{self, Event, Session};

/*
 * amane's authentication agent: polkitd asks it to authenticate the user
 * when a program needs a privilege (pkexec, systemctl start, flatpak), and
 * it asks the user for a password through the window that reads Polkit.
 * the password goes to polkit's own helper, which checks it with PAM and
 * tells polkitd itself; amane only carries prompts one way and the answer
 * the other. registered for the user's graphical session, so it answers
 * for every program in it, from whichever process runs amane
 */
pub(super) const PATH: &str = "/org/amane/PolicyKit1/AuthenticationAgent";
const AGENT: &str = "org.freedesktop.PolicyKit1.AuthenticationAgent";
const INTROSPECTABLE: &str = "org.freedesktop.DBus.Introspectable";

const AUTHORITY: &str = "org.freedesktop.PolicyKit1";
const AUTHORITY_PATH: &str = "/org/freedesktop/PolicyKit1/Authority";
const AUTHORITY_INTERFACE: &str = "org.freedesktop.PolicyKit1.Authority";

const LOGIND: &str = "org.freedesktop.login1";
const LOGIND_USER_PATH: &str = "/org/freedesktop/login1/user/self";
const LOGIND_USER: &str = "org.freedesktop.login1.User";

const CANCELLED: &str = "org.freedesktop.PolicyKit1.Error.Cancelled";
const FAILED: &str = "org.freedesktop.PolicyKit1.Error.Failed";
const UNKNOWN_METHOD: &str = "org.freedesktop.DBus.Error.UnknownMethod";

// polkit's own agents give up after this many wrong passwords
const TRIES: u8 = 3;

const INTROSPECTION: &str = r#"<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN"
"http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">
<node>
  <interface name="org.freedesktop.PolicyKit1.AuthenticationAgent">
    <method name="BeginAuthentication">
      <arg name="action_id" type="s" direction="in"/>
      <arg name="message" type="s" direction="in"/>
      <arg name="icon_name" type="s" direction="in"/>
      <arg name="details" type="a{ss}" direction="in"/>
      <arg name="cookie" type="s" direction="in"/>
      <arg name="identities" type="a(sa{sv})" direction="in"/>
    </method>
    <method name="CancelAuthentication">
      <arg name="cookie" type="s" direction="in"/>
    </method>
  </interface>
  <interface name="org.freedesktop.DBus.Introspectable">
    <method name="Introspect">
      <arg name="xml_data" type="s" direction="out"/>
    </method>
  </interface>
</node>
"#;

static NEXT: AtomicU64 = AtomicU64::new(1);

// the request polkitd is waiting on, and the helper run answering it
static WAITING: Mutex<Option<Waiting>> = Mutex::new(None);

struct Waiting {
    id: u64,

    cookie: String,

    user: String,

    // polkitd's BeginAuthentication, answered when the helper is done
    method: Method,

    session: Option<Session>,

    // whether the helper is waiting on a line from the user
    prompting: bool,

    // what the user typed before the helper asked, sent once it does
    pending: Option<String>,
}

// what polkitd is asking the user to prove
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthRequest {
    pub(crate) id: u64,

    // like org.freedesktop.systemd1.manage-units
    pub(crate) action_id: String,

    // the action's own sentence, like "Authentication is required to start 'sshd.service'."
    pub(crate) message: String,

    // the action's icon name, often empty
    pub(crate) icon: String,

    // the user whose password proves it
    pub(crate) user: String,

    // what the helper asks for right now, once it does
    pub(crate) prompt: Option<AuthPrompt>,

    // lines the helper showed, like a fingerprint reader's
    pub(crate) info: Vec<String>,

    // why the last try failed
    pub(crate) error: Option<String>,

    // wrong answers so far
    pub(crate) tries: u8,

    // an answer is with the helper, so the window waits
    pub(crate) busy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthPrompt {
    pub(crate) text: String,

    // whether what's typed may show, which a password may not
    pub(crate) echo: bool,
}

impl AuthRequest {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn action_id(&self) -> &str {
        &self.action_id
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn icon(&self) -> &str {
        &self.icon
    }

    pub fn user(&self) -> &str {
        &self.user
    }

    pub fn prompt(&self) -> Option<&AuthPrompt> {
        self.prompt.as_ref()
    }

    pub fn info(&self) -> &[String] {
        &self.info
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn tries(&self) -> u8 {
        self.tries
    }

    pub fn busy(&self) -> bool {
        self.busy
    }
}

impl AuthPrompt {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn echo(&self) -> bool {
        self.echo
    }
}

impl Polkit {
    /*
     * answers polkitd's authentication requests through auth_request() and
     * answer(); only one program should, so a shell calls this once and
     * other amane programs leave it alone
     */
    pub fn serve_agent() {
        static SERVE: Once = Once::new();

        SERVE.call_once(|| {
            thread::spawn(serve);
        });
    }

    // answers the request with this id with what the user typed; an old id does nothing
    pub fn answer(id: u64, text: String) {
        // off the caller's thread, which may be drawing with the service read
        worker::run(move || answer(id, text));
    }

    // gives up on the request with this id, which the program asking sees as a refusal
    pub fn cancel(id: u64) {
        worker::run(move || {
            let waiting = take_if(|waiting| waiting.id == id);

            let Some(waiting) = waiting else {
                return;
            };

            waiting
                .method
                .reply_error(CANCELLED, "Cancelled by the user");

            clear(id);
        });
    }
}

fn serve() {
    // watched before registering, so polkitd's first call isn't missed
    let methods = Bus::system().methods_at(PATH);

    register();

    thread::spawn(follow_authority);

    for method in methods {
        handle(method);
    }
}

// polkitd forgets its agents when it restarts
fn follow_authority() {
    for change in Bus::system().owner_changes(AUTHORITY) {
        let [_, _, new_owner] = change.arguments() else {
            continue;
        };

        let waiting = take_if(|_| true);

        if let Some(waiting) = waiting {
            waiting
                .method
                .reply_error(FAILED, "The authority went away");

            clear(waiting.id);
        }

        if !new_owner.text().is_empty() {
            register();
        }
    }
}

fn register() {
    let subject = Argument::Struct(vec![
        Argument::from("unix-session"),
        Argument::Map(BTreeMap::from([(
            String::from("session-id"),
            Argument::from(session_id()),
        )])),
    ]);

    let registered = Bus::system().try_call(
        AUTHORITY,
        AUTHORITY_PATH,
        AUTHORITY_INTERFACE,
        "RegisterAuthenticationAgent",
        &[
            subject,
            Argument::from(locale()),
            // polkit takes the path as plain text
            Argument::from(PATH),
        ],
    );

    if let Err(error) = &registered {
        eprintln!(
            "amane: polkit agent not registered: {} ({})",
            error.message(),
            error.name()
        );
    }

    Polkit::write().registered = registered.is_ok();
}

/*
 * the user's graphical session, from logind, since amane may run as a
 * user service outside any session; the environment's id when logind has
 * no display for the user
 */
fn session_id() -> String {
    let display = Bus::system().property(LOGIND, LOGIND_USER_PATH, LOGIND_USER, "Display");

    let id = display.list().first().map(Value::text).unwrap_or_default();

    if !id.is_empty() {
        return id.to_string();
    }

    std::env::var("XDG_SESSION_ID").unwrap_or_default()
}

fn locale() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| String::from("C"))
}

fn handle(method: Method) {
    let interface = method.interface().unwrap_or_else(|| String::from(AGENT));

    match (interface.as_str(), method.name()) {
        (AGENT, "BeginAuthentication") => begin(method),
        (AGENT, "CancelAuthentication") => {
            let cookie = method
                .arguments()
                .first()
                .map(Value::text)
                .unwrap_or_default()
                .to_string();

            let waiting = take_if(|waiting| waiting.cookie == cookie);

            if let Some(waiting) = waiting {
                waiting.method.reply_error(CANCELLED, "Cancelled");

                clear(waiting.id);
            }

            method.reply(&[]);
        }
        (INTROSPECTABLE, "Introspect") => method.reply(&[Argument::from(INTROSPECTION)]),
        _ => method.reply_error(UNKNOWN_METHOD, "Unknown method"),
    }
}

fn begin(method: Method) {
    let [action_id, message, icon, _details, cookie, identities] = method.arguments() else {
        method.reply_error(FAILED, "Bad arguments");

        return;
    };

    let Some(user) = pick_user(identities) else {
        method.reply_error(FAILED, "No user to authenticate as");

        return;
    };

    let id = NEXT.fetch_add(1, Ordering::Relaxed);

    let request = AuthRequest {
        id,
        action_id: action_id.text().to_string(),
        message: message.text().to_string(),
        icon: icon.text().to_string(),
        user: user.clone(),
        prompt: None,
        info: Vec::new(),
        error: None,
        tries: 0,
        busy: false,
    };

    let mut waiting = WAITING.lock().unwrap_or_else(PoisonError::into_inner);

    // polkitd asks one at a time per agent, a second means its first was lost
    if waiting.is_some() {
        method.reply_error(FAILED, "Another request is waiting");

        return;
    }

    *waiting = Some(Waiting {
        id,
        cookie: cookie.text().to_string(),
        user,
        method,
        session: None,
        prompting: false,
        pending: None,
    });

    Polkit::write().request = Some(request);

    // under the lock, so no event from the helper comes before it's kept
    start_session(&mut waiting);
}

// runs the helper for the request kept; a helper that can't start fails the request
fn start_session(waiting: &mut Option<Waiting>) {
    let Some(current) = waiting.as_mut() else {
        return;
    };

    let id = current.id;

    match Session::start(&current.user, &current.cookie, move |event| {
        on_event(id, event)
    }) {
        Ok(session) => {
            current.session = Some(session);
            current.prompting = false;
        }
        Err(error) => {
            let taken = waiting.take();

            if let Some(taken) = taken {
                eprintln!("amane: polkit helper at {}: {error}", helper::SOCKET);

                taken.method.reply_error(
                    FAILED,
                    &format!("The polkit helper could not start: {error}"),
                );

                clear(id);
            }
        }
    }
}

fn on_event(id: u64, event: Event) {
    let mut waiting = WAITING.lock().unwrap_or_else(PoisonError::into_inner);

    if !waiting.as_ref().is_some_and(|waiting| waiting.id == id) {
        return;
    }

    match event {
        Event::Prompt { text, echo } => {
            let current = waiting.as_mut().expect("checked above");

            // what was typed early answers it at once
            if let Some(pending) = current.pending.take() {
                if let Some(session) = &current.session {
                    let _ = session.answer(&pending);
                }

                return;
            }

            current.prompting = true;

            with_request(id, |request| {
                request.prompt = Some(AuthPrompt { text, echo });
                request.busy = false;
            });
        }
        Event::Info(text) => with_request(id, |request| request.info.push(text)),
        Event::Error(text) => with_request(id, |request| request.error = Some(text)),
        Event::Done(true) => {
            if let Some(done) = waiting.take() {
                done.method.reply(&[]);
            }

            clear(id);
        }
        Event::Done(false) => {
            let tries = Polkit::read()
                .request
                .as_ref()
                .filter(|request| request.id == id)
                .map_or(TRIES, |request| request.tries + 1);

            if tries >= TRIES {
                if let Some(done) = waiting.take() {
                    done.method.reply_error(FAILED, "Authentication failed");
                }

                clear(id);

                return;
            }

            with_request(id, |request| {
                request.tries = tries;
                request.prompt = None;
                request.info.clear();
                request.busy = false;
                request.error = Some(String::from("Sorry, that didn't work. Try again."));
            });

            // the same cookie stands for another try
            start_session(&mut waiting);
        }
    }
}

fn answer(id: u64, text: String) {
    let mut waiting = WAITING.lock().unwrap_or_else(PoisonError::into_inner);

    let Some(current) = waiting.as_mut().filter(|waiting| waiting.id == id) else {
        return;
    };

    if current.prompting {
        current.prompting = false;

        if let Some(session) = &current.session {
            let _ = session.answer(&text);
        }
    } else {
        current.pending = Some(text);
    }

    with_request(id, |request| {
        request.prompt = None;
        request.error = None;
        request.busy = true;
    });
}

// the user to authenticate as: the one running amane when polkitd allows them, else the first user
fn pick_user(identities: &Value) -> Option<String> {
    let own = own_uid();

    let users: Vec<u32> = identities
        .list()
        .iter()
        .filter(|identity| identity.list().first().map(Value::text) == Some("unix-user"))
        .filter_map(|identity| {
            identity
                .list()
                .get(1)
                .map(|details| details.get("uid").number())
        })
        .map(|uid| uid as u32)
        .collect();

    let uid = users
        .iter()
        .copied()
        .find(|uid| Some(*uid) == own)
        .or_else(|| users.first().copied())?;

    user_name(uid)
}

fn own_uid() -> Option<u32> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;

    let line = status.lines().find(|line| line.starts_with("Uid:"))?;

    line.split_whitespace().nth(1)?.parse().ok()
}

fn user_name(uid: u32) -> Option<String> {
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;

    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');

        let name = fields.next()?;

        let id: u32 = fields.nth(1)?.parse().ok()?;

        (id == uid).then(|| name.to_string())
    })
}

fn take_if(wanted: impl Fn(&Waiting) -> bool) -> Option<Waiting> {
    let mut waiting = WAITING.lock().unwrap_or_else(PoisonError::into_inner);

    match waiting.as_ref() {
        Some(current) if wanted(current) => waiting.take(),
        _ => None,
    }
}

fn with_request(id: u64, change: impl FnOnce(&mut AuthRequest)) {
    let mut polkit = Polkit::write();

    match polkit.request.as_mut() {
        Some(request) if request.id == id => change(request),
        _ => polkit.quiet(),
    }
}

fn clear(id: u64) {
    let mut polkit = Polkit::write();

    if polkit
        .request
        .as_ref()
        .is_some_and(|request| request.id == id)
    {
        polkit.request = None;
    } else {
        polkit.quiet();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn identity(kind: &str, key: &str, id: f64) -> Value {
        Value::List(vec![
            Value::Text(String::from(kind)),
            Value::Map(BTreeMap::from([(String::from(key), Value::Number(id))])),
        ])
    }

    #[test]
    fn the_running_user_is_picked_when_allowed() {
        let own = own_uid().expect("a uid");
        let name = user_name(own).expect("a name");

        let identities = Value::List(vec![
            identity("unix-group", "gid", 1.0),
            identity("unix-user", "uid", 0.0),
            identity("unix-user", "uid", f64::from(own)),
        ]);

        assert_eq!(pick_user(&identities), Some(name));

        // root alone, as pkexec asks for a wheel-less user
        let root_only = Value::List(vec![identity("unix-user", "uid", 0.0)]);

        assert_eq!(pick_user(&root_only), Some(String::from("root")));
        assert_eq!(pick_user(&Value::List(Vec::new())), None);
    }
}
