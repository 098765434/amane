/*
 * an item's menu over dbusmenu (com.canonical.dbusmenu): read whole as it's
 * about to show, and told which entry was picked
 */

use std::fs;
use std::path::PathBuf;

use crate::{Argument, Bus, Value};

use super::icon::fnv;

const DBUSMENU: &str = "com.canonical.dbusmenu";

/// How a menu entry can be ticked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrayToggle {
    /// A plain entry, with no tick.
    #[default]
    None,

    /// On or off, on its own.
    Check(bool),

    /// One of a group.
    Radio(bool),
}

/// One entry of an item's menu.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrayMenuItem {
    /// What picking it tells the program.
    pub id: i32,

    /// With the underscores that mark access keys taken out.
    pub label: String,

    /// Whether it can be picked; a disabled one shows grayed out.
    pub enabled: bool,
    /// A line between groups of entries, with nothing to pick.
    pub separator: bool,
    /// Whether it has a check mark or a radio dot, and if it's on.
    pub toggle: TrayToggle,

    /// Its icon as a file to load, when it has one.
    pub icon: Option<PathBuf>,

    /// A submenu's entries.
    pub children: Vec<TrayMenuItem>,
}

// the menu's entries; none when it doesn't answer
pub fn read(sender: &str, path: &str) -> Vec<TrayMenuItem> {
    let bus = Bus::session();

    // some programs fill their menu in only now
    bus.call(sender, path, DBUSMENU, "AboutToShow", &[Argument::Int(0)]);

    let reply = bus.call(
        sender,
        path,
        DBUSMENU,
        "GetLayout",
        &[
            Argument::Int(0),
            Argument::Int(-1),
            Argument::TextList(Vec::new()),
        ],
    );

    // (revision, layout); the root's children are the menu
    let [_, root] = reply.list() else {
        return Vec::new();
    };

    children(root)
}

pub fn clicked(sender: &str, path: &str, id: i32) {
    Bus::session().call(
        sender,
        path,
        DBUSMENU,
        "Event",
        &[
            Argument::Int(id),
            Argument::from("clicked"),
            Argument::Variant(Box::new(Argument::Int(0))),
            Argument::Unsigned(0),
        ],
    );
}

// a layout node is (id, properties, children), each child a node of its own
fn children(node: &Value) -> Vec<TrayMenuItem> {
    let [_, _, children] = node.list() else {
        return Vec::new();
    };

    children.list().iter().filter_map(entry).collect()
}

fn entry(node: &Value) -> Option<TrayMenuItem> {
    let [id, properties, _] = node.list() else {
        return None;
    };

    // a missing property means its default
    let flag = |name: &str| match properties.get(name) {
        Value::Bool(on) => *on,
        _ => true,
    };

    if !flag("visible") {
        return None;
    }

    let on = properties.get("toggle-state").number() == 1.0;

    let toggle = match properties.get("toggle-type").text() {
        "checkmark" => TrayToggle::Check(on),
        "radio" => TrayToggle::Radio(on),
        _ => TrayToggle::None,
    };

    let icon_data: Vec<u8> = properties
        .get("icon-data")
        .list()
        .iter()
        .map(|byte| byte.number() as u8)
        .collect();

    let icon_name = properties.get("icon-name").text();

    let icon = if icon_data.is_empty() {
        (!icon_name.is_empty())
            .then(|| crate::services::apps::find_icon(icon_name))
            .flatten()
    } else {
        write_png(&icon_data)
    };

    Some(TrayMenuItem {
        id: id.number() as i32,
        label: mnemonic_free(properties.get("label").text()),
        enabled: flag("enabled"),
        separator: properties.get("type").text() == "separator",
        toggle,
        icon,
        children: children(node),
    })
}

// "_File" is "File", and "__" a plain underscore
fn mnemonic_free(label: &str) -> String {
    let mut plain = String::with_capacity(label.len());
    let mut chars = label.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '_' {
            plain.push(c);

            continue;
        }

        if chars.peek() == Some(&'_') {
            chars.next();
            plain.push('_');
        }
    }

    plain
}

// where tray icons are written, under the cache
pub(crate) fn cache() -> Option<PathBuf> {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?;

    Some(cache.join("amane").join("tray"))
}

// a menu entry's icon comes as a png already
fn write_png(bytes: &[u8]) -> Option<PathBuf> {
    let folder = cache()?;
    let file = folder.join(format!("{:016x}.png", fnv(bytes)));

    if file.is_file() {
        return Some(file);
    }

    fs::create_dir_all(&folder).ok()?;

    let partial = file.with_extension("part");
    fs::write(&partial, bytes).ok()?;
    fs::rename(&partial, &file).ok()?;

    Some(file)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn node(id: i32, properties: &[(&str, Value)], children: Vec<Value>) -> Value {
        let map: BTreeMap<String, Value> = properties
            .iter()
            .map(|(key, value)| (String::from(*key), value.clone()))
            .collect();

        Value::List(vec![
            Value::Number(f64::from(id)),
            Value::Map(map),
            Value::List(children),
        ])
    }

    fn text(text: &str) -> Value {
        Value::Text(String::from(text))
    }

    #[test]
    fn reads_a_layout() {
        let root = node(
            0,
            &[],
            vec![
                node(1, &[("label", text("_Open"))], Vec::new()),
                node(2, &[("type", text("separator"))], Vec::new()),
                node(
                    3,
                    &[
                        ("label", text("Wi-Fi")),
                        ("toggle-type", text("checkmark")),
                        ("toggle-state", Value::Number(1.0)),
                        ("enabled", Value::Bool(false)),
                    ],
                    Vec::new(),
                ),
                node(4, &[("visible", Value::Bool(false))], Vec::new()),
                node(
                    5,
                    &[
                        ("label", text("More")),
                        ("children-display", text("submenu")),
                    ],
                    vec![node(6, &[("label", text("Inner"))], Vec::new())],
                ),
            ],
        );

        let items = children(&root);

        assert_eq!(items.len(), 4);
        assert_eq!(items[0].label, "Open");
        assert!(items[0].enabled);
        assert!(items[1].separator);
        assert_eq!(items[2].toggle, TrayToggle::Check(true));
        assert!(!items[2].enabled);
        assert_eq!(items[3].id, 5);
        assert_eq!(items[3].children[0].label, "Inner");
    }

    #[test]
    fn access_key_marks_come_out() {
        assert_eq!(mnemonic_free("_Quit"), "Quit");
        assert_eq!(mnemonic_free("snake__case"), "snake_case");
        assert_eq!(mnemonic_free("plain"), "plain");
    }
}
