use std::path::{Path, PathBuf};

use crate::spawn;

use super::{exec, mime};

#[derive(Debug, Clone, PartialEq)]
pub struct DesktopApp {
    pub(crate) name: String,
    pub(crate) exec: String,
    // the Exec line as written, with its %f style placeholders
    pub(crate) exec_codes: String,
    // the kinds of file it opens (MimeType)
    pub(crate) mime_types: Vec<String>,
    pub(crate) icon: Option<String>,
    pub(crate) icon_path: Option<PathBuf>,
    pub(crate) description: Option<String>,
    pub(crate) categories: Vec<String>,
    pub(crate) keywords: Vec<String>,
    pub(crate) actions: Vec<DesktopAction>,
}

// one of an app's [Desktop Action] sections: another way to start it, like "New Private Window"
#[derive(Debug, Clone, PartialEq)]
pub struct DesktopAction {
    pub(crate) name: String,
    pub(crate) exec: String,
    pub(crate) icon: Option<String>,
}

impl DesktopApp {
    pub fn name(&self) -> &str {
        &self.name
    }

    // the command line, without the %f style placeholders
    pub fn exec(&self) -> &str {
        &self.exec
    }

    // the icon's name in the icon theme, like "firefox"
    pub fn icon(&self) -> Option<&str> {
        self.icon.as_deref()
    }

    // the image file for the icon, a png when the theme has one
    pub fn icon_path(&self) -> Option<&Path> {
        self.icon_path.as_deref()
    }

    // the entry's Comment, or its GenericName like "Web Browser" when it has none
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    // the freedesktop Categories, like "Development" or "Network" (no trailing empty)
    pub fn categories(&self) -> &[String] {
        &self.categories
    }

    // the entry's Keywords, extra search terms the author added
    pub fn keywords(&self) -> &[String] {
        &self.keywords
    }

    // the entry's Actions, in the order it lists them, each with its own command line
    pub fn actions(&self) -> &[DesktopAction] {
        &self.actions
    }

    // starts the program and moves on without waiting for it
    pub fn launch(&self) {
        spawn(&self.exec);
    }

    // the kinds of file it says it opens, like "text/markdown"
    pub fn mime_types(&self) -> &[String] {
        &self.mime_types
    }

    // whether it opens `file`, by the kind of file its name says it is
    pub fn opens(&self, file: &Path) -> bool {
        mime::mime_type(file).is_some_and(|kind| mime::opens(&self.mime_types, &kind))
    }

    // the command lines that open `files` with it, one a file for a program that takes one at a time
    pub fn commands_with(&self, files: &[PathBuf]) -> Vec<String> {
        exec::fill_field_codes(&self.exec_codes, files)
    }

    // starts it on `files`, and moves on without waiting
    pub fn launch_with(&self, files: &[PathBuf]) {
        for command in self.commands_with(files) {
            spawn(&command);
        }
    }
}

impl DesktopAction {
    pub fn name(&self) -> &str {
        &self.name
    }

    // the command line, without the %f style placeholders
    pub fn exec(&self) -> &str {
        &self.exec
    }

    // the icon's name in the icon theme, or none to use the app's
    pub fn icon(&self) -> Option<&str> {
        self.icon.as_deref()
    }

    // starts the action and moves on without waiting for it
    pub fn launch(&self) {
        spawn(&self.exec);
    }
}
