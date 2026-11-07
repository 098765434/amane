#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo)]

/*
 * status bar apps: a program's own item in the bar, and its activities
 * (something going on now), over the bar's session bus service
 * org.amane.StatusBar1. The protocol types are shared with the bar that
 * serves it; the client is what a program uses
 */

mod client;
mod protocol;

pub use client::{LiveActivity, StatusItem};
pub use protocol::{
    Accepts, ActivityButton, ActivityContent, ActivityEvent, BUS_NAME, BarEdge, Dismiss, INTERFACE,
    ItemButton, ItemEvent, ItemIcon, ItemSpec, MenuEntry, PATH, ProtocolError, Tint, Trailing,
    VERSION, encode,
};
