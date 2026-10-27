use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

/*
 * one run of polkit's PAM helper, which since polkit 124 is a system
 * service behind this socket rather than a setuid program: the agent
 * connects, names the user and the cookie, and then the helper speaks the
 * PAM conversation one line at a time, ending with SUCCESS or FAILURE. the
 * helper tells polkitd the answer itself, so the agent only relays prompts
 */
pub const SOCKET: &str = "/run/polkit/agent-helper.socket";

// what the helper said
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    // something to type: a password when echo is off
    Prompt { text: String, echo: bool },

    // a line to show, like a fingerprint reader asking for a finger
    Info(String),

    // what went wrong, from PAM
    Error(String),

    // the helper finished, and whether the user was authenticated
    Done(bool),
}

pub struct Session {
    stream: Mutex<UnixStream>,

    done: Arc<AtomicBool>,
}

impl Session {
    // connects to the helper and starts its conversation for this user and cookie
    pub fn start(
        user: &str,
        cookie: &str,
        on_event: impl Fn(Event) + Send + 'static,
    ) -> io::Result<Self> {
        let mut stream = UnixStream::connect(SOCKET)?;

        stream.write_all(format!("{user}\n{cookie}\n").as_bytes())?;

        Self::from_stream(stream, on_event)
    }

    // relays the helper's lines from an open stream, until it closes
    pub fn from_stream(
        stream: UnixStream,
        on_event: impl Fn(Event) + Send + 'static,
    ) -> io::Result<Self> {
        let reading = stream.try_clone()?;
        let done = Arc::new(AtomicBool::new(false));
        let finished = Arc::clone(&done);

        thread::Builder::new()
            .name(String::from("amane-polkit-helper"))
            .spawn(move || {
                let mut lines = BufReader::new(reading).lines();

                while let Some(Ok(line)) = lines.next() {
                    let Some(event) = parse_line(&line) else {
                        continue;
                    };

                    let ends = matches!(event, Event::Done(_));

                    if ends {
                        finished.store(true, Ordering::Release);
                    }

                    on_event(event);

                    if ends {
                        return;
                    }
                }

                // the helper went away without a verdict, like when it was cancelled
                if !finished.swap(true, Ordering::AcqRel) {
                    on_event(Event::Done(false));
                }
            })?;

        Ok(Self {
            stream: Mutex::new(stream),
            done,
        })
    }

    // answers the prompt the helper is waiting on
    pub fn answer(&self, text: &str) -> io::Result<()> {
        let mut stream = self.stream.lock().unwrap_or_else(PoisonError::into_inner);

        stream.write_all(text.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()
    }

    #[cfg(test)]
    pub fn is_done(&self) -> bool {
        self.done.load(Ordering::Acquire)
    }

    // ends the conversation, which the helper takes as a failure
    pub fn close(&self) {
        self.done.store(true, Ordering::Release);

        let stream = self.stream.lock().unwrap_or_else(PoisonError::into_inner);

        let _ = stream.shutdown(std::net::Shutdown::Both);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}

// one line from the helper; an unknown one is ignored, as libpolkit-agent ignores none it knows
pub fn parse_line(line: &str) -> Option<Event> {
    let line = line.trim_end_matches(['\r', '\n']);

    let (kind, rest) = match line.split_once(' ') {
        Some((kind, rest)) => (kind, rest),
        None => (line, ""),
    };

    let text = || unescape(rest);

    Some(match kind {
        "PAM_PROMPT_ECHO_OFF" => Event::Prompt {
            text: text(),
            echo: false,
        },
        "PAM_PROMPT_ECHO_ON" => Event::Prompt {
            text: text(),
            echo: true,
        },
        "PAM_ERROR_MSG" => Event::Error(text()),
        "PAM_TEXT_INFO" => Event::Info(text()),
        "SUCCESS" => Event::Done(true),
        "FAILURE" => Event::Done(false),
        _ => return None,
    })
}

// undoes the helper's g_strescape: \n, \t, \\, \" and octal \ooo
pub fn unescape(text: &str) -> String {
    let mut out = Vec::with_capacity(text.len());
    let mut bytes = text.bytes().peekable();

    while let Some(byte) = bytes.next() {
        if byte != b'\\' {
            out.push(byte);

            continue;
        }

        match bytes.next() {
            Some(b'n') => out.push(b'\n'),
            Some(b't') => out.push(b'\t'),
            Some(b'r') => out.push(b'\r'),
            Some(b'b') => out.push(0x08),
            Some(b'f') => out.push(0x0c),
            Some(b'v') => out.push(0x0b),
            Some(b'\\') => out.push(b'\\'),
            Some(b'"') => out.push(b'"'),
            Some(digit @ b'0'..=b'7') => {
                let mut value = u32::from(digit - b'0');

                for _ in 0..2 {
                    match bytes.peek() {
                        Some(next @ b'0'..=b'7') => {
                            value = value * 8 + u32::from(next - b'0');
                            bytes.next();
                        }
                        _ => break,
                    }
                }

                out.push((value & 0xff) as u8);
            }
            Some(other) => {
                out.push(b'\\');
                out.push(other);
            }
            None => out.push(b'\\'),
        }
    }

    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    #[test]
    fn helper_lines_become_events() {
        assert_eq!(
            parse_line("PAM_PROMPT_ECHO_OFF Password: "),
            Some(Event::Prompt {
                text: String::from("Password: "),
                echo: false
            })
        );
        assert_eq!(
            parse_line("PAM_TEXT_INFO Place your finger on the reader\\n"),
            Some(Event::Info(String::from(
                "Place your finger on the reader\n"
            )))
        );
        assert_eq!(parse_line("SUCCESS"), Some(Event::Done(true)));
        assert_eq!(parse_line("FAILURE\n"), Some(Event::Done(false)));
        assert_eq!(parse_line("something else"), None);
    }

    #[test]
    fn escapes_are_undone() {
        assert_eq!(unescape("a\\\\b\\\"c\\n"), "a\\b\"c\n");
        assert_eq!(unescape("\\303\\270l"), "øl");
        assert_eq!(unescape("plain"), "plain");
        assert_eq!(unescape("odd\\"), "odd\\");
    }

    #[test]
    fn a_session_relays_prompts_and_answers() {
        let (ours, theirs) = UnixStream::pair().expect("socket pair");
        let (events, seen) = mpsc::channel();

        let session =
            Session::from_stream(ours, move |event| events.send(event).unwrap()).expect("session");

        // a fake helper: asks for a password, checks it, says so
        let helper = thread::spawn(move || {
            let mut theirs = theirs;
            theirs
                .write_all(b"PAM_PROMPT_ECHO_OFF Password: \n")
                .unwrap();
            let mut reader = BufReader::new(theirs.try_clone().unwrap());
            let mut answer = String::new();
            reader.read_line(&mut answer).unwrap();
            let verdict: &[u8] = if answer == "hunter2\n" {
                b"SUCCESS\n"
            } else {
                b"FAILURE\n"
            };
            theirs.write_all(verdict).unwrap();
            let mut rest = Vec::new();
            let _ = theirs.read_to_end(&mut rest);
        });

        let wait = Duration::from_secs(5);
        assert_eq!(
            seen.recv_timeout(wait).unwrap(),
            Event::Prompt {
                text: String::from("Password: "),
                echo: false
            }
        );
        session.answer("hunter2").unwrap();
        assert_eq!(seen.recv_timeout(wait).unwrap(), Event::Done(true));
        assert!(session.is_done());
        drop(session);
        helper.join().unwrap();
    }

    #[test]
    fn a_closed_helper_counts_as_failure() {
        let (ours, theirs) = UnixStream::pair().expect("socket pair");
        let (events, seen) = mpsc::channel();
        let session =
            Session::from_stream(ours, move |event| events.send(event).unwrap()).expect("session");
        drop(theirs);
        assert_eq!(
            seen.recv_timeout(Duration::from_secs(5)).unwrap(),
            Event::Done(false)
        );
        assert!(session.is_done());
    }
}
