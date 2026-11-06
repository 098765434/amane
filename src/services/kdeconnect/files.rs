/*
 * what the messages leave on disk: a message's thumbnails and a contact's
 * photo come inside the data as base64 and are written out once, under
 * $XDG_CACHE_HOME/amane/messages, so they draw like any other image
 */

use std::fs;
use std::path::PathBuf;

pub(crate) fn cache(folder: &str) -> Option<PathBuf> {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?;

    Some(cache.join("amane").join("messages").join(folder))
}

/*
 * `data` decoded into `folder/name`, unless it's already there; the path,
 * or none when it isn't base64 or can't be written
 */
pub(crate) fn write_once(folder: &str, name: &str, data: &str) -> Option<PathBuf> {
    let folder = cache(folder)?;

    let path = folder.join(name);

    if path.exists() {
        return Some(path);
    }

    let bytes = decode(data)?;

    if bytes.is_empty() {
        return None;
    }

    fs::create_dir_all(&folder).ok()?;

    // written beside and moved, so a half-written file is never drawn
    let partial = folder.join(format!(".{name}.part"));

    fs::write(&partial, bytes).ok()?;
    fs::rename(&partial, &path).ok()?;

    Some(path)
}

// a file name made of `text`, the same each time
pub(crate) fn name_for(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });

    format!("{hash:016x}")
}

// standard base64, padded or not, whitespace skipped; none on any other byte
pub(crate) fn decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);

    let mut buffer = 0u32;
    let mut bits = 0;

    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        };

        buffer = (buffer << 6) | u32::from(value);
        bits += 6;

        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }

    Some(out)
}

// a shell word for `text`, quoted, for a command line
pub(crate) fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_base64() {
        assert_eq!(decode("aGVp").unwrap(), b"hei");
        assert_eq!(decode("aGVpIQ==").unwrap(), b"hei!");
        assert_eq!(decode("aGVp\n IQ").unwrap(), b"hei!");
        assert_eq!(decode(""), Some(Vec::new()));
        assert_eq!(decode("a*b"), None);
    }

    #[test]
    fn names_are_stable() {
        assert_eq!(name_for("a"), name_for("a"));
        assert_ne!(name_for("a"), name_for("b"));
    }
}
