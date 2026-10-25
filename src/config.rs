use std::env;
use std::fs;
use std::io;
use std::ops::Deref;
use std::path::PathBuf;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{Service, watch_file};

/*
 * settings an app keeps in a file of its own, ~/.config/<app>/<file>.json; derive
 * Serialize, Deserialize, Default and PartialEq on a struct and name where it lives
 */
pub trait Settings:
    Serialize + DeserializeOwned + Default + PartialEq + Send + Sync + 'static
{
    // the folder in ~/.config, usually the app-id, like "amane-shell"
    const APP: &'static str;

    // the file's name without .json, like "appearance"
    const FILE: &'static str;
}

/*
 * a settings file as a Service: read it in a view like any other, change it with
 * Config::<T>::change, and every program that reads the same file sees the change, the
 * shell as well as a settings app
 */
pub struct Config<T: Settings> {
    value: T,
}

impl<T: Settings> Config<T> {
    // where the file lives
    pub fn path() -> PathBuf {
        let config = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .unwrap_or_else(env::temp_dir);

        config.join(T::APP).join(format!("{}.json", T::FILE))
    }

    /*
     * changes the settings and saves them, for input handlers; the file is written whole
     * and moved into place, so a program reading it never sees half of it
     */
    pub fn change(edit: impl FnOnce(&mut T)) {
        let mut config = Self::write();

        let before = serde_json::to_value(&config.value).ok();

        edit(&mut config.value);

        // nothing to write, and nobody to wake
        if serde_json::to_value(&config.value).ok() == before {
            config.quiet();

            return;
        }

        if let Err(error) = save(&Self::path(), &config.value) {
            eprintln!("amane: failed to save {}: {error}", Self::path().display());
        }
    }
}

impl<T: Settings> Deref for Config<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T: Settings> Service for Config<T> {
    fn new() -> Self {
        Self {
            value: load(&Self::path()),
        }
    }

    // a change from another program, or from an editor, is read again
    fn listen() {
        let path = Self::path();

        // the folder is watched, so it has to be there
        if let Some(folder) = path.parent() {
            let _ = fs::create_dir_all(folder);
        }

        for _ in watch_file(&path.to_string_lossy()) {
            let loaded = load::<T>(&path);

            let mut config = Self::write();

            // our own save comes back as a change too, and changes nothing
            if config.value == loaded {
                config.quiet();

                continue;
            }

            config.value = loaded;
        }
    }
}

// a missing file is the defaults; a broken one too, said once so it can be fixed
fn load<T: Settings>(path: &PathBuf) -> T {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return T::default(),
        Err(error) => {
            eprintln!("amane: failed to read {}: {error}", path.display());

            return T::default();
        }
    };

    match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("amane: failed to read {}: {error}", path.display());

            T::default()
        }
    }
}

fn save<T: Settings>(path: &PathBuf, value: &T) -> io::Result<()> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }

    let text = serde_json::to_string_pretty(value).map_err(io::Error::other)?;

    // written next to it, then moved over it in one step
    let temporary = path.with_extension("json.new");

    fs::write(&temporary, text + "\n")?;
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Serialize, Deserialize, Default, PartialEq, Debug)]
    #[serde(default)]
    struct Appearance {
        accent: String,
        dark: bool,
    }

    impl Settings for Appearance {
        const APP: &'static str = "amane-test";
        const FILE: &'static str = "appearance";
    }

    #[test]
    fn saves_and_loads_settings_and_fills_in_what_is_missing() {
        let folder = env::temp_dir().join(format!("amane-config-{}", std::process::id()));
        let path = folder.join("appearance.json");

        let appearance = Appearance {
            accent: String::from("blue"),
            dark: true,
        };

        save(&path, &appearance).unwrap();

        assert_eq!(load::<Appearance>(&path), appearance);

        // an older file without a field takes that field's default
        fs::write(&path, r#"{ "accent": "red" }"#).unwrap();

        assert_eq!(
            load::<Appearance>(&path),
            Appearance {
                accent: String::from("red"),
                dark: false,
            }
        );

        // a broken file is the defaults rather than a crash
        fs::write(&path, "{").unwrap();

        assert_eq!(load::<Appearance>(&path), Appearance::default());

        fs::remove_dir_all(folder).unwrap();
    }
}
