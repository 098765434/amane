use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Mutex, PoisonError};

use crate::changes;
use crate::launch::Launch;

pub struct Lines {
    // none when the command couldn't start: then there are no lines
    child: Option<Child>,

    reader: Option<BufReader<ChildStdout>>,
}

impl Iterator for Lines {
    type Item = String;

    fn next(&mut self) -> Option<String> {
        let mut line = String::new();

        let read = self.reader.as_mut()?.read_line(&mut line).ok()?;

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
        if let Some(child) = &mut self.child {
            let _ = child.kill();

            let _ = child.wait();
        }
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

/*
 * starts a program and moves on without waiting for it, in a scope of its own
 * (crate::launch); one that can't start is logged
 */
pub fn spawn(command: &str) {
    spawn_with_token(command, None, None);
}

pub(crate) fn spawn_with_token(command: &str, token: Option<&str>, app_id: Option<&str>) {
    let launched = Launch::shell(command).token(token).app_id(app_id).spawn();

    if let Err(err) = launched {
        eprintln!("amane: {err}");
    }
}

// a program to start once the compositor has given it a token, and the app it is
pub(crate) struct Activated {
    pub command: String,
    pub app_id: Option<String>,
}

// programs to start once the compositor has given each a token, in the order asked
pub(crate) static ACTIVATED: Mutex<Vec<Activated>> = Mutex::new(Vec::new());

/*
 * starts a program the way a launcher should: the compositor hands it a token that lets
 * its window take the focus, or brings it forward if it is already running. Call it from
 * input, like a click, since the token stands for that input
 */
pub fn spawn_activated(command: &str) {
    spawn_activated_app(command, None);
}

// the same, with the app it is, which names its scope
pub(crate) fn spawn_activated_app(command: &str, app_id: Option<&str>) {
    // without an event loop there is no compositor to ask
    if !changes::has_loop() {
        spawn_with_token(command, None, app_id);

        return;
    }

    ACTIVATED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Activated {
            command: String::from(command),
            app_id: app_id.map(String::from),
        });

    changes::wake();
}

/*
 * waits for the command to finish, so call it from a service thread, not from view();
 * a command that can't run gives no output, and is logged
 */
pub fn output(command: &str) -> String {
    let output = match shell(command).output() {
        Ok(output) => output,
        Err(err) => {
            eprintln!("amane: couldn't run {command}: {err}");

            return String::new();
        }
    };

    let text = String::from_utf8_lossy(&output.stdout);

    // commands end their output with a newline that nobody wants on screen
    let trimmed = text.trim_end_matches('\n');

    String::from(trimmed)
}

// streams each line a long-running command prints, until it exits; none if it can't start
pub fn lines(command: &str) -> Lines {
    let mut child = match shell(command).stdout(Stdio::piped()).spawn() {
        Ok(child) => child,
        Err(err) => {
            eprintln!("amane: couldn't start {command}: {err}");

            return Lines {
                child: None,
                reader: None,
            };
        }
    };

    let reader = child.stdout.take().map(BufReader::new);

    Lines {
        child: Some(child),

        reader,
    }
}
