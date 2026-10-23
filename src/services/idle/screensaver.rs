use std::thread;

use crate::{Argument, Bus, Idle, Method, Service};

const NAME: &str = "org.freedesktop.ScreenSaver";

// some kde-minded programs ask for this name instead
const KDE_NAME: &str = "org.kde.screensaver";

// chromium calls the first path, firefox and kde programs the second
const PATHS: [&str; 2] = ["/org/freedesktop/ScreenSaver", "/ScreenSaver"];

const INTERFACE: &str = "org.freedesktop.ScreenSaver";

const INTROSPECTABLE: &str = "org.freedesktop.DBus.Introspectable";

// qt programs read this before calling, and wait for a timeout without it
const INTROSPECTION: &str = r#"<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">
<node>
  <interface name="org.freedesktop.ScreenSaver">
    <method name="Inhibit">
      <arg name="application_name" type="s" direction="in"/>
      <arg name="reason_for_inhibit" type="s" direction="in"/>
      <arg name="cookie" type="u" direction="out"/>
    </method>
    <method name="UnInhibit">
      <arg name="cookie" type="u" direction="in"/>
    </method>
    <method name="GetActive">
      <arg type="b" direction="out"/>
    </method>
  </interface>
  <interface name="org.freedesktop.DBus.Introspectable">
    <method name="Introspect">
      <arg name="xml_data" type="s" direction="out"/>
    </method>
  </interface>
</node>
"#;

pub fn run() {
    let bus = Bus::session();

    // watched before owning the name, so no early call is missed
    let [main, other] = PATHS.map(|path| bus.methods_at(path));

    // a desktop's own screensaver service keeps the name, amane only follows the compositor then
    if !bus.own(NAME) {
        return;
    }

    bus.own(KDE_NAME);

    Idle::write().running = true;

    thread::spawn(release_departed);

    thread::spawn(move || {
        for method in other {
            answer(&method);
        }
    });

    for method in main {
        answer(&method);
    }
}

fn answer(method: &Method) {
    // the interface may be left out of a call, which then means the only one with that method
    let interface = method
        .interface()
        .unwrap_or_else(|| String::from(INTERFACE));

    match (interface.as_str(), method.name()) {
        (INTERFACE, "Inhibit") => inhibit(method),
        (INTERFACE, "UnInhibit") => uninhibit(method),
        (INTERFACE, "GetActive") => method.reply(&[Argument::Bool(Idle::read().is_idle())]),

        (INTROSPECTABLE, "Introspect") => method.reply(&[Argument::from(INTROSPECTION)]),

        // every call waits for an answer, even one amane does not know
        _ => method.reply(&[]),
    }
}

fn inhibit(method: &Method) {
    let [application, reason] = method.arguments() else {
        method.reply(&[]);

        return;
    };

    let sender = method.sender().unwrap_or_default();

    let cookie = Idle::write().inhibit(&sender, application.text(), reason.text());

    method.reply(&[Argument::Unsigned(cookie)]);
}

fn uninhibit(method: &Method) {
    if let [cookie] = method.arguments() {
        let mut idle = Idle::write();

        if !idle.uninhibit(cookie.number() as u32) {
            idle.quiet();
        }
    }

    method.reply(&[]);
}

// a program that quits or crashes without UnInhibit would otherwise hold off idle forever
fn release_departed() {
    let changes = Bus::session().signals("org.freedesktop.DBus", "NameOwnerChanged");

    for signal in changes {
        let [name, _old_owner, new_owner] = signal.arguments() else {
            continue;
        };

        // only unique names like :1.42 are what inhibits are kept under
        if !new_owner.text().is_empty() || !name.text().starts_with(':') {
            continue;
        }

        let mut idle = Idle::write();

        if !idle.release_sender(name.text()) {
            idle.quiet();
        }
    }
}
