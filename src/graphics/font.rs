use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, PoisonError};

use parley::fontique::Synthesis;
use parley::{FontContext, FontData, LayoutContext};
use swash::FontRef;
use swash::scale::ScaleContext;

static DEFAULT_FAMILY: LazyLock<Mutex<String>> =
    LazyLock::new(|| Mutex::new(String::from("sans-serif")));

// counts changes to the default family, so text laid out in the old one is laid out again
static GENERATION: AtomicU64 = AtomicU64::new(0);

// each font file parley found, opened for swash once, by the file's id and its index in it
static FACES: LazyLock<Mutex<HashMap<(u64, u32), FontRef<'static>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// every variable font slider setting seen, kept once so a face can point at it
static COORDS: LazyLock<Mutex<HashSet<&'static [i16]>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

thread_local! {
    // swash keeps its working memory here, so each letter doesn't allocate it again
    static CONTEXT: RefCell<ScaleContext> = RefCell::new(ScaleContext::new());

    // the installed fonts, which take a while to find, and parley's working memory for laying out text
    static FONTS: RefCell<FontContext> = RefCell::new(FontContext::new());
    static LAYOUTS: RefCell<LayoutContext<()>> = RefCell::new(LayoutContext::new());
}

pub fn set_default(family: &str) {
    let mut default_family = DEFAULT_FAMILY
        .lock()
        .unwrap_or_else(PoisonError::into_inner);

    *default_family = String::from(family);

    GENERATION.fetch_add(1, Ordering::Relaxed);
}

pub fn default_family() -> String {
    DEFAULT_FAMILY
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

// parley's fonts and working memory, for laying out text
pub fn with_layout<T>(work: impl FnOnce(&mut FontContext, &mut LayoutContext<()>) -> T) -> T {
    FONTS.with_borrow_mut(|fonts| LAYOUTS.with_borrow_mut(|layouts| work(fonts, layouts)))
}

// swash's working memory for turning letters into outlines and pixels
pub fn with_context<T>(work: impl FnOnce(&mut ScaleContext) -> T) -> T {
    CONTEXT.with_borrow_mut(work)
}

// a font as one run of text draws it: the file, its sliders, and what was faked for a style it lacks
#[derive(Clone, Copy)]
pub struct Face {
    pub font: FontRef<'static>,

    // where a variable font's sliders sit, like its weight; empty for the font's default
    pub coords: &'static [i16],

    // a slant in degrees for an italic the family doesn't have
    pub skew: Option<f32>,

    // a thicker outline for a bold the family doesn't have
    pub embolden: bool,
}

impl Face {
    // tells faces apart for the caches of letters, which look different in each
    pub fn key(&self) -> u64 {
        let mut hasher = std::hash::DefaultHasher::new();

        self.font.key.value().hash(&mut hasher);

        // the sliders are kept once each, so where they are says which they are
        self.coords.as_ptr().hash(&mut hasher);

        self.skew.map(f32::to_bits).hash(&mut hasher);
        self.embolden.hash(&mut hasher);

        hasher.finish()
    }
}

// the same font file at the same sliders and faked style, for telling frames apart
impl PartialEq for Face {
    fn eq(&self, other: &Self) -> bool {
        self.font.key == other.font.key
            && self.font.offset == other.font.offset
            && std::ptr::eq(self.coords, other.coords)
            && self.skew.map(f32::to_bits) == other.skew.map(f32::to_bits)
            && self.embolden == other.embolden
    }
}

// the face a run laid out by parley draws with
pub fn face(data: &FontData, coords: &[i16], synthesis: Synthesis) -> Face {
    Face {
        font: open(data),
        coords: keep_coords(coords),
        skew: synthesis.skew(),
        embolden: synthesis.embolden(),
    }
}

fn open(data: &FontData) -> FontRef<'static> {
    let key = (data.data.id(), data.index);

    let mut faces = FACES.lock().unwrap_or_else(PoisonError::into_inner);

    if let Some(font) = faces.get(&key) {
        return *font;
    }

    /*
     * fonts stay open until the program exits, like the letters drawn from
     * them, so the file's bytes are kept for good
     */
    let bytes: &'static [u8] = Box::leak(Box::new(data.data.clone())).data();

    let font = FontRef::from_index(bytes, data.index as usize).expect("failed to parse font");

    faces.insert(key, font);

    font
}

fn keep_coords(coords: &[i16]) -> &'static [i16] {
    if coords.is_empty() {
        return &[];
    }

    let mut kept = COORDS.lock().unwrap_or_else(PoisonError::into_inner);

    if let Some(found) = kept.get(coords) {
        return found;
    }

    let leaked: &'static [i16] = Vec::leak(coords.to_vec());

    kept.insert(leaked);

    leaked
}
