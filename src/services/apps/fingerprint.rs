#![warn(clippy::unwrap_used, clippy::dbg_macro, clippy::todo, missing_docs)]

//! What a set of files and folders looks like from outside: where each one really is (on
//! NixOS a system switch moves `/run/current-system` to a new store path, whose files all carry
//! the same time), its modification time and size. Comparing two is a few `stat`s, so a
//! service can check often and read the files themselves only when something moved.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A snapshot of some paths; equal snapshots mean nothing visible changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Fingerprint(Vec<Option<(PathBuf, SystemTime, u64)>>);

impl Fingerprint {
    pub(crate) fn of<P: AsRef<Path>>(paths: impl IntoIterator<Item = P>) -> Self {
        Self(
            paths
                .into_iter()
                .map(|path| {
                    let path = path.as_ref();
                    let real = fs::canonicalize(path).ok()?;
                    let meta = fs::metadata(&real).ok()?;
                    Some((real, meta.modified().ok()?, meta.len()))
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_when_a_file_or_where_a_link_points_does() {
        let root = std::env::temp_dir().join(format!("amane-fingerprint-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("a")).expect("folder");
        fs::create_dir_all(root.join("b")).expect("folder");
        std::os::unix::fs::symlink(root.join("a"), root.join("current")).expect("link");

        let paths = [root.join("current"), root.join("missing")];
        let before = Fingerprint::of(&paths);
        assert_eq!(before, Fingerprint::of(&paths));

        fs::remove_file(root.join("current")).expect("unlink");
        std::os::unix::fs::symlink(root.join("b"), root.join("current")).expect("link");
        assert_ne!(before, Fingerprint::of(&paths), "the link moved");

        let _ = fs::remove_dir_all(&root);
    }
}
