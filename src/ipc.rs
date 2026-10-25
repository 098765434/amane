use std::collections::HashMap;
use std::env;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

// the call a second launch of an app sends to the one already running
pub(crate) const ACTIVATE: &str = "activate";

pub struct IpcCall {
    pub(crate) name: String,

    pub(crate) arguments: Vec<String>,
}

type Handler = fn(&[String]) -> String;

#[derive(Default)]
pub struct IpcHandlers {
    pub by_name: HashMap<String, Handler>,

    // runs when the app is launched again, with that launch's arguments
    pub on_activate: Option<fn(&[String])>,
}

impl IpcCall {
    pub fn new(name: &str, arguments: &[String]) -> Self {
        Self {
            name: String::from(name),
            arguments: arguments.to_vec(),
        }
    }

    // the name goes on the first line, then each argument on its own line
    pub fn write(&self, writer: &mut impl Write) -> io::Result<()> {
        writeln!(writer, "{}", self.name)?;

        for argument in &self.arguments {
            writeln!(writer, "{argument}")?;
        }

        Ok(())
    }

    // reads until the sender closes its side, which marks the end of the call
    pub(crate) fn read(reader: impl Read) -> io::Result<Self> {
        let mut lines = BufReader::new(reader).lines();

        let Some(name) = lines.next() else {
            return Err(io::ErrorKind::UnexpectedEof.into());
        };

        let name = name?;

        let arguments = lines.collect::<io::Result<Vec<String>>>()?;

        Ok(Self { name, arguments })
    }
}

impl IpcHandlers {
    pub fn insert(&mut self, name: &str, handler: Handler) {
        self.by_name.insert(String::from(name), handler);
    }

    // the reply is whatever the handler returns, so an unknown name answers in words too
    pub fn run(&self, call: &IpcCall) -> String {
        let Some(handler) = self.by_name.get(&call.name) else {
            return format!("no handler named {}", call.name);
        };

        handler(&call.arguments)
    }
}

// the shell's socket, the one an app without its own app-id listens on
pub fn ipc_socket() -> PathBuf {
    ipc_socket_for("amane")
}

/*
 * the socket an app with this app-id listens on: amane.sock for "amane", the shell's,
 * and amane-<app id>.sock for the rest, so apps and the shell run side by side;
 * "amane-settings" listens on amane-settings.sock
 */
pub fn ipc_socket_for(app_id: &str) -> PathBuf {
    // every wayland session sets this, and it is private to the logged in user
    let runtime_directory =
        env::var_os("XDG_RUNTIME_DIR").expect("failed to find socket: XDG_RUNTIME_DIR is not set");

    let name = match app_id.strip_prefix("amane") {
        Some("") => String::from("amane.sock"),
        Some(rest) if rest.starts_with('-') => format!("{app_id}.sock"),
        _ => format!("amane-{app_id}.sock"),
    };

    PathBuf::from(runtime_directory).join(name)
}

/*
 * calls a handler in the running app with this app-id, like the settings app asking the
 * shell to reload; waits for the reply, which the other app gives within a second
 */
pub fn call(app_id: &str, name: &str, arguments: &[String]) -> io::Result<String> {
    // a newline would split one argument into two on the other side
    let has_newline = arguments.iter().any(|argument| argument.contains('\n'));

    if name.contains('\n') || has_newline {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "names and arguments can't contain newlines",
        ));
    }

    let mut stream = UnixStream::connect(ipc_socket_for(app_id))?;

    IpcCall::new(name, arguments).write(&mut stream)?;

    // closing our sending side is how the other app knows the call is complete
    stream.shutdown(Shutdown::Write)?;

    let mut reply = String::new();

    stream.read_to_string(&mut reply)?;

    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_app_has_its_own_socket() {
        let name = |app_id| ipc_socket_for(app_id).file_name().unwrap().to_owned();

        assert_eq!(name("amane"), "amane.sock");
        assert_eq!(name("amane-settings"), "amane-settings.sock");
        assert_eq!(name("org.example.Notes"), "amane-org.example.Notes.sock");
        assert_eq!(name("amanex"), "amane-amanex.sock");
    }
}
