//! Text decorations: what a decorating box draws, resolved from its style, and the lines a line
//! of text draws for each decorating box over it.

use std::collections::HashMap;

use kurbo::Affine;
use linebender_resource_handle::FontData;

use super::{DecorationLine, TextSink};
use crate::NodeKey;
use crate::style::{
    Color, LengthPercentage, TextDecorationInset, TextDecorationLine, TextDecorationStyle,
    TextDecorationThickness, TextStyle, TextUnderlinePosition,
};

/// Each font's OS/2 `usWinAscent / unitsPerEm`, keyed by its data's id and its index. `None` is
/// kept for a font without a usable OS/2 or head table, so it is not parsed again either.
pub type WinAscentCache = HashMap<(u64, u32), Option<f32>>;

/// The font's OS/2 `usWinAscent`, in the same (device) pixels as `font_size`.
///
/// This is the ascent browsers use as the top of the em box when placing overlines. It is
/// typically taller than the hhea ascent a shaper reports. `None` where the font has no OS/2
/// table.
fn win_ascent(cache: &mut WinAscentCache, font: &FontData, font_size: f32) -> Option<f32> {
    let ratio = *cache
        .entry((font.data.id(), font.index))
        .or_insert_with(|| win_ascent_ratio(font));
    Some(ratio? * font_size)
}

/// The unitless `usWinAscent / unitsPerEm` of a face, or `None` where it has no usable head or
/// OS/2 table.
fn win_ascent_ratio(font: &FontData) -> Option<f32> {
    use skrifa::raw::{FontRef, TableProvider as _};
    let font_ref = FontRef::from_index(font.data.as_ref(), font.index).ok()?;
    let units_per_em = font_ref.head().ok()?.units_per_em();
    let win_ascent = font_ref.os2().ok()?.us_win_ascent();
    Some(win_ascent as f32 / units_per_em as f32)
}

/// A text decoration resolved from one decorating box (an element that sets
/// `text-decoration-line`), ready to be drawn over a run of text. The `text-decoration-*`
/// properties do not inherit: an ancestor's decorations propagate to the text it holds, so a run
/// may be under several of these.
///
/// Only the values that depend on the style are kept, so that one is resolved once for a
/// decorating box and drawn over every run in it. `text-decoration-inset` and
/// `text-underline-offset` depend on the run's advance and font size, and are resolved where
/// the line is drawn.
#[derive(Clone, Debug)]
pub struct ResolvedDecoration {
    pub line: TextDecorationLine,
    pub style: TextDecorationStyle,
    pub color: Color,
    pub thickness: TextDecorationThickness,
    /// `text-underline-offset`; `None` for `auto`.
    pub underline_offset: Option<LengthPercentage>,
    /// Whether `text-underline-position: under` is in effect.
    pub underline_under: bool,
    /// Whether `text-underline-position: from-font` is in effect.
    pub underline_from_font: bool,
    /// `text-decoration-inset`.
    pub inset: TextDecorationInset,
}

impl ResolvedDecoration {
    /// Returns the decoration an element set in `style` draws as a decorating box, if it is one.
    pub fn of(style: &impl TextStyle) -> Option<Self> {
        let line = style.text_decoration_line();
        // Decorations propagate through the box tree, and a `display: contents` element
        // generates no box, so its decorations have no effect on what it holds.
        if style.display().is_contents() || !line.intersects(TextDecorationLine::DRAWN) {
            return None;
        }
        let position = style.text_underline_position();
        Some(Self {
            line,
            style: style.text_decoration_style(),
            // `currentcolor`, the initial value, is the decorating box's own color, not that of
            // the run under it.
            color: style.text_decoration_color(),
            thickness: style.text_decoration_thickness(),
            underline_offset: style.text_underline_offset(),
            underline_under: position.contains(TextUnderlinePosition::UNDER),
            underline_from_font: position.contains(TextUnderlinePosition::FROM_FONT),
            inset: style.text_decoration_inset(),
        })
    }
}

/// The underline under the text an input method is composing: solid, `auto` thick, and in
/// `color`.
pub fn composition_underline(color: Color) -> ResolvedDecoration {
    ResolvedDecoration {
        line: TextDecorationLine::UNDERLINE,
        style: TextDecorationStyle::Solid,
        color,
        thickness: TextDecorationThickness::Auto,
        underline_offset: None,
        underline_under: false,
        underline_from_font: false,
        inset: TextDecorationInset::Auto,
    }
}

/// An element on the path from an inline root to the run being painted, with the values read
/// from its style, so that the runs after it in the same element read them once.
pub struct DecorationStackEntry {
    pub key: NodeKey,
    /// Its text color, which the glyphs of runs whose innermost node it is are drawn in.
    pub text_color: Color,
    /// The decoration it draws as a decorating box, if it is one.
    pub decoration: Option<ResolvedDecoration>,
}

/// The geometry a decoration is placed and sized by, taken from one run of text. A decorating
/// box's decoration is drawn once over the box in the box's own font, so the values of a run
/// whose innermost node is the box are taken first, and the first run it covers otherwise.
#[derive(Clone, Debug)]
pub struct DecorationRunGeometry {
    /// The baseline the decoration is placed from: the run's own, as `vertical-align` shifted
    /// it.
    pub baseline: f32,
    pub ascent: f32,
    pub descent: f32,
    pub underline_offset: f32,
    pub underline_size: f32,
    pub strikethrough_size: f32,
    /// The font whose OS/2 ascent an overline and a line-through are placed by, where `ascent`
    /// is not already the one to use.
    pub font: Option<FontData>,
    pub font_size: f32,
    pub css_font_size: f64,
    /// Whether `ascent` and `descent` are measured about a central baseline, as a vertical
    /// line's are, so that the em box's edges are the overline's place and its middle the
    /// line-through's.
    pub centered: bool,
}

/// A decoration gathered across the runs of one line for one decorating box.
///
/// The `text-decoration-*` properties describe the whole box, so the extent it covers on the
/// line and the font geometry to draw with are gathered, then one line is drawn spanning the box
/// rather than one piece a run.
#[derive(Clone, Debug)]
pub struct LineDecoration {
    pub key: NodeKey,
    pub deco: ResolvedDecoration,
    pub min_x: f64,
    pub max_x: f64,
    /// The part of `min_x..max_x` drawn, where only a part is: the line is inset and patterned as
    /// the whole is, and drawn over that part and past an end it holds.
    pub piece: Option<(f64, f64)>,
    /// Geometry from a run whose innermost node is the decorating box itself.
    pub own: Option<DecorationRunGeometry>,
    /// Geometry from the first run the box covers, where it has no text of its own on the line.
    pub first: Option<DecorationRunGeometry>,
}

/// Returns the `text-decoration-thickness` `thickness` in whole device pixels, at least one.
///
/// Percentages are of the font size, `from-font` is `metric_size`, and `auto` is a tenth of the
/// font size, at least a CSS pixel.
pub fn decoration_size(
    thickness: &TextDecorationThickness,
    metric_size: f32,
    css_font_size: f64,
    scale: f64,
) -> f32 {
    (match thickness {
        TextDecorationThickness::Length(lp) => lp.resolve(css_font_size as f32) as f64 * scale,
        TextDecorationThickness::FromFont => metric_size as f64,
        TextDecorationThickness::Auto => (css_font_size / 10.0).max(1.0) * scale,
    })
    .floor()
    .max(1.0) as f32
}

/// Draws the decorations gathered for one line, one line for each decorating box.
///
/// Only the kinds of line in `drawn` are drawn: a caller drawing the lines under the text apart
/// from the one through it asks for each in turn.
#[allow(clippy::too_many_arguments)]
pub fn flush_line_decorations(
    sink: &mut impl TextSink,
    transform: Affine,
    scale: f64,
    deco_boxes: &[LineDecoration],
    win_ascent_ratios: &mut WinAscentCache,
    inline_root: NodeKey,
    line_baseline: f32,
    drawn: TextDecorationLine,
) {
    // Innermost boxes first, so that ancestors' decorations draw over them.
    for acc in deco_boxes.iter().rev() {
        let deco = &acc.deco;
        // The decorating box's own font; else the first run it covers.
        let geom = match (&acc.own, &acc.first) {
            (Some(own), _) => own.clone(),
            (None, Some(first)) => {
                let mut geom = first.clone();
                // A descendant run's baseline may be shifted by `vertical-align`, but the
                // decoration is placed from the decorating box's own baseline. For the inline
                // root that is the line's baseline.
                if acc.key == inline_root {
                    geom.baseline = line_baseline;
                }
                geom
            }
            (None, None) => continue,
        };
        let geom = &geom;
        let width = acc.max_x - acc.min_x;
        if width <= 0.0 {
            continue;
        }

        // `text-decoration-inset` shortens (or, when negative, lengthens) the line from the
        // inline-start and end edges. Percentages are of the line's length, the box's advance on
        // this line; `auto` is no inset.
        let (inset_start, inset_end) = match deco.inset {
            TextDecorationInset::Length { ref start, ref end } => {
                // The extent is in device pixels, and percentages resolve in CSS pixels.
                let line_length = (width / scale) as f32;
                (
                    start.resolve(line_length) as f64 * scale,
                    end.resolve(line_length) as f64 * scale,
                )
            }
            TextDecorationInset::Auto => (0.0, 0.0),
        };
        // A piece draws what of the whole line lies over it, and past an end of the span it
        // holds. Its ends inside the span are on whole pixels, where the piece beside it starts
        // or ends too.
        let clip = match acc.piece {
            Some((left, right)) => (
                if left > acc.min_x {
                    left.round()
                } else {
                    f64::NEG_INFINITY
                },
                if right < acc.max_x {
                    right.round()
                } else {
                    f64::INFINITY
                },
            ),
            None => (f64::NEG_INFINITY, f64::INFINITY),
        };
        let line = |baseline: f32, offset: f32, size: f32, double_dir: f64| DecorationLine {
            transform,
            scale,
            x0: acc.min_x,
            width,
            baseline,
            offset,
            size,
            color: deco.color,
            double_dir,
            style: deco.style,
            inset_start,
            inset_end,
            clip,
        };

        let lines = deco.line & drawn;
        if lines.contains(TextDecorationLine::UNDERLINE) {
            let size = decoration_size(
                &deco.thickness,
                geom.underline_size,
                geom.css_font_size,
                scale,
            );

            // `text-underline-offset` moves the underline away from the text. `auto` keeps the
            // font's suggested position; otherwise percentages are of the font size.
            let extra_underline_offset = deco
                .underline_offset
                .as_ref()
                .map(|lp| lp.resolve(geom.css_font_size as f32) as f64 * scale)
                .unwrap_or(0.0);

            // `text-underline-position: under` places the underline below the glyph box, clear
            // of descenders, rather than at the font's suggested position. An explicit offset
            // under `auto` is taken from the alphabetic baseline, as CSS and Chrome have it, not
            // from the auto gap.
            let base_offset = if deco.underline_under {
                -geom.descent
            } else if deco.underline_offset.is_some() && !deco.underline_from_font {
                0.0
            } else {
                geom.underline_offset
            };
            // The underline sits on a whole device pixel from the baseline, as Chrome's
            // `TextDecorationOffset` rounds its offset.
            let offset = (base_offset - extra_underline_offset as f32).round();

            // A `double` underline grows downward, away from the text.
            sink.decoration(&line(geom.baseline, offset, size, 1.0));
        }
        if lines.contains(TextDecorationLine::OVERLINE) {
            // Fonts give no overline metric, so the underline's thickness is taken. The line sits
            // on the em box's top: its lower edge rests on the ascent, clear of the glyphs.
            //
            // Browsers take the OS/2 `usWinAscent` for this edge, which is taller than the hhea
            // ascent; the run's ascent is taken for fonts without a usable OS/2 table.
            let size = decoration_size(
                &deco.thickness,
                geom.underline_size,
                geom.css_font_size,
                scale,
            );
            let ascent = match &geom.font {
                Some(font) if !geom.centered => {
                    win_ascent(win_ascent_ratios, font, geom.font_size).unwrap_or(geom.ascent)
                }
                _ => geom.ascent,
            };
            let offset = ascent + size;

            // A `double` overline grows upward, away from the text.
            sink.decoration(&line(geom.baseline, offset, size, -1.0));
        }
        if lines.contains(TextDecorationLine::LINE_THROUGH) {
            let size = decoration_size(
                &deco.thickness,
                geom.strikethrough_size,
                geom.css_font_size,
                scale,
            );

            // The line through is centred a third of the ascent above the baseline, as Chrome
            // places it (`2/3 * ascent` below the text top). A vertical line's is through the
            // middle of the em box, which is its baseline.
            let offset = if geom.centered {
                size / 2.0
            } else {
                let ascent = geom
                    .font
                    .as_ref()
                    .and_then(|font| win_ascent(win_ascent_ratios, font, geom.font_size))
                    .unwrap_or(geom.ascent);
                ascent / 3.0 + size / 2.0
            };

            sink.decoration(&line(geom.baseline, offset, size, 1.0));
        }
    }
}
