use crate::{Argument, Bus, Color, Service, Value};

const PORTAL: &str = "org.freedesktop.portal.Desktop";

const PATH: &str = "/org/freedesktop/portal/desktop";

const SETTINGS: &str = "org.freedesktop.portal.Settings";

const NAMESPACE: &str = "org.freedesktop.appearance";

const COLOR_SCHEME: &str = "color-scheme";

const ACCENT_COLOR: &str = "accent-color";

// the light or dark preference the desktop asks programs to follow
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scheme {
    #[default]
    NoPreference,
    Dark,
    Light,
}

#[derive(Default, PartialEq)]
pub struct Appearance {
    // none without xdg-desktop-portal, then everything reads as no preference
    present: bool,

    scheme: Scheme,

    // only some portal backends know one
    accent: Option<Color>,
}

// the portal announces every change with SettingChanged, so nothing is polled
impl Service for Appearance {
    fn new() -> Self {
        fetch()
    }

    fn listen() {
        if !Self::read().present {
            return;
        }

        let changes = Bus::session().signals(SETTINGS, "SettingChanged");

        // a change between new() and watching would otherwise wait for the next one
        refresh(fetch());

        for signal in changes {
            let [namespace, key, value] = signal.arguments() else {
                continue;
            };

            if namespace.text() != NAMESPACE {
                continue;
            }

            let mut fresh = Self {
                present: true,
                ..*Self::read()
            };

            match key.text() {
                COLOR_SCHEME => fresh.scheme = Scheme::from_portal(value),
                ACCENT_COLOR => fresh.accent = accent_from(value),

                _ => continue,
            }

            refresh(fresh);
        }
    }
}

impl Appearance {
    // false without xdg-desktop-portal
    pub fn present(&self) -> bool {
        self.present
    }

    pub fn scheme(&self) -> Scheme {
        self.scheme
    }

    pub fn prefers_dark(&self) -> bool {
        self.scheme == Scheme::Dark
    }

    pub fn accent(&self) -> Option<Color> {
        self.accent
    }
}

impl Scheme {
    fn from_portal(value: &Value) -> Self {
        match value {
            Value::Number(1.0) => Self::Dark,
            Value::Number(2.0) => Self::Light,

            _ => Self::NoPreference,
        }
    }
}

// three sRGB parts from 0 to 1, and anything outside that range means unset
fn accent_from(value: &Value) -> Option<Color> {
    let [red, green, blue] = value.list() else {
        return None;
    };

    let mut parts = [0; 3];

    for (part, value) in parts.iter_mut().zip([red, green, blue]) {
        let Value::Number(number) = value else {
            return None;
        };

        if !(0.0..=1.0).contains(number) {
            return None;
        }

        *part = (number * 255.0).round() as u8;
    }

    let [red, green, blue] = parts;

    Some(Color::rgb(red, green, blue))
}

fn read(key: &str) -> Value {
    let arguments = [Argument::from(NAMESPACE), Argument::from(key)];

    Bus::session().call(PORTAL, PATH, SETTINGS, "Read", &arguments)
}

// the portal is asked outside the lock
fn fetch() -> Appearance {
    let scheme = read(COLOR_SCHEME);

    // a missing portal, or one without the appearance namespace, answers nothing
    if scheme == Value::Nothing {
        return Appearance::default();
    }

    Appearance {
        present: true,
        scheme: Scheme::from_portal(&scheme),
        accent: accent_from(&read(ACCENT_COLOR)),
    }
}

// nothing is written when nothing changed
fn refresh(fresh: Appearance) {
    if *Appearance::read() == fresh {
        return;
    }

    *Appearance::write() = fresh;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(parts: [f64; 3]) -> Value {
        Value::List(parts.into_iter().map(Value::Number).collect())
    }

    #[test]
    fn schemes_from_the_portal() {
        assert_eq!(
            Scheme::from_portal(&Value::Number(0.0)),
            Scheme::NoPreference
        );
        assert_eq!(Scheme::from_portal(&Value::Number(1.0)), Scheme::Dark);
        assert_eq!(Scheme::from_portal(&Value::Number(2.0)), Scheme::Light);
        assert_eq!(
            Scheme::from_portal(&Value::Number(7.0)),
            Scheme::NoPreference
        );
        assert_eq!(Scheme::from_portal(&Value::Nothing), Scheme::NoPreference);
    }

    #[test]
    fn accent_in_range() {
        let accent = accent_from(&rgb([0.0, 0.5, 1.0]));

        assert_eq!(accent, Some(Color::rgb(0, 128, 255)));
    }

    #[test]
    fn accent_out_of_range_is_unset() {
        assert_eq!(accent_from(&rgb([-1.0, -1.0, -1.0])), None);
        assert_eq!(accent_from(&rgb([0.2, 1.5, 0.2])), None);
    }

    #[test]
    fn accent_of_the_wrong_shape_is_unset() {
        assert_eq!(accent_from(&Value::Nothing), None);

        let two = Value::List(vec![Value::Number(0.1), Value::Number(0.2)]);

        assert_eq!(accent_from(&two), None);
    }
}
