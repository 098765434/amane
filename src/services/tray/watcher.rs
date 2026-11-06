/*
 * org.kde.StatusNotifierWatcher, where programs register their tray items:
 * amane keeps it, and is its host; with another program keeping it, amane
 * registers as a host there and follows its list instead
 */

use crate::services::worker;
use crate::{Argument, Bus, Method, Service};

use super::{DEFAULT_PATH, Tray, add};

const NAME: &str = "org.kde.StatusNotifierWatcher";
const PATH: &str = "/StatusNotifierWatcher";
const INTERFACE: &str = "org.kde.StatusNotifierWatcher";

const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const INTROSPECTABLE: &str = "org.freedesktop.DBus.Introspectable";

const INTROSPECTION: &str = r#"<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">
<node>
  <interface name="org.kde.StatusNotifierWatcher">
    <method name="RegisterStatusNotifierItem">
      <arg name="service" type="s" direction="in"/>
    </method>
    <method name="RegisterStatusNotifierHost">
      <arg name="service" type="s" direction="in"/>
    </method>
    <property name="RegisteredStatusNotifierItems" type="as" access="read"/>
    <property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
    <property name="ProtocolVersion" type="i" access="read"/>
    <signal name="StatusNotifierItemRegistered">
      <arg type="s"/>
    </signal>
    <signal name="StatusNotifierItemUnregistered">
      <arg type="s"/>
    </signal>
    <signal name="StatusNotifierHostRegistered"/>
  </interface>
  <interface name="org.freedesktop.DBus.Introspectable">
    <method name="Introspect">
      <arg name="data" type="s" direction="out"/>
    </method>
  </interface>
</node>
"#;

pub fn run() {
    let bus = Bus::session();

    // watched before owning the name, so no early call is missed
    let methods = bus.methods_at(PATH);

    // the host's own name, which programs look for before they register
    let host = format!("org.kde.StatusNotifierHost-{}", std::process::id());
    bus.own(&host);

    if !bus.own(NAME) {
        follow(&host);

        return;
    }

    // programs that started before the watcher register again once it's here
    bus.emit(PATH, INTERFACE, "StatusNotifierHostRegistered", &[]);

    for method in methods {
        answer(&method);
    }
}

fn answer(method: &Method) {
    let interface = method
        .interface()
        .unwrap_or_else(|| String::from(INTERFACE));

    match (interface.as_str(), method.name()) {
        (INTERFACE, "RegisterStatusNotifierItem") => register(method),
        (INTERFACE, "RegisterStatusNotifierHost") => method.reply(&[]),

        (PROPERTIES, "Get") => {
            let name = method
                .arguments()
                .get(1)
                .map(|name| name.text().to_string());
            let value = name.and_then(|name| property(&name));

            match value {
                Some(value) => method.reply(&[Argument::Variant(Box::new(value))]),
                None => method.reply_error(
                    "org.freedesktop.DBus.Error.UnknownProperty",
                    "no such property",
                ),
            }
        }
        (PROPERTIES, "GetAll") => {
            let all = [
                "RegisteredStatusNotifierItems",
                "IsStatusNotifierHostRegistered",
                "ProtocolVersion",
            ]
            .into_iter()
            .filter_map(|name| Some((String::from(name), property(name)?)))
            .collect();

            method.reply(&[Argument::Map(all)]);
        }

        (INTROSPECTABLE, "Introspect") => method.reply(&[Argument::from(INTROSPECTION)]),

        // every call waits for an answer, even one amane does not know
        _ => method.reply(&[]),
    }
}

fn property(name: &str) -> Option<Argument> {
    Some(match name {
        "RegisteredStatusNotifierItems" => Argument::TextList(Tray::read().keys()),
        "IsStatusNotifierHostRegistered" => Argument::Bool(true),
        "ProtocolVersion" => Argument::Int(0),
        _ => return None,
    })
}

/*
 * an item registers by its object path (its bus name is the caller's), by
 * a bus name (at the usual path), or by both run together
 */
fn register(method: &Method) {
    let service = method
        .arguments()
        .first()
        .map(|service| service.text().to_string())
        .unwrap_or_default();

    let caller = method.sender().unwrap_or_default();

    // the program waits for the answer, so it comes before reading the item
    method.reply(&[]);

    let (name, path) = split(&service, &caller);

    worker::run(move || {
        let Some(sender) = unique(&name) else {
            return;
        };

        add(&sender, &path);

        if Tray::read().keys().contains(&format!("{sender}{path}")) {
            let key = format!("{sender}{path}");

            Bus::session().emit(
                PATH,
                INTERFACE,
                "StatusNotifierItemRegistered",
                &[Argument::from(key)],
            );
        }
    });
}

// what an item registered with, as (bus name, object path)
fn split(service: &str, caller: &str) -> (String, String) {
    if service.starts_with('/') {
        return (String::from(caller), String::from(service));
    }

    match service.find('/') {
        Some(at) => (String::from(&service[..at]), String::from(&service[at..])),
        None if service.is_empty() => (String::from(caller), String::from(DEFAULT_PATH)),
        None => (String::from(service), String::from(DEFAULT_PATH)),
    }
}

// the unique name like :1.42 that owns `name`, which its signals come from
fn unique(name: &str) -> Option<String> {
    if name.starts_with(':') {
        return Some(String::from(name));
    }

    let owner = Bus::session().call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetNameOwner",
        &[Argument::from(name)],
    );

    let owner = owner.text();

    (!owner.is_empty()).then(|| String::from(owner))
}

pub fn announce_unregistered(key: &str) {
    Bus::session().emit(
        PATH,
        INTERFACE,
        "StatusNotifierItemUnregistered",
        &[Argument::from(key)],
    );
}

// another program keeps the watcher: amane is a host there, following its list
fn follow(host: &str) {
    let bus = Bus::session();

    let signals = bus.signals_from(NAME);

    bus.call(
        NAME,
        PATH,
        INTERFACE,
        "RegisterStatusNotifierHost",
        &[Argument::from(host)],
    );

    let registered = bus.property(NAME, PATH, INTERFACE, "RegisteredStatusNotifierItems");

    for service in registered.list() {
        follow_item(service.text());
    }

    for signal in signals {
        let Some(service) = signal.arguments().first() else {
            continue;
        };

        match signal.name() {
            "StatusNotifierItemRegistered" => follow_item(service.text()),
            "StatusNotifierItemUnregistered" => {
                let (name, path) = split(service.text(), "");

                if let Some(sender) = unique(&name).or(Some(name)) {
                    Tray::write().remove(&sender, Some(&path));
                }
            }
            _ => {}
        }
    }
}

fn follow_item(service: &str) {
    let (name, path) = split(service, "");

    if let Some(sender) = unique(&name) {
        add(&sender, &path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_registers_by_path_name_or_both() {
        assert_eq!(
            split("/org/ayatana/NotificationItem/nm", ":1.9"),
            (
                String::from(":1.9"),
                String::from("/org/ayatana/NotificationItem/nm")
            )
        );
        assert_eq!(
            split("org.kde.StatusNotifierItem-12-1", ":1.9"),
            (
                String::from("org.kde.StatusNotifierItem-12-1"),
                String::from("/StatusNotifierItem")
            )
        );
        assert_eq!(
            split(":1.7/StatusNotifierItem", ":1.9"),
            (String::from(":1.7"), String::from("/StatusNotifierItem"))
        );
    }
}
