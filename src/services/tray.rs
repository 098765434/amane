#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo, missing_docs)]

/*
 * the system tray: the icons programs like Steam, Discord, Nextcloud or
 * nm-applet put in a status bar, by the StatusNotifierItem protocol, with
 * the menus they send over dbusmenu
 *
 * amane keeps the watcher (org.kde.StatusNotifierWatcher) programs register
 * with, and is the host that shows them; when another program already
 * keeps the watcher, amane only registers as a host and reads its list
 *
 * each item's properties are read once as it registers and again on each
 * of its New* signals; a menu is read when it's asked for, since apps fill
 * theirs in as they're about to show
 */

mod icon;
mod menu;
mod watcher;

use std::path::{Path, PathBuf};
use std::thread;

use crate::services::worker;
use crate::{Argument, Bus, Service, Value};

pub use menu::{TrayMenuItem, TrayToggle};

pub(crate) const ITEM: &str = "org.kde.StatusNotifierItem";

// the path an item that registers by its bus name alone is at
pub(crate) const DEFAULT_PATH: &str = "/StatusNotifierItem";

const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

// the size icons are asked for at, in pixels: a bar's icon on a scaled screen
const ICON_SIZE: u32 = 48;

/// What an item says it's doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrayStatus {
    /// Nothing worth showing; a bar may leave it out.
    Passive,

    /// Running as usual.
    #[default]
    Active,

    /// Wants attention, like a new message.
    NeedsAttention,
}

/// One program's icon in the tray.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrayItem {
    // its unique bus name, like :1.42, which its signals come from
    pub(crate) sender: String,
    pub(crate) path: String,

    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) status: TrayStatus,
    pub(crate) tooltip: String,

    pub(crate) icon: Option<PathBuf>,
    pub(crate) attention_icon: Option<PathBuf>,

    // its dbusmenu's object path, when it has one
    pub(crate) menu: Option<String>,

    // it has only a menu: a click shows it rather than activating
    pub(crate) item_is_menu: bool,
}

impl TrayItem {
    /// What tells it apart from every other item now, like ":1.42/StatusNotifierItem".
    pub fn key(&self) -> String {
        format!("{}{}", self.sender, self.path)
    }

    /// The name the program gives it, like "steam" or "nm-applet", the same each run.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The program's title for the item.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Whether it's quiet, active or wants attention.
    pub fn status(&self) -> TrayStatus {
        self.status
    }

    /// The tooltip's text, or the title when it has none.
    pub fn tooltip(&self) -> &str {
        if self.tooltip.is_empty() {
            &self.title
        } else {
            &self.tooltip
        }
    }

    /// The icon to show now: the attention one while it wants attention.
    pub fn icon(&self) -> Option<&Path> {
        let attention = (self.status == TrayStatus::NeedsAttention)
            .then_some(self.attention_icon.as_deref())
            .flatten();

        attention.or(self.icon.as_deref())
    }

    /// Whether it has a dbusmenu, which Tray::open_menu reads.
    pub fn has_menu(&self) -> bool {
        self.menu.is_some()
    }

    /// It has only a menu: a click shows the menu rather than activating it.
    pub fn item_is_menu(&self) -> bool {
        self.item_is_menu
    }
}

/// The tray's items (status notifier items), with amane as the watcher programs register
/// with, and the menu read last for one of them.
#[derive(Default)]
pub struct Tray {
    // in the order they registered
    items: Vec<TrayItem>,

    // the menu last read, by the item's key, until it's taken
    menu: Option<(String, Vec<TrayMenuItem>)>,
}

impl Service for Tray {
    fn new() -> Self {
        Self::default()
    }

    fn listen() {
        thread::spawn(watch_items);
        thread::spawn(watch_owners);

        watcher::run();
    }
}

impl Tray {
    /// The items, in the order they registered.
    pub fn items(&self) -> &[TrayItem] {
        &self.items
    }

    /// A menu read for the item `key` since the last take, to show.
    pub fn take_menu(key: &str) -> Option<Vec<TrayMenuItem>> {
        let mut tray = Self::write();

        if tray
            .menu
            .as_ref()
            .is_none_or(|(menu_key, _)| menu_key != key)
        {
            tray.quiet();

            return None;
        }

        tray.menu.take().map(|(_, items)| items)
    }

    /// The item's main action, like showing its window; x and y are where on the screen.
    pub fn activate(key: &str, x: i32, y: i32) {
        call_item(key, "Activate", vec![Argument::Int(x), Argument::Int(y)]);
    }

    /// The middle click's action.
    pub fn secondary_activate(key: &str, x: i32, y: i32) {
        call_item(
            key,
            "SecondaryActivate",
            vec![Argument::Int(x), Argument::Int(y)],
        );
    }

    /// Asks the program to show its own menu, for an item without a dbusmenu.
    pub fn context_menu(key: &str, x: i32, y: i32) {
        call_item(key, "ContextMenu", vec![Argument::Int(x), Argument::Int(y)]);
    }

    /// Scrolls over the item by `delta` steps, along the vertical or the horizontal.
    pub fn scroll(key: &str, delta: i32, vertical: bool) {
        let orientation = if vertical { "vertical" } else { "horizontal" };

        call_item(
            key,
            "Scroll",
            vec![Argument::Int(delta), Argument::from(orientation)],
        );
    }

    /// Reads the item's menu on a worker; it shows up for take_menu, and
    /// the windows that read the tray draw again.
    pub fn open_menu(key: &str) {
        let Some((sender, path)) = Self::read().menu_of(key) else {
            return;
        };

        let key = String::from(key);

        worker::run(move || {
            let items = menu::read(&sender, &path);

            Self::write().menu = Some((key, items));
        });
    }

    /// A menu entry was picked.
    pub fn select(key: &str, id: i32) {
        let Some((sender, path)) = Self::read().menu_of(key) else {
            return;
        };

        worker::run(move || menu::clicked(&sender, &path, id));
    }

    fn menu_of(&self, key: &str) -> Option<(String, String)> {
        let item = self.items.iter().find(|item| item.key() == key)?;

        Some((item.sender.clone(), item.menu.clone()?))
    }

    // whether it changed what shows
    fn put(&mut self, item: TrayItem) -> bool {
        match self.items.iter_mut().find(|old| old.key() == item.key()) {
            Some(old) if *old == item => return false,
            Some(old) => *old = item,
            None => self.items.push(item),
        }

        true
    }

    pub(crate) fn remove(&mut self, sender: &str, path: Option<&str>) -> Vec<String> {
        let mut gone = Vec::new();

        self.items.retain(|item| {
            let keep = item.sender != sender || path.is_some_and(|path| path != item.path);

            if !keep {
                gone.push(item.key());
            }

            keep
        });

        gone
    }

    pub(crate) fn keys(&self) -> Vec<String> {
        self.items.iter().map(TrayItem::key).collect()
    }
}

fn call_item(key: &str, method: &'static str, arguments: Vec<Argument>) {
    let Some((sender, path)) = Tray::read()
        .items
        .iter()
        .find(|item| item.key() == key)
        .map(|item| (item.sender.clone(), item.path.clone()))
    else {
        return;
    };

    worker::run(move || {
        Bus::session().call(&sender, &path, ITEM, method, &arguments);
    });
}

/*
 * reads the item at `sender` and `path` and keeps it, or leaves it out
 * when it doesn't answer; `sender` is its unique name
 */
pub(crate) fn add(sender: &str, path: &str) {
    let properties =
        Bus::session().call(sender, path, PROPERTIES, "GetAll", &[Argument::from(ITEM)]);

    if properties == Value::Nothing {
        return;
    }

    let item = read_item(sender, path, &properties);

    let mut tray = Tray::write();

    if !tray.put(item) {
        tray.quiet();
    }
}

fn read_item(sender: &str, path: &str, properties: &Value) -> TrayItem {
    let theme_path = properties.get("IconThemePath").text();

    let icon = icon::find(
        properties.get("IconName").text(),
        properties.get("IconPixmap"),
        theme_path,
        ICON_SIZE,
    );

    let attention_icon = icon::find(
        properties.get("AttentionIconName").text(),
        properties.get("AttentionIconPixmap"),
        theme_path,
        ICON_SIZE,
    );

    // the tooltip is (icon name, icon pixmap, title, description)
    let tooltip = properties.get("ToolTip").list();
    let tooltip = tooltip
        .get(2)
        .map(Value::text)
        .filter(|title| !title.is_empty())
        .or_else(|| tooltip.get(3).map(Value::text))
        .unwrap_or("");

    let menu = properties.get("Menu").text();

    TrayItem {
        sender: String::from(sender),
        path: String::from(path),

        id: String::from(properties.get("Id").text()),
        title: String::from(properties.get("Title").text()),
        status: status(properties.get("Status").text()),
        tooltip: plain(tooltip),

        icon,
        attention_icon,

        // "/" or "/NO_DBUSMENU" mean none
        menu: (!menu.is_empty() && menu != "/" && menu != "/NO_DBUSMENU")
            .then(|| String::from(menu)),

        item_is_menu: properties.get("ItemIsMenu").bool(),
    }
}

fn status(text: &str) -> TrayStatus {
    match text {
        "Passive" => TrayStatus::Passive,
        "NeedsAttention" => TrayStatus::NeedsAttention,
        _ => TrayStatus::Active,
    }
}

// tooltips may carry a little html, which a bar shows as plain text
fn plain(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut in_tag = false;

    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => plain.push(c),
            _ => {}
        }
    }

    plain
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .trim()
        .to_string()
}

// an item's New* signals: it changed its icon, title, tooltip or status
fn watch_items() {
    for signal in Bus::session().interface_signals(ITEM) {
        let (sender, path) = (signal.sender().to_string(), signal.path().to_string());

        let known = Tray::read()
            .items
            .iter()
            .any(|item| item.sender == sender && item.path == path);

        if known {
            add(&sender, &path);
        }
    }
}

// a program that quits takes its items with it
fn watch_owners() {
    for signal in Bus::session().signals("org.freedesktop.DBus", "NameOwnerChanged") {
        let [name, _, new_owner] = signal.arguments() else {
            continue;
        };

        if !new_owner.text().is_empty() || !name.text().starts_with(':') {
            continue;
        }

        let mut tray = Tray::write();
        let gone = tray.remove(name.text(), None);

        if gone.is_empty() {
            tray.quiet();
        }

        drop(tray);

        for key in gone {
            watcher::announce_unregistered(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tooltip_loses_its_markup() {
        assert_eq!(plain("<b>Steam</b> &amp; friends"), "Steam & friends");
        assert_eq!(plain("  plain "), "plain");
    }

    #[test]
    fn the_attention_icon_shows_while_it_wants_attention() {
        let mut item = TrayItem {
            icon: Some(PathBuf::from("/a.png")),
            attention_icon: Some(PathBuf::from("/b.png")),
            ..TrayItem::default()
        };

        assert_eq!(item.icon(), Some(Path::new("/a.png")));

        item.status = TrayStatus::NeedsAttention;
        assert_eq!(item.icon(), Some(Path::new("/b.png")));

        item.attention_icon = None;
        assert_eq!(item.icon(), Some(Path::new("/a.png")));
    }
}
