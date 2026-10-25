use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Mutex, PoisonError};
use std::thread;

use crate::changes;

pub struct Lines {
    child: Child,

    reader: BufReader<ChildStdout>,
}

impl Iterator for Lines {
    type Item = String;

    fn next(&mut self) -> Option<String> {
        let mut line = String::new();

        let read = self.reader.read_line(&mut line).ok()?;

        // 0 bytes means the command closed its output
        if read == 0 {
            return None;
        }

        let trimmed = line.trim_end_matches('\n');

        Some(String::from(trimmed))
    }
}

// a service that stops listening must not leave the command running
impl Drop for Lines {
    fn drop(&mut self) {
        let _ = self.child.kill();

        let _ = self.child.wait();
    }
}

// through the shell, so pipes, globs and ~ work like they do in a terminal
fn shell(command: &str) -> Command {
    let mut shell = Command::new("sh");

    shell.arg("-c").arg(command);

    // the token amane was started with is used up, a program it starts gets its own
    shell
        .env_remove("XDG_ACTIVATION_TOKEN")
        .env_remove("DESKTOP_STARTUP_ID");

    shell
}

// starts a program and moves on without waiting for it
pub fn spawn(command: &str) {
    spawn_with_token(command, None);
}

pub(crate) fn spawn_with_token(command: &str, token: Option<&str>) {
    let mut shell = shell(command);

    if let Some(token) = token {
        shell
            .env("XDG_ACTIVATION_TOKEN", token)
            .env("DESKTOP_STARTUP_ID", token);
    }

    let mut child = shell.spawn().expect("failed to start command");

    // a finished child that nobody waits for stays behind as a zombie
    thread::spawn(move || child.wait());
}

// programs to start once the compositor has given each a token, in the order asked
pub(crate) static ACTIVATED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/*
 * starts a program the way a launcher should: the compositor hands it a token that lets
 * its window take the focus, or brings it forward if it is already running. Call it from
 * input, like a click, since the token stands for that input
 */
pub fn spawn_activated(command: &str) {
    // without an event loop there is no compositor to ask
    if !changes::has_loop() {
        spawn(command);

        return;
    }

    ACTIVATED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(String::from(command));

    changes::wake();
}

// waits for the command to finish, so call it from a service thread, not from view()
pub fn output(command: &str) -> String {
    let output = shell(command).output().expect("failed to run command");

    let text = String::from_utf8_lossy(&output.stdout);

    // commands end their output with a newline that nobody wants on screen
    let trimmed = text.trim_end_matches('\n');

    String::from(trimmed)
}

// streams each line a long-running command prints, until it exits
pub fn lines(command: &str) -> Lines {
    let mut child = shell(command)
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to start command");

    let stdout = child.stdout.take().expect("failed to read command output");

    Lines {
        child,

        reader: BufReader::new(stdout),
    }
}
