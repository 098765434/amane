use std::hash::{Hash, Hasher};

use crate::Weight;

// how letters are cased before they are laid out, like css text-transform
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Case {
    #[default]
    AsWritten,
    Upper,
    Lower,

    // the first letter of every word made a capital, the rest left alone
    Capitalize,
}

impl Case {
    pub(super) fn apply(self, content: &str) -> String {
        match self {
            Case::AsWritten => String::from(content),
            Case::Upper => content.to_uppercase(),
            Case::Lower => content.to_lowercase(),
            Case::Capitalize => capitalize(content),
        }
    }
}

fn capitalize(content: &str) -> String {
    let mut out = String::with_capacity(content.len());

    let mut word_start = true;

    for letter in content.chars() {
        if word_start && letter.is_alphabetic() {
            out.extend(letter.to_uppercase());
        } else {
            out.push(letter);
        }

        word_start = letter.is_whitespace();
    }

    out
}

// a line drawn with the letters, in their color
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Decoration {
    #[default]
    None,
    Underline,
    StrikeThrough,
}

// where lines sit across the width text is given; start and end follow the text's direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,

    // every line but a paragraph's last stretched to the full width
    Justify,
}

// which way a paragraph reads; auto takes it from the first strong letter
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextDirection {
    #[default]
    Auto,
    LeftToRight,
    RightToLeft,
}

// everything that changes how text lays out, kept together so a layout can be looked up by it
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Style {
    pub size: f32,
    pub font: Option<String>,
    pub weight: Weight,
    pub italic: bool,

    // a multiple of the size; none spaces lines by the font's own metrics
    pub line_height: Option<f32>,

    // extra room after every letter, in pixels
    pub letter_spacing: f32,

    pub case: Case,
    pub decoration: Decoration,
    pub align: TextAlign,
    pub direction: TextDirection,

    // a variable font's sliders by their four-letter tag, like wdth or opsz
    pub variations: Vec<([u8; 4], f32)>,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            size: 16.0,
            font: None,
            weight: Weight::Regular,
            italic: false,
            line_height: None,
            letter_spacing: 0.0,
            case: Case::AsWritten,
            decoration: Decoration::None,
            align: TextAlign::Start,
            direction: TextDirection::Auto,
            variations: Vec::new(),
        }
    }
}

// the numbers are compared by their bits, which is all a lookup needs
impl Eq for Style {}

impl Hash for Style {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.size.to_bits().hash(state);
        self.font.hash(state);
        self.weight.value().hash(state);
        self.italic.hash(state);
        self.line_height.map(f32::to_bits).hash(state);
        self.letter_spacing.to_bits().hash(state);
        self.case.hash(state);
        self.decoration.hash(state);
        self.align.hash(state);
        self.direction.hash(state);

        for (tag, value) in &self.variations {
            tag.hash(state);
            value.to_bits().hash(state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases_change_the_letters() {
        assert_eq!(Case::Upper.apply("Hello, world"), "HELLO, WORLD");
        assert_eq!(Case::Lower.apply("Hello, World"), "hello, world");
        assert_eq!(
            Case::Capitalize.apply("hello wide\tworld"),
            "Hello Wide\tWorld"
        );
        assert_eq!(Case::Capitalize.apply("iOS stays"), "IOS Stays");
        assert_eq!(Case::AsWritten.apply("as is"), "as is");
    }
}
