use crate::changes::{self, Source};

// A single open window (toplevel) as reported by the compositor. Named
// `WindowInfo` because the crate root already exports `Window` for the
// xdg-toplevel *surface role* this shell can open itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub(crate) id: u64,

    // null while the app has set neither
    pub(crate) app_id: Option<String>,
    pub(crate) title: Option<String>,

    // exactly one window is focused, or none (e.g. a layer surface has focus)
    pub(crate) focused: bool,

    // false means it is in the tiling layout
    pub(crate) floating: bool,

    // the workspace it is on, if any
    pub(crate) workspace: Option<i64>,

    // its column and its place in the column, from 1, in a scrolling layout
    pub(crate) column: Option<(usize, usize)>,
}

impl WindowInfo {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn app_id(&self) -> Option<&str> {
        self.app_id.as_deref()
    }

    // a later change to any title draws again only the windows that asked for one
    pub fn title(&self) -> Option<&str> {
        changes::note_source(Source::Titles);

        self.title.as_deref()
    }

    // whether the two are the same window in the same place, whatever their titles
    pub(crate) fn same_but_title(&self, other: &Self) -> bool {
        self.id == other.id
            && self.app_id == other.app_id
            && self.focused == other.focused
            && self.floating == other.floating
            && self.workspace == other.workspace
            && self.column == other.column
    }

    pub fn focused(&self) -> bool {
        self.focused
    }

    pub fn floating(&self) -> bool {
        self.floating
    }

    pub fn workspace(&self) -> Option<i64> {
        self.workspace
    }

    // where it is in niri's scrolling layout: its column, then its place in
    // the column, both from 1; none while floating or under another compositor
    pub fn column(&self) -> Option<(usize, usize)> {
        self.column
    }
}
