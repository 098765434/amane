use std::collections::HashMap;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;

use zbus::MatchRule;
use zbus::blocking::MessageIterator;
use zbus::message::Type;
use zbus::zvariant::{OwnedValue, Value};

use crate::Bus;
use crate::changes;

// each request names its own answer, so two dialogs open at once don't mix theirs up
static REQUESTS: AtomicU32 = AtomicU32::new(0);

/*
 * the desktop's own file dialog, through the file chooser portal, so it looks like every
 * other app's: FileDialog::open("Open a picture").filter("Images", &["*.png"]).show(...)
 */
pub struct FileDialog {
    save: bool,
    title: String,

    // a name and the patterns it covers, like ("Images", ["*.png", "*.jpg"])
    filters: Vec<(String, Vec<String>)>,

    multiple: bool,
    directory: bool,

    // a save dialog's suggested file name, and the folder either kind starts in
    name: Option<String>,
    folder: Option<PathBuf>,
}

impl FileDialog {
    // a dialog that picks files that are there
    pub fn open(title: &str) -> Self {
        Self::new(false, title)
    }

    // a dialog that picks where to save a file
    pub fn save(title: &str) -> Self {
        Self::new(true, title)
    }

    fn new(save: bool, title: &str) -> Self {
        Self {
            save,
            title: String::from(title),
            filters: Vec::new(),
            multiple: false,
            directory: false,
            name: None,
            folder: None,
        }
    }

    // only shows files that match one of the patterns, which the user picks by name
    pub fn filter(mut self, name: &str, patterns: &[&str]) -> Self {
        let patterns = patterns
            .iter()
            .map(|pattern| String::from(*pattern))
            .collect();

        self.filters.push((String::from(name), patterns));

        self
    }

    // lets the user pick more than one
    pub fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;

        self
    }

    // picks folders instead of files
    pub fn directory(mut self, directory: bool) -> Self {
        self.directory = directory;

        self
    }

    // the file name a save dialog suggests
    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(String::from(name));

        self
    }

    pub fn folder(mut self, folder: impl Into<PathBuf>) -> Self {
        self.folder = Some(folder.into());

        self
    }

    /*
     * shows the dialog and moves on; `then` runs with what the user picked once they
     * have, on the event loop like a handler, and not at all when they cancel
     */
    pub fn show(self, then: impl FnOnce(Vec<PathBuf>) + Send + 'static) {
        // the user takes their time, so this waits on a thread of its own
        thread::spawn(move || {
            let Some(picked) = self.ask() else {
                return;
            };

            changes::later(move || then(picked));
        });
    }

    // none when cancelled, or when there is no portal to ask
    fn ask(&self) -> Option<Vec<PathBuf>> {
        let connection = Bus::session().connection?;

        let token = format!("amane{}", REQUESTS.fetch_add(1, Ordering::Relaxed));

        // the portal answers on a path made from our name on the bus and the token
        let sender = connection
            .unique_name()?
            .trim_start_matches(':')
            .replace('.', "_");

        let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");

        // listening starts before asking, so a quick answer isn't missed
        let rule = MatchRule::builder()
            .msg_type(Type::Signal)
            .interface("org.freedesktop.portal.Request")
            .ok()?
            .member("Response")
            .ok()?
            .path(path.as_str())
            .ok()?
            .build();

        let mut answers = MessageIterator::for_match_rule(rule, connection, None).ok()?;

        let method = if self.save { "SaveFile" } else { "OpenFile" };

        connection
            .call_method(
                Some("org.freedesktop.portal.Desktop"),
                "/org/freedesktop/portal/desktop",
                Some("org.freedesktop.portal.FileChooser"),
                method,
                // no parent window handle: the dialog is not tied to one of ours
                &("", self.title.as_str(), self.options(&token)),
            )
            .ok()?;

        let answer = answers.next()?.ok()?;

        let (response, results): (u32, HashMap<String, OwnedValue>) =
            answer.body().deserialize().ok()?;

        // 1 is cancelled, 2 is anything else that went wrong
        if response != 0 {
            return None;
        }

        let uris: Vec<String> = results.get("uris")?.try_clone().ok()?.try_into().ok()?;

        Some(crate::wayland::dropped::parse(&uris.join("\n")))
    }

    fn options(&self, token: &str) -> HashMap<&'static str, Value<'static>> {
        let mut options: HashMap<&str, Value> = HashMap::new();

        options.insert("handle_token", Value::from(String::from(token)));

        if !self.save {
            options.insert("multiple", Value::from(self.multiple));
            options.insert("directory", Value::from(self.directory));
        }

        if !self.filters.is_empty() {
            // each pattern is a glob, kind 0, rather than a mime type, kind 1
            let filters: Vec<(String, Vec<(u32, String)>)> = self
                .filters
                .iter()
                .map(|(name, patterns)| {
                    let patterns = patterns
                        .iter()
                        .map(|pattern| (0, pattern.clone()))
                        .collect();

                    (name.clone(), patterns)
                })
                .collect();

            options.insert("filters", Value::from(filters));
        }

        if let Some(name) = &self.name {
            options.insert("current_name", Value::from(name.clone()));
        }

        // a path goes as bytes ending in a zero byte
        if let Some(folder) = &self.folder {
            let mut bytes = folder.as_os_str().as_bytes().to_vec();

            bytes.push(0);

            options.insert("current_folder", Value::from(bytes));
        }

        options
    }
}
