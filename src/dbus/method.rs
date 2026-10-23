use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type;
use zbus::{MatchRule, Message};

use super::{Argument, Bus, Value, convert};

// a call another program made to amane, waiting for its answer
pub struct Method {
    pub(crate) connection: &'static Connection,

    pub(crate) message: Message,

    pub(crate) name: String,

    pub(crate) arguments: Vec<Value>,
}

// empty when the bus could not be reached
pub struct Methods {
    connection: Option<&'static Connection>,

    messages: Option<MessageIterator>,
}

impl Method {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn arguments(&self) -> &[Value] {
        &self.arguments
    }

    pub(crate) fn interface(&self) -> Option<String> {
        let header = self.message.header();

        header.interface().map(|interface| interface.to_string())
    }

    // the caller's unique name like :1.42, to notice when it leaves the bus
    pub fn sender(&self) -> Option<String> {
        let header = self.message.header();

        header.sender().map(|name| name.to_string())
    }

    // a failed reply means the caller already left, so there is nobody to tell
    pub fn reply(&self, arguments: &[Argument]) {
        let header = self.message.header();

        if arguments.is_empty() {
            let _ = self.connection.reply(&header, &());

            return;
        }

        let body = convert::body(arguments);

        let _ = self.connection.reply(&header, &body);
    }
}

impl Bus {
    /*
     * waits for each call to this path and interface,
     * so call it from a service thread, and before own(),
     * so the first calls are not missed
     */
    pub fn methods(&self, path: &str, interface: &str) -> Methods {
        let rule = MatchRule::builder()
            .msg_type(Type::MethodCall)
            .path(path)
            .expect("failed to watch method calls: bad path")
            .interface(interface)
            .expect("failed to watch method calls: bad interface name")
            .build();

        self.watch(rule)
    }

    // every interface at this path, for a server that also answers Introspect
    pub(crate) fn methods_at(&self, path: &str) -> Methods {
        let rule = MatchRule::builder()
            .msg_type(Type::MethodCall)
            .path(path)
            .expect("failed to watch method calls: bad path")
            .build();

        self.watch(rule)
    }

    fn watch(&self, rule: MatchRule<'_>) -> Methods {
        let messages = self
            .connection
            .and_then(|connection| MessageIterator::for_match_rule(rule, connection, None).ok());

        Methods {
            connection: self.connection,
            messages,
        }
    }
}

impl Iterator for Methods {
    type Item = Method;

    fn next(&mut self) -> Option<Method> {
        loop {
            // nothing to wait on without a bus
            let (Some(connection), Some(messages)) = (self.connection, self.messages.as_mut())
            else {
                return None;
            };

            // a message that fails to read is skipped, the next one may be fine
            let Ok(message) = messages.next()? else {
                continue;
            };

            let name = message.header().member().map(|name| name.to_string());

            let method = Method {
                connection,

                name: name.unwrap_or_default(),
                arguments: convert::arguments(&message),

                message,
            };

            return Some(method);
        }
    }
}
