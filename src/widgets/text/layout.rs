use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use parley::setting::Tag;
use parley::{
    Alignment, AlignmentOptions, BaseDirection, FontFamily, FontStyle, FontVariation, FontWeight,
    Layout, LineHeight, PositionedLayoutItem, StyleProperty, TextWrapMode,
};

use crate::graphics::Placed;
use crate::graphics::font;

use super::style::{Decoration, Style, TextAlign, TextDirection};

// what's left of an elided line ends in this
const ELLIPSIS: &str = "…";

/*
 * the cache is emptied when it reaches this, like the letter pictures, so
 * text that changes every frame, like a clock, can't grow it without end
 */
const MAX_LAYOUTS: usize = 2048;

// how text that is too wide for its area gets handled
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Rules {
    pub wrap: bool,
    pub elide: bool,
    pub max_lines: Option<usize>,
}

type Key = (String, Style, Rules, Option<u32>, u64);

thread_local! {
    // laying out shapes every letter, so text measured and drawn each frame is laid out once
    static LAYOUTS: RefCell<HashMap<Key, Rc<Arranged>>> = RefCell::new(HashMap::new());
}

// a line drawn with the letters: across from the area's left edge, down from the baseline to its top
pub struct Stroke {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub thickness: f32,
}

// one line ready to draw: its letters, its baseline from the area's top, and its decorations
pub struct ArrangedLine {
    pub letters: Vec<Placed>,
    pub baseline: f32,
    pub strokes: Vec<Stroke>,

    // how far its pen moves, trailing spaces included, so a caret after one moves too
    pub width: f32,
}

// text laid out at a width: its lines and the box they fill
pub struct Arranged {
    pub lines: Vec<ArrangedLine>,
    pub width: f32,
    pub height: f32,

    // from one line's baseline to the next
    pub spacing: f32,

    // where the letters' ink starts and ends on the first line, worked out when asked
    ink: OnceCell<(f32, f32)>,
}

impl Arranged {
    // where the first line's ink starts and ends, counted from the pen's starting point
    pub fn ink(&self) -> (f32, f32) {
        *self.ink.get_or_init(|| ink(self))
    }
}

// `content` laid out in `style`, within `width` when it wraps or elides
pub(super) fn arrange(
    content: &str,
    style: &Style,
    rules: Rules,
    width: Option<f32>,
) -> Rc<Arranged> {
    let key = (
        String::from(content),
        style.clone(),
        rules,
        width.map(f32::to_bits),
        font::generation(),
    );

    if let Some(found) = LAYOUTS.with_borrow(|layouts| layouts.get(&key).cloned()) {
        return found;
    }

    let arranged = Rc::new(lay_out(content, style, rules, width));

    LAYOUTS.with_borrow_mut(|layouts| {
        if layouts.len() >= MAX_LAYOUTS {
            layouts.clear();
        }

        layouts.insert(key, Rc::clone(&arranged));
    });

    arranged
}

fn lay_out(content: &str, style: &Style, rules: Rules, width: Option<f32>) -> Arranged {
    // empty text still measures one line tall, like a line with nothing typed yet
    let empty = content.is_empty();

    let mut text = style.case.apply(if empty { " " } else { content });

    // text that doesn't wrap stays on one line
    if !rules.wrap {
        text = text.replace('\n', " ");
    }

    let layout = build(&text, style, rules.wrap, width);

    let limit = match rules.wrap {
        true => rules.max_lines.unwrap_or(usize::MAX).max(1),
        false => 1,
    };

    let count = layout.len();
    let shown = count.min(limit);

    let mut lines: Vec<ArrangedLine> = layout.lines().take(shown).map(arranged_line).collect();

    // a line that was cut off also gets the "…", even if it fits
    if rules.elide
        && let Some(width) = width
        && let Some(last) = layout.get(shown.saturating_sub(1))
    {
        let cut = count > shown;

        let metrics = last.metrics();

        if cut || metrics.advance - metrics.hanging_advance > width {
            let line_text = &text[last.text_range()];

            let baseline = last.metrics().baseline;

            if let Some(line) = lines.last_mut() {
                *line = elided(line_text, style, width, baseline);
            }
        }
    }

    let (top, bottom) = block(&layout, style, shown);

    let spacing = layout.get(0).map_or(0.0, |line| line.metrics().line_height);

    for line in &mut lines {
        line.baseline -= top;
    }

    let width = match empty {
        true => 0.0,
        false => lines.iter().map(|line| line.width).fold(0.0, f32::max),
    };

    if empty {
        lines.clear();
    }

    Arranged {
        lines,
        width,
        height: bottom - top,
        spacing,
        ink: OnceCell::new(),
    }
}

fn build(text: &str, style: &Style, wrap: bool, width: Option<f32>) -> Layout<()> {
    font::with_layout(|fonts, layouts| {
        let mut builder = layouts.ranged_builder(fonts, text, 1.0, false);

        let family = style.font.clone().unwrap_or_else(font::default_family);

        builder.push_default(StyleProperty::FontFamily(FontFamily::from(family.as_str())));
        builder.push_default(StyleProperty::FontSize(style.size));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(f32::from(
            style.weight.value(),
        ))));

        if style.italic {
            builder.push_default(StyleProperty::FontStyle(FontStyle::Italic));
        }

        // none keeps the font's own spacing, which is how amane always spaced lines
        let line_height = match style.line_height {
            Some(multiple) => LineHeight::FontSizeRelative(multiple),
            None => LineHeight::MetricsRelative(1.0),
        };

        builder.push_default(StyleProperty::LineHeight(line_height));

        if style.letter_spacing != 0.0 {
            builder.push_default(StyleProperty::LetterSpacing(style.letter_spacing));
        }

        match style.decoration {
            Decoration::None => {}
            Decoration::Underline => builder.push_default(StyleProperty::Underline(true)),
            Decoration::StrikeThrough => builder.push_default(StyleProperty::Strikethrough(true)),
        }

        let variations: Vec<FontVariation> = style
            .variations
            .iter()
            .map(|(tag, value)| FontVariation::new(Tag::new(tag), *value))
            .collect();

        if !variations.is_empty() {
            builder.push_default(StyleProperty::FontVariations(variations.as_slice().into()));
        }

        if !wrap {
            builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::NoWrap));
        }

        builder.set_base_direction(match style.direction {
            TextDirection::Auto => BaseDirection::Auto,
            TextDirection::LeftToRight => BaseDirection::Ltr,
            TextDirection::RightToLeft => BaseDirection::Rtl,
        });

        let mut layout = builder.build(text);

        // the width lines break at, and the one they align within
        layout.break_all_lines(width);

        let alignment = match style.align {
            TextAlign::Start => Alignment::Start,
            TextAlign::Center => Alignment::Center,
            TextAlign::End => Alignment::End,
            TextAlign::Justify => Alignment::Justify,
        };

        layout.align(alignment, AlignmentOptions::default());

        layout
    })
}

fn arranged_line(line: parley::layout::Line<'_, ()>) -> ArrangedLine {
    let baseline = line.metrics().baseline;
    let width = line.metrics().advance;

    let mut letters = Vec::new();
    let mut strokes = Vec::new();

    for item in line.items() {
        let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
            continue;
        };

        let run = glyph_run.run();

        let coords: Vec<i16> = run
            .normalized_coords()
            .iter()
            .map(|coord| coord.to_bits())
            .collect();

        let face = font::face(run.font(), &coords, run.synthesis());

        let size = run.font_size();

        for glyph in glyph_run.positioned_glyphs() {
            letters.push(Placed {
                face,
                id: glyph.id as u16,
                size,
                x: glyph.x,
                y: glyph.y - baseline,
            });
        }

        let style = glyph_run.style();
        let metrics = run.font_metrics();

        // offsets count up from the baseline to the line's top edge
        if let Some(underline) = &style.underline {
            let offset = underline.offset.unwrap_or(metrics.underline_offset);
            let thickness = underline.size.unwrap_or(metrics.underline_size);

            strokes.push(Stroke {
                x: glyph_run.offset(),
                y: -offset,
                width: glyph_run.advance(),
                thickness,
            });
        }

        if let Some(strikethrough) = &style.strikethrough {
            let offset = strikethrough.offset.unwrap_or(metrics.strikethrough_offset);
            let thickness = strikethrough.size.unwrap_or(metrics.strikethrough_size);

            strokes.push(Stroke {
                x: glyph_run.offset(),
                y: -offset,
                width: glyph_run.advance(),
                thickness,
            });
        }
    }

    ArrangedLine {
        letters,
        baseline,
        strokes,
        width,
    }
}

/*
 * the longest start of the line that still fits with the "…" after it,
 * found by halving, since each try shapes the line again
 */
fn elided(line_text: &str, style: &Style, width: f32, baseline: f32) -> ArrangedLine {
    let ends: Vec<usize> = line_text
        .char_indices()
        .map(|(index, _)| index)
        .chain([line_text.len()])
        .collect();

    let try_end = |end: usize| {
        let shortened = format!("{}{ELLIPSIS}", line_text[..end].trim_end());

        let layout = build(&shortened, style, false, Some(width));

        (layout.full_width() <= width, layout)
    };

    // how many letters are kept: the most that fits, or none
    let mut low = 0;
    let mut high = ends.len() - 1;

    while low < high {
        let middle = (low + high).div_ceil(2);

        if try_end(ends[middle]).0 {
            low = middle;
        } else {
            high = middle - 1;
        }
    }

    // the line it replaces keeps its place, so the shortened one sits on its baseline
    let (_, layout) = try_end(ends[low]);

    let mut line = layout
        .lines()
        .next()
        .map(arranged_line)
        .unwrap_or(ArrangedLine {
            letters: Vec::new(),
            baseline,
            strokes: Vec::new(),
            width: 0.0,
        });

    line.baseline = baseline;

    line
}

/*
 * the top and bottom of the shown lines: with the font's own spacing the box
 * hugs the letters, from the first line's ascent to the last line's descent,
 * as amane always measured text; a set line height keeps the whole line
 * boxes, like css and design tools
 */
fn block(layout: &Layout<()>, style: &Style, shown: usize) -> (f32, f32) {
    let (Some(first), Some(last)) = (layout.get(0), layout.get(shown.saturating_sub(1))) else {
        return (0.0, 0.0);
    };

    match style.line_height {
        Some(_) => (
            first.metrics().block_min_coord,
            last.metrics().block_max_coord,
        ),
        None => (
            first.metrics().content_block_min_coord,
            last.metrics().content_block_max_coord,
        ),
    }
}

fn ink(arranged: &Arranged) -> (f32, f32) {
    let Some(line) = arranged.lines.first() else {
        return (0.0, 0.0);
    };

    let mut left = f32::MAX;
    let mut right = f32::MIN;

    for letter in &line.letters {
        // a space has no ink, only an advance
        if let Some((min, max)) = bounds(letter) {
            left = left.min(letter.x + min);
            right = right.max(letter.x + max);
        }
    }

    // nothing but spaces is as wide as its advance
    if left > right {
        return (0.0, arranged.width);
    }

    (left, right)
}

// where a letter's outline starts and ends across, in pixels from its pen; none for a space
fn bounds(letter: &Placed) -> Option<(f32, f32)> {
    font::with_context(|context| {
        let mut scaler = context
            .builder(letter.face.font)
            .normalized_coords(letter.face.coords)
            .size(letter.size)
            .build();

        let shape = scaler.scale_outline(letter.id)?;

        let bounds = shape.bounds();

        if bounds.width() == 0.0 {
            return None;
        }

        Some((bounds.min.x, bounds.max.x))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(size: f32) -> Style {
        Style {
            size,
            ..Style::default()
        }
    }

    fn width(content: &str, style: &Style) -> f32 {
        arrange(content, style, Rules::default(), None).width
    }

    #[test]
    fn letter_spacing_adds_room_after_every_letter() {
        let plain = style(13.0);
        let spaced = Style {
            letter_spacing: 2.0,
            ..style(13.0)
        };

        let difference = width("hello", &spaced) - width("hello", &plain);

        assert!((difference - 10.0).abs() < 0.01, "{difference}");
    }

    #[test]
    fn a_line_height_is_a_multiple_of_the_size() {
        let set = Style {
            line_height: Some(1.25),
            ..style(13.0)
        };

        let one = arrange("one", &set, Rules::default(), None);

        assert!((one.height - 16.25).abs() < 0.01, "{}", one.height);
        assert!((one.spacing - 16.25).abs() < 0.01, "{}", one.spacing);

        let wrapped = Rules {
            wrap: true,
            ..Rules::default()
        };

        let three = arrange("one\ntwo\nthree", &set, wrapped, Some(500.0));

        assert_eq!(three.lines.len(), 3);
        assert!((three.height - 48.75).abs() < 0.01, "{}", three.height);
    }

    #[test]
    fn the_font_spaces_lines_when_no_height_is_set() {
        let one = arrange("one", &style(13.0), Rules::default(), None);

        // a line hugs its letters, the gap between lines comes on top
        assert!(one.height > 0.0 && one.height <= one.spacing);
    }

    #[test]
    fn empty_text_is_one_line_tall_and_has_no_width() {
        let empty = arrange("", &style(13.0), Rules::default(), None);
        let one = arrange("x", &style(13.0), Rules::default(), None);

        assert_eq!(empty.width, 0.0);
        assert!(empty.lines.is_empty());
        assert_eq!(empty.height, one.height);
    }

    #[test]
    fn trailing_spaces_still_move_the_pen() {
        assert!(width("ab ", &style(13.0)) > width("ab", &style(13.0)));
    }

    #[test]
    fn cut_text_ends_in_an_ellipsis_within_its_lines() {
        let long =
            "the quick brown fox jumps over the lazy dog, then keeps running far past the edge";

        let rules = Rules {
            wrap: true,
            elide: true,
            max_lines: Some(2),
        };

        let arranged = arrange(long, &style(13.0), rules, Some(120.0));

        assert_eq!(arranged.lines.len(), 2);

        for line in &arranged.lines {
            assert!(line.width <= 120.0 + 0.01, "{}", line.width);
        }

        let one_line = Rules {
            elide: true,
            ..Rules::default()
        };

        let elided = arrange(long, &style(13.0), one_line, Some(120.0));

        assert_eq!(elided.lines.len(), 1);
        assert!(elided.width <= 120.0 + 0.01, "{}", elided.width);
        assert!(elided.width > 60.0, "{}", elided.width);
    }

    #[test]
    fn case_changes_what_is_laid_out() {
        let upper = Style {
            case: super::super::Case::Upper,
            ..style(13.0)
        };

        assert!(width("hello", &upper) > width("hello", &style(13.0)));
    }

    #[test]
    fn decorations_are_measured_from_the_font() {
        let underlined = Style {
            decoration: Decoration::Underline,
            ..style(13.0)
        };

        let arranged = arrange("under", &underlined, Rules::default(), None);

        let stroke = &arranged.lines[0].strokes[0];

        // below the baseline, as wide as the text
        assert!(stroke.y > 0.0, "{}", stroke.y);
        assert!(stroke.thickness > 0.0);
        assert!((stroke.width - arranged.width).abs() < 0.01);
    }

    #[test]
    fn pairs_are_kerned() {
        let plain = style(40.0);

        let pair = width("AV", &plain);
        let apart = width("A", &plain) + width("V", &plain);

        assert!(pair < apart, "{pair} vs {apart}");
    }
}
