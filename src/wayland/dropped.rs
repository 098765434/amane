use std::io::Read;
use std::path::PathBuf;

use smithay_client_toolkit::data_device_manager::{
    WritePipe,
    data_device::DataDeviceHandler,
    data_offer::{DataOfferHandler, DragOffer},
    data_source::DataSourceHandler,
};
use wayland_client::{
    Connection, QueueHandle,
    protocol::{
        wl_data_device::WlDataDevice, wl_data_device_manager::DndAction,
        wl_data_source::WlDataSource, wl_surface::WlSurface,
    },
};

use super::WaylandState;

// file managers offer dragged files as a list of file:// links
const URI_LIST: &str = "text/uri-list";

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

        let has_files = offer.with_mime_types(|types| types.iter().any(|kind| kind == URI_LIST));

        // anything but files is turned down, so the pointer shows it can't be dropped here
        if !has_files {
            offer.accept_mime_type(offer.serial, None);

            return;
        }

        // a widget may want to know which files before they are let go, as a dock does
        let files = self.read_files(&offer).unwrap_or_default();

        self.files_held = Some((surface.clone(), files));

        self.files_moved(x, y);
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
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
        self.files_moved(x, y);
    }

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
        self.files_held = None;

        let Some(offer) = self.drag_offer() else {
            return;
        };

        let files = self.read_files(&offer);

        offer.finish();
        offer.destroy();

        let Some(files) = files.filter(|files| !files.is_empty()) else {
            return;
        };

        let surface = offer.surface.clone();

        let Some(window) = self.window(&surface) else {
            return;
        };

        let x = offer.x as f32;
        let y = offer.y as f32;

        if window.pointer.drop_files(x, y, files) {
            self.request_frame_on(&surface);
        }
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

// amane never offers data of its own
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
        _: &WlDataSource,
        _: String,
        _: WritePipe,
    ) {
    }

    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}

impl WaylandState {
    fn drag_offer(&self) -> Option<DragOffer> {
        self.data_device.as_ref()?.data().drag_offer()
    }

    // the files offered, none when the other program wouldn't say
    fn read_files(&self, offer: &DragOffer) -> Option<Vec<PathBuf>> {
        let mut pipe = offer.receive(String::from(URI_LIST)).ok()?;

        // the receive request has to reach the other program before it can write anything
        let _ = self.connection.flush();

        /*
         * ponytail: blocks the event loop until the other program has written
         * the list, which is instant for file managers; read it on the loop if
         * a slow source ever stalls a frame
         */
        let mut list = String::new();

        pipe.read_to_string(&mut list).ok()?;

        Some(parse(&list))
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

// one link per line, lines starting with # are comments
fn parse(list: &str) -> Vec<PathBuf> {
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
}
