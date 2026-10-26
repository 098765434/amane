use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::thread;

use inotify::{Inotify, WatchDescriptor, WatchMask, Watches};

use crate::changes;

/*
 * shader files are watched once read, and saving one counts up the
 * generation, so every window compiles its shaders again and redraws
 */
static GENERATION: AtomicU64 = AtomicU64::new(0);

static WATCHER: Mutex<Option<Watcher>> = Mutex::new(None);

// the last error each shader file gave, gone once it compiles
static ERRORS: Mutex<BTreeMap<PathBuf, String>> = Mutex::new(BTreeMap::new());

struct Watcher {
    watches: Watches,

    // the shader file names read from each watched folder
    folders: HashMap<WatchDescriptor, HashSet<OsString>>,
}

pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

/*
 * the shader files that failed to compile or load, with why; a window
 * keeps drawing a file's last good version, or nothing, until it's fixed
 */
pub fn shader_errors() -> Vec<(PathBuf, String)> {
    let errors = ERRORS.lock().unwrap_or_else(PoisonError::into_inner);

    errors
        .iter()
        .map(|(path, error)| (path.clone(), error.clone()))
        .collect()
}

// redraws only when the error changed, so a failing shader doesn't keep the windows drawing
pub fn report(path: &Path, error: Option<String>) {
    let mut errors = ERRORS.lock().unwrap_or_else(PoisonError::into_inner);

    let changed = match error {
        Some(error) => errors.insert(path.to_path_buf(), error.clone()) != Some(error),
        None => errors.remove(path).is_some(),
    };

    drop(errors);

    if changed {
        changes::mark_all();
    }
}

/*
 * editors save by renaming a new file over the old one, so the folder is
 * watched, as in files.rs; a link is followed, so saving the file it points
 * to counts
 */
pub fn watch(path: &Path) {
    let real = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let path = real.as_path();

    let Some(name) = path.file_name() else {
        return;
    };

    let folder = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };

    let mut watcher = WATCHER.lock().unwrap_or_else(PoisonError::into_inner);

    if watcher.is_none() {
        *watcher = start();
    }

    let Some(watcher) = watcher.as_mut() else {
        return;
    };

    let mask = WatchMask::MODIFY | WatchMask::MOVED_TO | WatchMask::CREATE;

    // adding a folder twice hands back the same descriptor
    let Ok(descriptor) = watcher.watches.add(folder, mask) else {
        return;
    };

    watcher
        .folders
        .entry(descriptor)
        .or_default()
        .insert(name.to_os_string());
}

// none when inotify can't start, then shaders just don't reload
fn start() -> Option<Watcher> {
    let mut inotify = Inotify::init().ok()?;

    let watches = inotify.watches();

    thread::Builder::new()
        .name("amane-shaders".into())
        .spawn(move || {
            let mut buffer = [0; 4096];

            while let Ok(events) = inotify.read_events_blocking(&mut buffer) {
                let mut changed = false;

                let guard = WATCHER.lock().unwrap_or_else(PoisonError::into_inner);

                let Some(watcher) = guard.as_ref() else {
                    continue;
                };

                for event in events {
                    let Some(names) = watcher.folders.get(&event.wd) else {
                        continue;
                    };

                    if event.name.is_some_and(|name| names.contains(name)) {
                        changed = true;
                    }
                }

                drop(guard);

                if changed {
                    GENERATION.fetch_add(1, Ordering::Relaxed);

                    changes::mark_all();
                }
            }
        })
        .ok()?;

    Some(Watcher {
        watches,
        folders: HashMap::new(),
    })
}
