#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo)]

//! Starting programs: never a panic, the activation token passed on, and each program in a
//! systemd scope of its own (`app-amane-<app>-<pid>.scope` under `app.slice`), so restarting
//! the program that started it — a shell's own service — doesn't take it down too.
//!
//! The scope is asked for over the session bus right after the program starts, on amane's
//! worker thread, so the caller never waits on systemd. Without systemd the program stays where
//! it was started, which is said once.

use std::fmt;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use zbus::zvariant::Value;

use crate::dbus::Bus;
use crate::services::worker;

/// A program to start.
#[derive(Clone, Debug)]
pub struct Launch {
    program: Program,
    token: Option<String>,
    app_id: Option<String>,
}

#[derive(Clone, Debug)]
enum Program {
    // through `sh -c`, so pipes, globs and ~ work as in a terminal
    Shell(String),
    Argv(Vec<String>),
}

/// Why a program didn't start.
#[derive(Debug)]
pub struct LaunchError {
    command: String,
    error: std::io::Error,
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "couldn't start {}: {}", self.command, self.error)
    }
}

impl std::error::Error for LaunchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

impl Launch {
    /// A command line run through `sh -c`.
    pub fn shell(command: &str) -> Self {
        Self {
            program: Program::Shell(command.to_owned()),
            token: None,
            app_id: None,
        }
    }

    /// A program and its arguments, run as they are.
    pub fn argv<S: AsRef<str>>(argv: &[S]) -> Self {
        Self {
            program: Program::Argv(argv.iter().map(|arg| arg.as_ref().to_owned()).collect()),
            token: None,
            app_id: None,
        }
    }

    /// The activation token the program's first window shows with
    /// (`XDG_ACTIVATION_TOKEN` and `DESKTOP_STARTUP_ID`).
    pub fn token(mut self, token: Option<&str>) -> Self {
        self.token = token.map(str::to_owned);
        self
    }

    /// The application its scope is named for, like `org.mozilla.firefox`; by default the
    /// program's file name.
    pub fn app_id(mut self, app_id: Option<&str>) -> Self {
        self.app_id = app_id.map(str::to_owned);
        self
    }

    /// Starts it and moves on without waiting; returns its process id.
    pub fn spawn(self) -> Result<u32, LaunchError> {
        let mut command = self.command();
        let mut child = command.spawn().map_err(|error| LaunchError {
            command: self.describe(),
            error,
        })?;
        let pid = child.id();

        // a finished child that nobody waits for stays behind as a zombie
        thread::spawn(move || child.wait());

        let unit = unit_name(self.scope_name().as_deref(), pid);
        worker::run(move || move_to_scope(&unit, pid));

        Ok(pid)
    }

    fn command(&self) -> Command {
        let mut command = match &self.program {
            Program::Shell(line) => {
                let mut shell = Command::new("sh");
                shell.arg("-c").arg(line);
                shell
            }
            Program::Argv(argv) => {
                let mut command = Command::new(argv.first().map_or("", String::as_str));
                command.args(argv.iter().skip(1));
                command
            }
        };

        // the token amane was started with is used up; a program it starts gets its own
        command
            .env_remove("XDG_ACTIVATION_TOKEN")
            .env_remove("DESKTOP_STARTUP_ID");
        if let Some(token) = &self.token {
            command
                .env("XDG_ACTIVATION_TOKEN", token)
                .env("DESKTOP_STARTUP_ID", token);
        }
        command
    }

    fn describe(&self) -> String {
        match &self.program {
            Program::Shell(line) => line.clone(),
            Program::Argv(argv) => argv.join(" "),
        }
    }

    // the app id, or the program's file name: `env A=1 foo --x` is foo
    fn scope_name(&self) -> Option<String> {
        if let Some(id) = &self.app_id {
            return Some(id.strip_suffix(".desktop").unwrap_or(id).to_owned());
        }
        let first = match &self.program {
            Program::Argv(argv) => argv.first().cloned(),
            Program::Shell(line) => line
                .split_whitespace()
                .find(|word| *word != "env" && !word.contains('='))
                .map(str::to_owned),
        }?;
        Some(first.rsplit('/').next().unwrap_or(&first).to_owned())
    }
}

/// `app-amane-<name>-<pid>.scope`, with what a unit name can't hold made `_`.
fn unit_name(name: Option<&str>, pid: u32) -> String {
    let name: String = name
        .unwrap_or("program")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':') {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    let name = if name.is_empty() { "program" } else { &name };
    format!("app-amane-{name}-{pid}.scope")
}

static NO_SCOPES_SAID: AtomicBool = AtomicBool::new(false);

fn move_to_scope(unit: &str, pid: u32) {
    let Some(connection) = Bus::session().connection else {
        say_once("no session bus");
        return;
    };

    let properties: Vec<(&str, Value)> = vec![
        ("PIDs", Value::from(vec![pid])),
        ("Slice", Value::from("app.slice")),
        ("CollectMode", Value::from("inactive-or-failed")),
    ];
    let aux: Vec<(&str, Vec<(&str, Value)>)> = Vec::new();

    let reply = connection.call_method(
        Some("org.freedesktop.systemd1"),
        "/org/freedesktop/systemd1",
        Some("org.freedesktop.systemd1.Manager"),
        "StartTransientUnit",
        &(unit, "fail", properties, aux),
    );
    if let Err(err) = reply {
        say_once(&err.to_string());
    }
}

fn say_once(why: &str) {
    if !NO_SCOPES_SAID.swap(true, Ordering::Relaxed) {
        eprintln!("amane: programs stay in this program's own scope: {why}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_scope_for_the_app() {
        let launch = Launch::shell("env GDK_BACKEND=wayland /usr/bin/foot --server");
        assert_eq!(launch.scope_name().as_deref(), Some("foot"));

        let launch = Launch::argv(&["code", "--new-window"]).app_id(Some("code.desktop"));
        assert_eq!(launch.scope_name().as_deref(), Some("code"));

        assert_eq!(
            unit_name(Some("org.mozilla.firefox"), 42),
            "app-amane-org.mozilla.firefox-42.scope"
        );
        assert_eq!(unit_name(Some("my app/€"), 7), "app-amane-my_app__-7.scope");
        assert_eq!(unit_name(None, 7), "app-amane-program-7.scope");
    }

    #[test]
    fn passes_the_token_and_drops_its_own() {
        let command = Launch::argv(&["true"]).token(Some("abc")).command();
        let env: Vec<_> = command.get_envs().collect();
        assert!(env.contains(&("XDG_ACTIVATION_TOKEN".as_ref(), Some("abc".as_ref()))));
        assert!(env.contains(&("DESKTOP_STARTUP_ID".as_ref(), Some("abc".as_ref()))));

        let command = Launch::shell("true").command();
        let env: Vec<_> = command.get_envs().collect();
        assert!(env.contains(&("XDG_ACTIVATION_TOKEN".as_ref(), None)));
    }

    #[test]
    fn a_missing_program_is_an_error_not_a_panic() {
        let err = Launch::argv(&["/nonexistent/amane-test-program"])
            .spawn()
            .expect_err("it can't start");
        assert!(err.to_string().contains("/nonexistent/amane-test-program"));
    }
}
