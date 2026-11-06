use std::fs::File;
use std::io::Read;
use std::mem;
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};
use std::thread;

use smithay_client_toolkit::data_device_manager::{
    WritePipe,
    data_device::DataDeviceHandler,
    data_offer::{DataOfferHandler, DragOffer},
    data_source::{DataSourceHandler, DragSource},
};
use wayland_client::{
    Connection, QueueHandle,
    protocol::{
        wl_data_device::WlDataDevice, wl_data_device_manager::DndAction,
        wl_data_source::WlDataSource, wl_surface::WlSurface,
    },
};

use crate::changes;
use crate::clipboard::DragData;

use super::{
    WaylandState,
    clipboard::{TEXT_TYPES, text_type, write_bytes},
};

// file managers offer dragged files as a list of file:// links
const URI_LIST: &str = "text/uri-list";

/*
 * what another program offers is read on a thread of its own, so a slow
 * source never stalls a frame, and handed back here for the event loop
 */
static ARRIVED: Mutex<Vec<Arrived>> = Mutex::new(Vec::new());

enum Arrived {
    // the files held over a window, for the drag that entered as `epoch`
    Held(u64, Vec<PathBuf>),

    // what was let go, none when it couldn't be read
    Dropped(u64, Option<DragData>),
}

// the drags whose data is still being read
#[derive(Default)]
pub(crate) struct Drags {
    // counts each drag that enters, so a list read for one that already left is let go
    epoch: u64,

    // the surface a list is being read for
    reading: Option<(u64, WlSurface)>,

    // where the pointer is meanwhile, so the widget under it is asked once the list arrives
    at: (f64, f64),

    // offers let go, waiting for their data before they are finished
    dropped: Vec<(u64, DragOffer)>,
}

impl DataDeviceHandler for WaylandState {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataDevice,
        x: f64,
        y: f64,
        surface: &WlSurface,
    ) {
        let Some(offer) = self.drag_offer() else {
            return;
        };

        let (has_files, text) = offered(&offer);

        // text is taken anywhere, and whether it lands on a widget is seen when it is let go
        if !has_files {
            offer.accept_mime_type(offer.serial, text.clone());

            if text.is_some() {
                offer.set_actions(DndAction::Copy, DndAction::Copy);
            }

            return;
        }

        self.drags.epoch += 1;
        self.drags.at = (x, y);

        // amane's own drag needs no pipe to itself
        if let Some((_, data)) = &self.dragging_out {
            let files = match data {
                DragData::Files(files) => files.clone(),
                DragData::Text(_) => Vec::new(),
            };

            self.files_held = Some((surface.clone(), files));

            self.files_moved(x, y);

            return;
        }

        // a widget may want to know which files before they are let go, as a dock does
        let Some(file) = self.receive(&offer, String::from(URI_LIST)) else {
            self.files_held = Some((surface.clone(), Vec::new()));

            self.files_moved(x, y);

            return;
        };

        let epoch = self.drags.epoch;

        self.drags.reading = Some((epoch, surface.clone()));

        read_later(file, move |bytes| {
            let list = String::from_utf8_lossy(&bytes.unwrap_or_default()).into_owned();

            Arrived::Held(epoch, parse(&list))
        });
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
        self.drags.reading = None;

        let Some((surface, files)) = self.files_held.take() else {
            return;
        };

        let Some(window) = self.window(&surface) else {
            return;
        };

        if window.pointer.files_left(&files) {
            self.request_frame_on(&surface);
        }
    }

    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, x: f64, y: f64) {
        self.drags.at = (x, y);

        self.files_moved(x, y);
    }

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
        self.files_held = None;
        self.drags.reading = None;

        let Some(offer) = self.drag_offer() else {
            return;
        };

        if let Some((_, data)) = &self.dragging_out {
            let data = data.clone();

            self.let_go(&offer, Some(data));

            return;
        }

        let (has_files, text) = offered(&offer);

        let kind = if has_files {
            Some(String::from(URI_LIST))
        } else {
            text
        };

        let Some(file) = kind.and_then(|kind| self.receive(&offer, kind)) else {
            self.let_go(&offer, None);

            return;
        };

        let epoch = self.drags.epoch;

        self.drags.dropped.push((epoch, offer));

        read_later(file, move |bytes| {
            let data = bytes.map(|bytes| {
                let text = String::from_utf8_lossy(&bytes).into_owned();

                if has_files {
                    DragData::Files(parse(&text))
                } else {
                    DragData::Text(text)
                }
            });

            Arrived::Dropped(epoch, data)
        });
    }
}

// only drags into amane are handled, so the offer's choices need no answer
impl DataOfferHandler for WaylandState {
    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }

    fn selected_action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
    }
}

// text amane copied, or what it is dragging out to another program
impl DataSourceHandler for WaylandState {
    fn accept_mime(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlDataSource,
        _: Option<String>,
    ) {
    }

    fn send_request(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        source: &WlDataSource,
        kind: String,
        pipe: WritePipe,
    ) {
        if let Some((copied, text)) = &self.copied.clipboard
            && copied.inner() == source
        {
            write_bytes(pipe, text.clone().into_bytes());

            return;
        }

        if let Some((dragged, data)) = &self.dragging_out
            && dragged.inner() == source
        {
            let bytes = match data {
                DragData::Files(files) if kind == URI_LIST => uri_list(files),
                DragData::Files(files) => paths(files),
                DragData::Text(text) => text.clone(),
            };

            write_bytes(pipe, bytes.into_bytes());
        }
    }

    // another selection took its place, or the drag was dropped nowhere
    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        if let Some((copied, _)) = &self.copied.clipboard
            && copied.inner() == source
        {
            self.copied.clipboard = None;
        }

        if let Some((dragged, _)) = &self.dragging_out
            && dragged.inner() == source
        {
            self.dragging_out = None;
        }
    }

    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        if let Some((dragged, _)) = &self.dragging_out
            && dragged.inner() == source
        {
            self.dragging_out = None;
        }
    }

    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}

impl WaylandState {
    fn drag_offer(&self) -> Option<DragOffer> {
        self.data_device.as_ref()?.data().drag_offer()
    }

    /*
     * starts dragging data out of the window the left button went down on; the compositor
     * takes over the pointer from here until it is let go
     */
    pub fn start_drag_out(&mut self, surface: &WlSurface, data: DragData) {
        let (Some(manager), Some(device), Some(serial)) = (
            &self.data_device_manager,
            &self.data_device,
            self.last_press_serial,
        ) else {
            return;
        };

        let types: Vec<&str> = match &data {
            DragData::Files(_) => vec![URI_LIST, TEXT_TYPES[0], TEXT_TYPES[2]],
            DragData::Text(_) => TEXT_TYPES.to_vec(),
        };

        let source: DragSource =
            manager.create_drag_and_drop_source(&self.qh, types, DndAction::Copy);

        source.start_drag(device, surface, None, serial);

        self.dragging_out = Some((source, data));
    }

    // asks the other program to write what it offers, none when it wouldn't
    fn receive(&self, offer: &DragOffer, kind: String) -> Option<File> {
        let pipe = offer.receive(kind).ok()?;

        // the receive request has to reach the other program before it can write anything
        let _ = self.connection.flush();

        Some(File::from(OwnedFd::from(pipe)))
    }

    // what the reading threads handed back, run from the event loop's wake
    pub(crate) fn handle_arrived_drags(&mut self) {
        let arrived = mem::take(&mut *ARRIVED.lock().unwrap_or_else(PoisonError::into_inner));

        for arrived in arrived {
            match arrived {
                Arrived::Held(epoch, files) => {
                    let Some((reading, _)) = &self.drags.reading else {
                        continue;
                    };

                    // the drag left, or another came in, while its list was read
                    if *reading != epoch {
                        continue;
                    }

                    let Some((_, surface)) = self.drags.reading.take() else {
                        continue;
                    };

                    self.files_held = Some((surface, files));

                    let (x, y) = self.drags.at;

                    self.files_moved(x, y);
                }

                Arrived::Dropped(epoch, data) => {
                    let Some(index) = self.drags.dropped.iter().position(|(e, _)| *e == epoch)
                    else {
                        continue;
                    };

                    let (_, offer) = self.drags.dropped.remove(index);

                    self.let_go(&offer, data);
                }
            }
        }
    }

    // finishes a let-go offer and hands what it carried to the widget under it
    fn let_go(&mut self, offer: &DragOffer, dropped: Option<DragData>) {
        offer.finish();
        offer.destroy();

        let surface = offer.surface.clone();

        let Some(window) = self.window(&surface) else {
            return;
        };

        let x = offer.x as f32;
        let y = offer.y as f32;

        let handled = match dropped {
            Some(DragData::Files(files)) if !files.is_empty() => {
                window.pointer.drop_files(x, y, files)
            }
            Some(DragData::Text(text)) => window.pointer.drop_text(x, y, text),
            _ => false,
        };

        if handled {
            self.request_frame_on(&surface);
        }
    }

    // the held files moved to (x, y): the widget under them says whether they can be let go there
    fn files_moved(&mut self, x: f64, y: f64) {
        let Some((surface, files)) = self.files_held.clone() else {
            return;
        };

        let Some(offer) = self.drag_offer() else {
            return;
        };

        let Some(window) = self.window(&surface) else {
            return;
        };

        // a window that doesn't ask takes files anywhere, as before widgets could say
        let answer = window.pointer.files_over(x as f32, y as f32, &files);

        if answer == Some(false) {
            offer.accept_mime_type(offer.serial, None);
            offer.set_actions(DndAction::empty(), DndAction::empty());
        } else {
            offer.accept_mime_type(offer.serial, Some(String::from(URI_LIST)));
            offer.set_actions(DndAction::Copy, DndAction::Copy);
        }

        if answer.is_some() {
            self.request_frame_on(&surface);
        }
    }
}

// reads a pipe to its end on its own thread and wakes the event loop with what came
fn read_later(mut file: File, then: impl FnOnce(Option<Vec<u8>>) -> Arrived + Send + 'static) {
    thread::spawn(move || {
        let mut bytes = Vec::new();

        let read = file.read_to_end(&mut bytes).ok().map(|_| bytes);

        ARRIVED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(then(read));

        changes::wake();
    });
}

// whether a drag offers files, and the best kind of text it offers
fn offered(offer: &DragOffer) -> (bool, Option<String>) {
    offer.with_mime_types(|types| (types.iter().any(|kind| kind == URI_LIST), text_type(types)))
}

// files as other programs take them, one file:// link per line
fn uri_list(files: &[PathBuf]) -> String {
    let mut list = String::new();

    for file in files {
        list.push_str("file://");
        list.push_str(&encode(&file.to_string_lossy()));
        list.push_str("\r\n");
    }

    list
}

// files as text, for a program that only takes text
fn paths(files: &[PathBuf]) -> String {
    let lines: Vec<String> = files
        .iter()
        .map(|file| file.to_string_lossy().into_owned())
        .collect();

    lines.join("\n")
}

// the letters a link can't hold as they are become %20 and the like
fn encode(path: &str) -> String {
    let mut encoded = String::new();

    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }

    encoded
}

// one link per line, lines starting with # are comments
pub(crate) fn parse(list: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();

    for line in list.lines() {
        let line = line.trim();

        let Some(path) = line.strip_prefix("file://") else {
            continue;
        };

        // a host may come before the path, like file://laptop/home
        let Some(start) = path.find('/') else {
            continue;
        };

        files.push(PathBuf::from(decode(&path[start..])));
    }

    files
}

// spaces and other special letters arrive as %20 and the like
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();

    let mut decoded = Vec::new();

    let mut index = 0;

    while index < bytes.len() {
        let escaped = bytes[index] == b'%' && index + 2 < bytes.len();

        let value = if escaped {
            std::str::from_utf8(&bytes[index + 1..index + 3])
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        } else {
            None
        };

        let Some(value) = value else {
            decoded.push(bytes[index]);
            index += 1;

            continue;
        };

        decoded.push(value);
        index += 3;
    }

    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_uri_list() {
        let list = "# from a file manager\r\nfile:///home/me/My%20Clip.mov\r\nfile://host/tmp/a%2\r\nhttps://x\r\n";

        let files = parse(list);

        assert_eq!(
            files,
            [
                PathBuf::from("/home/me/My Clip.mov"),
                PathBuf::from("/tmp/a%2")
            ]
        );
    }

    #[test]
    fn dragged_files_come_back_the_same() {
        let files = vec![
            PathBuf::from("/home/me/My Clip.mov"),
            PathBuf::from("/tmp/100%/ü.txt"),
        ];

        assert_eq!(parse(&uri_list(&files)), files);
    }
}
