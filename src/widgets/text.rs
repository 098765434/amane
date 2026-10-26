mod layout;
mod style;

use std::rc::Rc;

use crate::graphics::{Area, Corners, Renderer};
use crate::{Color, Size, Weight};

use super::Widget;

use layout::{Arranged, Rules};
use style::Style;

pub use style::{Case, Decoration, TextAlign, TextDirection};

pub struct Text {
    content: String,
    color: Color,
    style: Style,
    rules: Rules,

    // sized by the letters' own shapes instead of their advance, so an icon centers exactly
    tight: bool,
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            color: Color::BLACK,
            style: Style::default(),
            rules: Rules::default(),
            tight: false,
        }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.style.size = size;

        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;

        self
    }

    pub fn font(mut self, family: &str) -> Self {
        self.style.font = Some(String::from(family));

        self
    }

    pub fn weight(mut self, weight: impl Into<Weight>) -> Self {
        self.style.weight = weight.into();

        self
    }

    pub fn italic(mut self, italic: bool) -> Self {
        self.style.italic = italic;

        self
    }

    // the distance between lines as a multiple of the size, like css's unitless line-height
    pub fn line_height(mut self, multiple: f32) -> Self {
        self.style.line_height = Some(multiple);

        self
    }

    // extra room after every letter in pixels; negative draws letters closer
    pub fn letter_spacing(mut self, pixels: f32) -> Self {
        self.style.letter_spacing = pixels;

        self
    }

    pub fn case(mut self, case: Case) -> Self {
        self.style.case = case;

        self
    }

    pub fn decoration(mut self, decoration: Decoration) -> Self {
        self.style.decoration = decoration;

        self
    }

    // where lines sit across the width the text is given; only text that wraps or elides has spare width
    pub fn align(mut self, align: TextAlign) -> Self {
        self.style.align = align;

        self
    }

    pub fn direction(mut self, direction: TextDirection) -> Self {
        self.style.direction = direction;

        self
    }

    // a variable font's slider by its four-letter tag, like b"wdth" or b"opsz"
    pub fn variation(mut self, tag: &[u8; 4], value: f32) -> Self {
        self.style.variations.push((*tag, value));

        self
    }

    pub fn wrap(mut self) -> Self {
        self.rules.wrap = true;

        self
    }

    pub fn elide(mut self) -> Self {
        self.rules.elide = true;

        self
    }

    pub fn max_lines(mut self, count: usize) -> Self {
        self.rules.max_lines = Some(count);

        self
    }

    pub fn tight(mut self) -> Self {
        self.tight = true;

        self
    }

    // how tall it is once laid out in this width, for wrapped text whose height depends on it
    pub fn height_in(&self, width: f32) -> f32 {
        self.arranged(Some(width)).height
    }

    // laid out at a width when it fits itself to one, else at its own
    fn arranged(&self, width: Option<f32>) -> Rc<Arranged> {
        let fitted = self.rules.wrap || self.rules.elide;

        layout::arrange(
            &self.content,
            &self.style,
            self.rules,
            width.filter(|_| fitted),
        )
    }
}

impl Widget for Text {
    fn width(&self) -> Size {
        // wrapped or elided text takes the width it is given and fits itself into it
        if self.rules.wrap || self.rules.elide {
            return Size::Parent;
        }

        let arranged = self.arranged(None);

        if self.tight {
            let (left, right) = arranged.ink();

            return Size::Fixed(right - left);
        }

        Size::Fixed(arranged.width)
    }

    fn height(&self) -> Size {
        /*
         * the width is only known when drawing,
         * so wrapped text without a line limit cannot know its height yet
         */
        match (self.rules.wrap, self.rules.max_lines) {
            (true, None) => Size::Parent,

            // as tall as its line limit, however many lines it fills
            (true, Some(count)) => {
                let line = layout::arrange("", &self.style, Rules::default(), None);

                let more = count.saturating_sub(1) as f32;

                Size::Fixed(line.height + line.spacing * more)
            }

            (false, _) => Size::Fixed(self.arranged(None).height),
        }
    }

    fn draw(&self, renderer: &mut Renderer, area: Area) {
        let arranged = self.arranged(Some(area.width));

        // the ink starts at the area's left edge, not the pen
        let left = match self.tight {
            true => area.x - arranged.ink().0,
            false => area.x,
        };

        for line in &arranged.lines {
            let baseline = area.y + line.baseline;

            renderer.text(&line.letters, self.color, left, baseline);

            for stroke in &line.strokes {
                let stroke_area = Area::new(
                    left + stroke.x,
                    baseline + stroke.y,
                    stroke.width,
                    stroke.thickness,
                );

                renderer.rectangle(stroke_area, self.color, Corners::default());
            }
        }
    }
}
