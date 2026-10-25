use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::{DesktopAction, DesktopApp, exec};

// the desktop file id: its path inside applications/, with / turned into -
pub fn id(applications: &Path, path: &Path) -> Option<String> {
    if path.extension()? != "desktop" {
        return None;
    }

    let relative = path.strip_prefix(applications).ok()?;

    let id = relative.to_str()?.replace('/', "-");

    Some(id)
}

// none for files that are not programs, or that ask not to be shown in menus
pub fn read(path: &Path, icons: &HashMap<String, PathBuf>) -> Option<DesktopApp> {
    let text = fs::read_to_string(path).ok()?;

    parse(&text, icons)
}

fn parse(text: &str, icons: &HashMap<String, PathBuf>) -> Option<DesktopApp> {
    let sections = sections(text);

    let fields = sections.get("Desktop Entry")?;

    if fields.get("Type").copied() != Some("Application") {
        return None;
    }

    if is_true(fields.get("NoDisplay")) || is_true(fields.get("Hidden")) {
        return None;
    }

    let name = fields.get("Name")?.to_string();
    let exec_codes = fields.get("Exec")?.to_string();
    let exec = exec::strip_field_codes(&exec_codes);

    let icon = fields.get("Icon").map(|icon| icon.to_string());

    let icon_path = icon.as_deref().and_then(|icon| find_icon(icon, icons));

    let description = fields
        .get("Comment")
        .or(fields.get("GenericName"))
        .map(|description| description.to_string());

    let categories = list_field(fields.get("Categories"));
    let keywords = list_field(fields.get("Keywords"));
    let mime_types = list_field(fields.get("MimeType"));

    let actions = list_field(fields.get("Actions"))
        .iter()
        .filter_map(|id| action(sections.get(format!("Desktop Action {id}").as_str())?))
        .collect();

    Some(DesktopApp {
        name,
        exec,
        exec_codes,
        mime_types,
        icon,
        icon_path,
        description,
        categories,
        keywords,
        actions,
    })
}

// an action needs a name to show and a command to run
fn action(fields: &HashMap<&str, &str>) -> Option<DesktopAction> {
    Some(DesktopAction {
        name: fields.get("Name")?.to_string(),
        exec: exec::strip_field_codes(fields.get("Exec")?),
        icon: fields.get("Icon").map(|icon| icon.to_string()),
    })
}

// a desktop-entry list value is semicolon-separated, with a trailing separator
fn list_field(value: Option<&&str>) -> Vec<String> {
    match value {
        Some(value) => value
            .split(';')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect(),
        None => Vec::new(),
    }
}

// the keys of each [section] by its name, like "Desktop Entry" or "Desktop Action new-window",
// leaving out translations like Name[de]
fn sections(text: &str) -> HashMap<&str, HashMap<&str, &str>> {
    let mut sections: HashMap<&str, HashMap<&str, &str>> = HashMap::new();

    let mut current = None;

    for line in text.lines() {
        let line = line.trim();

        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            current = Some(name);

            continue;
        }

        let Some(section) = current else {
            continue;
        };

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        sections
            .entry(section)
            .or_default()
            .insert(key.trim(), value.trim());
    }

    sections
}

fn is_true(value: Option<&&str>) -> bool {
    value.copied() == Some("true")
}

// an icon can be a full path instead of a name from the theme
pub fn find_icon(icon: &str, icons: &HashMap<String, PathBuf>) -> Option<PathBuf> {
    let path = Path::new(icon);

    if path.is_absolute() {
        return Some(path.to_path_buf());
    }

    icons.get(icon).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX: &str = "\
[Desktop Entry]
Type=Application
Name=Firefox
Name[de]=Feuerfuchs
Exec=firefox %u
Icon=firefox
Actions=new-window;new-private-window;broken;

[Desktop Action new-window]
Name=New Window
Exec=firefox --new-window %u

[Desktop Action new-private-window]
Name=New Private Window
Exec=firefox --private-window %u
Icon=firefox-private

[Desktop Action broken]
Name=Has No Command
";

    #[test]
    fn reads_the_main_section() {
        let app = parse(FIREFOX, &HashMap::new()).expect("an application");

        assert_eq!(app.name(), "Firefox");
        assert_eq!(app.exec(), "firefox");
        assert_eq!(app.icon(), Some("firefox"));
    }

    #[test]
    fn reads_the_actions_it_lists_in_order() {
        let app = parse(FIREFOX, &HashMap::new()).expect("an application");

        let names: Vec<&str> = app.actions().iter().map(DesktopAction::name).collect();

        assert_eq!(names, ["New Window", "New Private Window"]);
        assert_eq!(app.actions()[0].exec(), "firefox --new-window");
        assert_eq!(app.actions()[0].icon(), None);
        assert_eq!(app.actions()[1].icon(), Some("firefox-private"));
    }

    #[test]
    fn an_action_section_is_not_the_app() {
        let text = "[Desktop Action new-window]\nName=New Window\nExec=x\n";

        assert_eq!(parse(text, &HashMap::new()), None);
    }
}
