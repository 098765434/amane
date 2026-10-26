#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Weight {
    Thin,
    ExtraLight,
    Light,
    #[default]
    Regular,
    Medium,
    SemiBold,
    Bold,
    ExtraBold,
    Black,

    // any weight from 1 to 1000; a variable font draws it exactly, others pick the closest file
    Number(u16),
}

impl Weight {
    // how heavy it is, on the 100 to 900 scale fonts use
    pub fn value(self) -> u16 {
        match self {
            Weight::Thin => 100,
            Weight::ExtraLight => 200,
            Weight::Light => 300,
            Weight::Regular => 400,
            Weight::Medium => 500,
            Weight::SemiBold => 600,
            Weight::Bold => 700,
            Weight::ExtraBold => 800,
            Weight::Black => 900,
            Weight::Number(number) => number.clamp(1, 1000),
        }
    }
}

impl From<u16> for Weight {
    fn from(number: u16) -> Self {
        Weight::Number(number)
    }
}
