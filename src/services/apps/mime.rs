use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use super::data_dirs;

/*
 * what kind of file a path is, by its name, as the shared mime database
 * (mime/globs2 in the xdg data dirs) says; folders are inode/directory
 */
pub fn mime_type(path: &Path) -> Option<String> {
    if path.is_dir() {
        return Some(String::from("inode/directory"));
    }

    let name = path.file_name()?.to_str()?;

    database().kind_of(name)
}

// whether a program that opens `opens` (its MimeType list) opens a file of `kind`, or of a kind it comes from
pub fn opens(opens: &[String], kind: &str) -> bool {
    let database = database();

    let mut kinds = vec![kind.to_string()];

    let mut index = 0;

    // every kind it's a kind of, nearest first
    while let Some(kind) = kinds.get(index).cloned() {
        for parent in database.parents(&kind) {
            if !kinds.contains(&parent) {
                kinds.push(parent);
            }
        }

        index += 1;
    }

    kinds.iter().any(|kind| opens.contains(kind))
}

#[derive(Default)]
struct Database {
    // weight, kind, pattern, from globs2
    globs: Vec<(u32, String, String)>,

    // a kind and the kinds it comes from, from subclasses
    parents: HashMap<String, Vec<String>>,
}

fn database() -> &'static Database {
    static DATABASE: OnceLock<Database> = OnceLock::new();

    DATABASE.get_or_init(|| {
        let mut database = Database::default();

        for dir in data_dirs::find() {
            let mime = dir.join("mime");

            if let Ok(text) = fs::read_to_string(mime.join("globs2")) {
                database.read_globs(&text);
            }

            if let Ok(text) = fs::read_to_string(mime.join("subclasses")) {
                database.read_subclasses(&text);
            }
        }

        database
    })
}

impl Database {
    // lines of weight:kind:pattern, # for comments
    fn read_globs(&mut self, text: &str) {
        for line in text.lines() {
            if line.starts_with('#') {
                continue;
            }

            let mut fields = line.splitn(4, ':');

            let (Some(weight), Some(kind), Some(pattern)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };

            let Ok(weight) = weight.parse() else {
                continue;
            };

            self.globs
                .push((weight, kind.to_string(), pattern.to_string()));
        }
    }

    // lines of "kind parent"
    fn read_subclasses(&mut self, text: &str) {
        for line in text.lines() {
            let Some((kind, parent)) = line.split_once(' ') else {
                continue;
            };

            self.parents
                .entry(kind.to_string())
                .or_default()
                .push(parent.trim().to_string());
        }
    }

    // the heaviest pattern that fits, the longest of those
    fn kind_of(&self, name: &str) -> Option<String> {
        let lower = name.to_lowercase();

        self.globs
            .iter()
            .filter(|(_, _, pattern)| fits(pattern, name) || fits(pattern, &lower))
            .max_by_key(|(weight, _, pattern)| (*weight, pattern.len()))
            .map(|(_, kind, _)| kind.clone())
    }

    // every text kind is plain text too, as the spec says
    fn parents(&self, kind: &str) -> Vec<String> {
        let mut parents = self.parents.get(kind).cloned().unwrap_or_default();

        let plain = String::from("text/plain");

        if kind.starts_with("text/") && kind != plain && !parents.contains(&plain) {
            parents.push(plain);
        }

        parents
    }
}

// the patterns are a name, or * and ? standing for any run of letters and any one letter
fn fits(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();

    let (mut p, mut n) = (0, 0);

    // where the last * was, and the letter of the name it took up to
    let mut star: Option<(usize, usize)> = None;

    while n < name.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some((p, n));
                p += 1;
            }

            Some(&letter) if letter == '?' || letter == name[n] => {
                p += 1;
                n += 1;
            }

            _ => {
                let Some((at, took)) = star else {
                    return false;
                };

                // the * takes one more letter
                p = at + 1;
                n = took + 1;
                star = Some((at, took + 1));
            }
        }
    }

    pattern[p..].iter().all(|letter| *letter == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database() -> Database {
        let mut database = Database::default();

        database.read_globs(
            "# comment\n50:text/markdown:*.md\n50:text/x-csrc:*.c\n50:text/x-c++src:*.C\n10:text/x-readme:README*\n80:application/x-compressed-tar:*.tar.gz\n50:application/gzip:*.gz\n",
        );

        database.read_subclasses(
            "text/x-csrc text/plain\napplication/x-compressed-tar application/gzip\n",
        );

        database
    }

    #[test]
    fn a_file_is_the_kind_its_heaviest_longest_pattern_says() {
        let database = database();

        assert_eq!(
            database.kind_of("notes.md").as_deref(),
            Some("text/markdown")
        );
        assert_eq!(
            database.kind_of("NOTES.MD").as_deref(),
            Some("text/markdown")
        );
        assert_eq!(
            database.kind_of("a.tar.gz").as_deref(),
            Some("application/x-compressed-tar")
        );
        assert_eq!(database.kind_of("README").as_deref(), Some("text/x-readme"));
        assert_eq!(database.kind_of("photo"), None);

        // a case-sensitive pattern still wins for the name as written
        assert_eq!(database.kind_of("main.C").as_deref(), Some("text/x-c++src"));
    }

    #[test]
    fn a_kind_comes_from_its_parents_and_text_is_plain_text() {
        let database = database();

        assert_eq!(database.parents("text/x-csrc"), ["text/plain"]);
        assert_eq!(database.parents("text/markdown"), ["text/plain"]);
        assert!(database.parents("text/plain").is_empty());
        assert_eq!(
            database.parents("application/x-compressed-tar"),
            ["application/gzip"]
        );
    }

    #[test]
    fn patterns_take_any_run_or_one_letter() {
        assert!(fits("*.md", "a.md"));
        assert!(fits("*.md", ".md"));
        assert!(!fits("*.md", "a.mdx"));
        assert!(fits("README*", "README.txt"));
        assert!(fits("?.c", "a.c"));
        assert!(!fits("?.c", "ab.c"));
        assert!(fits("*a*b", "xxaxxb"));
    }
}
