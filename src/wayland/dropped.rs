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

use crate::clipboard::DragData;

use super::{WaylandState, clipboard::write_bytes};

// file managers offer dragged files as a list of file:// links
const URI_LIST: &str = "text/uri-list";

// the fork's drags: read off the event loop, and dragging out
mod drag;

pub(crate) use drag::Drags;
use drag::*;

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
            // a password's hint says only that it is one
            if kind == super::clipboard::PASSWORD_HINT {
                write_bytes(pipe, b"secret".to_vec());
            } else {
                write_bytes(pipe, text.clone().into_bytes());
            }

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
            self.copied.drop_clipboard();
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
