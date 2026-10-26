use crate::Color;
use crate::graphics::Area;

pub struct Shadow {
    pub(crate) color: Color,
    pub(crate) opacity: f32,
    pub(crate) blur: f32,

    pub(crate) x: f32,
    pub(crate) y: f32,

    // how much bigger a drop shadow is on every side, or how much further in an inner one reaches
    pub(crate) spread: f32,

    pub(crate) kind: Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    // cast behind the rectangle, onto whatever is under it
    #[default]
    Drop,

    // along the inside edge, as if the rectangle were pressed in
    Inner,
}

impl Shadow {
    pub fn drop(color: impl Into<Color>) -> Self {
        Self::new(color.into(), Kind::Drop)
    }

    pub fn inner(color: impl Into<Color>) -> Self {
        Self::new(color.into(), Kind::Inner)
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;

        self
    }

    pub fn blur(mut self, amount: f32) -> Self {
        self.blur = amount;

        self
    }

    // as CSS's spread: a drop shadow grows by it on every side and its corners round with it
    pub fn spread(mut self, spread: f32) -> Self {
        self.spread = spread;

        self
    }

    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.x = x;
        self.y = y;

        self
    }

    fn new(color: Color, kind: Kind) -> Self {
        Self {
            color,
            opacity: 1.0,
            blur: 0.0,
            x: 0.0,
            y: 0.0,
            spread: 0.0,
            kind,
        }
    }

    // opacity thins out whatever alpha the color already has
    pub(crate) fn tint(&self) -> Color {
        let opacity = f32::clamp(self.opacity, 0.0, 1.0);

        let alpha = f32::from(self.color.a) * opacity;

        Color::rgba(
            self.color.r,
            self.color.g,
            self.color.b,
            alpha.round() as u8,
        )
    }

    // a drop shadow's area grows by the spread, an inner shadow's hole shrinks by it
    pub(crate) fn place(&self, area: Area) -> Area {
        let grow = match self.kind {
            Kind::Drop => self.spread,
            Kind::Inner => -self.spread,
        };

        Area::new(
            area.x + self.x - grow,
            area.y + self.y - grow,
            (area.width + grow * 2.0).max(0.0),
            (area.height + grow * 2.0).max(0.0),
        )
    }

    // a drop shadow's corners round out as far as it spreads
    pub(crate) fn corners(&self, radius: f32) -> f32 {
        match self.kind {
            Kind::Drop => (radius + self.spread).max(0.0),
            Kind::Inner => radius,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drop_shadow_spreads_out_and_an_inner_one_in() {
        let area = Area::new(10.0, 10.0, 100.0, 50.0);

        let drop = Shadow::drop(Color::BLACK).offset(2.0, 4.0).spread(5.0);
        assert_eq!(drop.place(area), Area::new(7.0, 9.0, 110.0, 60.0));
        assert_eq!(drop.corners(8.0), 13.0);

        let inner = Shadow::inner(Color::BLACK).spread(5.0);
        assert_eq!(inner.place(area), Area::new(15.0, 15.0, 90.0, 40.0));
        assert_eq!(inner.corners(8.0), 8.0);
    }
}
