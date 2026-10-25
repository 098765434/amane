use std::path::{Path, PathBuf};

/*
 * removes the field codes like %f and %U, which a launcher
 * fills with files to open, and a menu has none of
 */
pub fn strip_field_codes(exec: &str) -> String {
    let mut command = String::new();

    let mut characters = exec.chars();

    while let Some(character) = characters.next() {
        if character != '%' {
            command.push(character);

            continue;
        }

        // %% is a plain percent sign, anything else after % is dropped with it
        if let Some('%') = characters.next() {
            command.push('%');
        }
    }

    command.trim().to_string()
}

/*
 * the command lines that open `files` with a program run by `exec`: %f and
 * %u take one file each, so there's a command a file, %F and %U take them
 * all; none of them, and the program opens no files, so it's started plain
 */
pub fn fill_field_codes(exec: &str, files: &[PathBuf]) -> Vec<String> {
    let one = exec.contains("%f") || exec.contains("%u");

    if one && !files.is_empty() {
        return files
            .iter()
            .map(|file| fill(exec, std::slice::from_ref(file)))
            .collect();
    }

    vec![fill(exec, files)]
}

fn fill(exec: &str, files: &[PathBuf]) -> String {
    let paths = || files.iter().map(|file| quote(&file.to_string_lossy()));
    let links = || files.iter().map(|file| quote(&link(file)));

    let mut command = String::new();

    let mut characters = exec.chars();

    while let Some(character) = characters.next() {
        if character != '%' {
            command.push(character);

            continue;
        }

        let filled: Vec<String> = match characters.next() {
            Some('%') => vec![String::from("%")],
            Some('f') => paths().take(1).collect(),
            Some('F') => paths().collect(),
            Some('u') => links().take(1).collect(),
            Some('U') => links().collect(),
            // the rest, like %i and %c, a launcher may leave out
            _ => Vec::new(),
        };

        command.push_str(&filled.join(" "));
    }

    command.trim().to_string()
}

// a word the shell takes as it is, whatever is in it
fn quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\''"))
}

// a file:// link to `file`, with the letters a link can't hold escaped
fn link(file: &Path) -> String {
    let mut link = String::from("file://");

    for byte in file.to_string_lossy().bytes() {
        if byte.is_ascii_alphanumeric() || b"/-_.~".contains(&byte) {
            link.push(byte as char);
        } else {
            link.push_str(&format!("%{byte:02X}"));
        }
    }

    link
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_files_in_for_their_field_codes() {
        let files = [
            PathBuf::from("/home/me/a b.md"),
            PathBuf::from("/tmp/it's.txt"),
        ];

        assert_eq!(
            fill_field_codes("code --new-window %F", &files),
            ["code --new-window '/home/me/a b.md' '/tmp/it'\\''s.txt'"]
        );
        assert_eq!(
            fill_field_codes("firefox %u", &files),
            [
                "firefox 'file:///home/me/a%20b.md'",
                "firefox 'file:///tmp/it%27s.txt'"
            ]
        );
        assert_eq!(fill_field_codes("foot", &files), ["foot"]);
        assert_eq!(fill_field_codes("gimp %U %i", &[]), ["gimp"]);
    }

    #[test]
    fn drops_field_codes_but_keeps_double_percent() {
        assert_eq!(strip_field_codes("firefox %u"), "firefox");
        assert_eq!(
            strip_field_codes("code --new-window %F"),
            "code --new-window"
        );
        assert_eq!(strip_field_codes("printf 100%%"), "printf 100%");
    }
}
