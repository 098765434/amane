/*
 * the two settings kdeconnect has no d-bus setter for, written where its own
 * settings pages write them: each plugin's QSettings ini file under
 * ~/.config/kdeconnect/<device>/<plugin>/config
 */

use std::fs;
use std::path::PathBuf;

use serde_json::{Map, Value as Json, json};

use super::device::{CLIPBOARD_PLUGIN, RUN_COMMAND_PLUGIN};

// QSettings puts top-level keys in this section
const GENERAL: &str = "General";

/*
 * the commands the phone gets while its remote is on, under fixed ids so
 * turning it on twice adds nothing and turning it off leaves commands the
 * user added by hand; the ids are the ones the quickshell version wrote
 */
pub const RUN_COMMANDS: [(&str, &str, &str); 4] = [
    (
        "{c1a0b5e2-0000-4a00-8000-caelestia0001}",
        "Lock Screen",
        "loginctl lock-session",
    ),
    (
        "{c1a0b5e2-0000-4a00-8000-caelestia0002}",
        "Suspend",
        "systemctl suspend",
    ),
    (
        "{c1a0b5e2-0000-4a00-8000-caelestia0003}",
        "Lock & Suspend",
        "loginctl lock-session && systemctl suspend",
    ),
    (
        "{c1a0b5e2-0000-4a00-8000-caelestia0004}",
        "Screenshot",
        "niri msg action screenshot-screen",
    ),
];

fn plugin_config(device: &str, plugin: &str) -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;

    Some(
        base.join("kdeconnect")
            .join(device)
            .join(plugin)
            .join("config"),
    )
}

pub(crate) fn run_commands_enabled(device: &str) -> bool {
    let commands = read(device, RUN_COMMAND_PLUGIN)
        .get(GENERAL, "commands")
        .map(|text| parse_commands(&text))
        .unwrap_or_default();

    RUN_COMMANDS
        .iter()
        .all(|(id, _, _)| commands.contains_key(*id))
}

pub(crate) fn set_run_commands(device: &str, on: bool) -> std::io::Result<()> {
    let mut ini = read(device, RUN_COMMAND_PLUGIN);

    let mut commands = ini
        .get(GENERAL, "commands")
        .map(|text| parse_commands(&text))
        .unwrap_or_default();

    for (id, name, command) in RUN_COMMANDS {
        if on {
            commands.insert(id.to_string(), json!({ "name": name, "command": command }));
        } else {
            commands.remove(id);
        }
    }

    let text = Json::Object(commands).to_string();

    ini.set(GENERAL, "commands", &quote(&text));

    write(device, RUN_COMMAND_PLUGIN, &ini)
}

// the plugin's own default: on, under its new key or the old one
pub(crate) fn clipboard_auto_sync(device: &str) -> bool {
    let ini = read(device, CLIPBOARD_PLUGIN);

    ini.get(GENERAL, "autoShare")
        .or_else(|| ini.get(GENERAL, "sendUnknown"))
        .is_none_or(|value| value == "true")
}

pub(crate) fn set_clipboard_auto_sync(device: &str, on: bool) -> std::io::Result<()> {
    let mut ini = read(device, CLIPBOARD_PLUGIN);

    ini.set(GENERAL, "autoShare", if on { "true" } else { "false" });

    write(device, CLIPBOARD_PLUGIN, &ini)
}

fn parse_commands(text: &str) -> Map<String, Json> {
    match serde_json::from_str(text) {
        Ok(Json::Object(commands)) => commands,
        _ => Map::new(),
    }
}

fn read(device: &str, plugin: &str) -> Ini {
    plugin_config(device, plugin)
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|text| Ini::parse(&text))
        .unwrap_or_default()
}

fn write(device: &str, plugin: &str, ini: &Ini) -> std::io::Result<()> {
    let Some(path) = plugin_config(device, plugin) else {
        return Err(std::io::Error::other("no home folder"));
    };

    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }

    fs::write(path, ini.render())
}

/*
 * a QSettings ini file, kept line by line so whatever kdeconnect wrote that
 * amane doesn't know about is written back as it was
 */
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Ini {
    // each section with its lines; keys before any section belong to General
    sections: Vec<(String, Vec<Line>)>,
}

#[derive(Debug, Clone, PartialEq)]
enum Line {
    // a key and its value as written, still quoted or escaped
    Entry(String, String),

    // a comment or blank line
    Other(String),
}

impl Ini {
    pub(crate) fn parse(text: &str) -> Self {
        let mut ini = Self::default();

        let mut section = GENERAL.to_string();

        for line in text.lines() {
            let trimmed = line.trim();

            if let Some(name) = trimmed
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            {
                section = name.to_string();

                ini.section(&section);

                continue;
            }

            let entry = match trimmed.split_once('=') {
                Some((key, value)) if !trimmed.starts_with(';') && !trimmed.starts_with('#') => {
                    Line::Entry(key.trim().to_string(), value.trim().to_string())
                }
                _ => Line::Other(line.to_string()),
            };

            ini.section(&section).push(entry);
        }

        ini
    }

    // a value read the way QSettings reads it: unquoted, unescaped
    pub(crate) fn get(&self, section: &str, key: &str) -> Option<String> {
        let (_, lines) = self.sections.iter().find(|(name, _)| name == section)?;

        lines.iter().find_map(|line| match line {
            Line::Entry(name, value) if name == key => Some(unescape(value)),
            _ => None,
        })
    }

    // `raw` is written as is, so a string must come through quote() first
    pub(crate) fn set(&mut self, section: &str, key: &str, raw: &str) {
        let lines = self.section(section);

        for line in lines.iter_mut() {
            if let Line::Entry(name, value) = line
                && name == key
            {
                *value = raw.to_string();

                return;
            }
        }

        lines.push(Line::Entry(key.to_string(), raw.to_string()));
    }

    fn section(&mut self, name: &str) -> &mut Vec<Line> {
        let index = match self
            .sections
            .iter()
            .position(|(section, _)| section == name)
        {
            Some(index) => index,
            None => {
                self.sections.push((name.to_string(), Vec::new()));

                self.sections.len() - 1
            }
        };

        &mut self.sections[index].1
    }

    pub(crate) fn render(&self) -> String {
        let mut text = String::new();

        for (name, lines) in &self.sections {
            // a blank line between sections, as QSettings writes them
            if !text.is_empty() && !text.ends_with("\n\n") {
                text.push('\n');
            }

            text.push_str(&format!("[{name}]\n"));

            for line in lines {
                match line {
                    Line::Entry(key, value) => text.push_str(&format!("{key}={value}\n")),
                    Line::Other(other) if other.trim().is_empty() => {}
                    Line::Other(other) => text.push_str(&format!("{other}\n")),
                }
            }
        }

        text
    }
}

/*
 * a string the way QSettings writes one that could be mistaken for a list:
 * in quotes, with quotes, backslashes and control characters escaped
 */
pub(crate) fn quote(text: &str) -> String {
    let mut quoted = String::from('"');

    for ch in text.chars() {
        match ch {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            ch if (ch as u32) < 0x20 => quoted.push_str(&format!("\\x{:x}", ch as u32)),
            ch => quoted.push(ch),
        }
    }

    quoted.push('"');

    quoted
}

/*
 * reads "quoted" and @ByteArray(...) values and bare ones; escapes work
 * anywhere, and quotes only group, as in QSettings
 */
fn unescape(raw: &str) -> String {
    let body = raw
        .strip_prefix("@ByteArray(")
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or(raw);

    let mut text = String::new();

    let mut chars = body.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '"' => {}
            '\\' => match chars.next() {
                Some('n') => text.push('\n'),
                Some('r') => text.push('\r'),
                Some('t') => text.push('\t'),
                Some('0') => text.push('\0'),
                Some('x') => {
                    let mut hex = String::new();

                    while let Some(digit) = chars.peek().filter(|digit| digit.is_ascii_hexdigit()) {
                        hex.push(*digit);

                        chars.next();
                    }

                    if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        text.push(ch);
                    }
                }
                Some(other) => text.push(other),
                None => {}
            },
            ch => text.push(ch),
        }
    }

    text
}

#[cfg(test)]
mod tests {
    use super::*;

    // what the quickshell version left on disk, written by QSettings itself
    const WRITTEN_BY_QSETTINGS: &str = r#"[General]
commands="{\"{c1a0b5e2-0000-4a00-8000-caelestia0001}\":{\"command\":\"loginctl lock-session\",\"name\":\"Lock screen\"},\"{custom}\":{\"command\":\"echo hi\",\"name\":\"Mine\"}}"
"#;

    #[test]
    fn reads_what_qsettings_wrote() {
        let ini = Ini::parse(WRITTEN_BY_QSETTINGS);

        let commands = parse_commands(&ini.get(GENERAL, "commands").unwrap());

        assert_eq!(
            commands["{c1a0b5e2-0000-4a00-8000-caelestia0001}"]["command"],
            "loginctl lock-session"
        );
        assert_eq!(commands["{custom}"]["name"], "Mine");
    }

    #[test]
    fn a_quoted_value_reads_back_the_same() {
        let text = r#"{"a":"say \"hi\"\n","b":"C:\path"}"#;

        assert_eq!(unescape(&quote(text)), text);
    }

    #[test]
    fn reads_byte_arrays_and_bare_values() {
        assert_eq!(unescape(r#"@ByteArray({\"a\":1})"#), r#"{"a":1}"#);
        assert_eq!(unescape("false"), "false");
        assert_eq!(unescape(r"\x41\x42"), "AB");
    }

    #[test]
    fn keeps_what_it_doesnt_know() {
        let mut ini = Ini::parse("[General]\nautoShare=true\nother=1\n\n[Extra]\nkey=value\n");

        ini.set(GENERAL, "autoShare", "false");

        let text = ini.render();

        assert!(text.contains("autoShare=false"));
        assert!(text.contains("other=1"));
        assert!(text.contains("[Extra]\nkey=value"));
        assert_eq!(text.matches("autoShare").count(), 1);
    }

    #[test]
    fn a_new_file_gets_a_general_section() {
        let mut ini = Ini::default();

        ini.set(GENERAL, "autoShare", "false");

        assert_eq!(ini.render(), "[General]\nautoShare=false\n");
    }

    #[test]
    fn the_remote_leaves_hand_added_commands() {
        let mut commands = parse_commands(
            &Ini::parse(WRITTEN_BY_QSETTINGS)
                .get(GENERAL, "commands")
                .unwrap(),
        );

        for (id, _, _) in RUN_COMMANDS {
            commands.remove(id);
        }

        assert_eq!(commands.len(), 1);
        assert!(commands.contains_key("{custom}"));
    }
}
