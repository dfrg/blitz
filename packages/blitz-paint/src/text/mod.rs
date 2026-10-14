//! Painting text: the sink the text backend paints into, over an anyrender scene, and the
//! document as the backend asks after it.

mod host;

use std::sync::Arc;

use anyrender::PaintScene;
use blitz_dom::text::blitz_text::NodeKey;
use blitz_dom::text::blitz_text::paint::{
    DecorationLine, Glyph, GlyphRun, InlineBoxPaint, TextSink,
};
use blitz_dom::text::blitz_text::style::TextDecorationStyle;
use kurbo::{Affine, BezPath, Cap, Circle, Insets, Rect, Stroke};
use peniko::Fill;
use style::values::computed::Length;
use style::values::generics::text::GenericTextDecorationLength;

use crate::kurbo_css::{CssBox, Edge, NonUniformRoundedRectRadii};

pub(crate) use self::host::DomPaintHost;

/// Whether every glyph's outline is grown, and left unhinted, as some platforms draw text.
const FONT_EMBOLDEN_ENABLED: bool = cfg!(any(
    feature = "font-embolden",
    all(feature = "apple-font-embolden", target_os = "macos"),
    all(feature = "apple-font-embolden", target_os = "ios"),
));

/// An anyrender scene, as the text backend paints into it, with what paints the document's
/// atomic inlines where the lines paint them.
pub(crate) struct SceneSink<'s, S, A> {
    pub(crate) scene: &'s mut S,
    pub(crate) atomic: A,
}

impl<S: PaintScene, A: FnMut(&mut S, NodeKey)> TextSink for SceneSink<'_, S, A> {
    #[inline]
    fn glyphs(&mut self, run: &GlyphRun<'_>, glyphs: impl Iterator<Item = Glyph> + Clone) {
        self.scene.draw_glyphs(
            run.font,
            run.size,
            run.hint,
            run.coords,
            run.embolden,
            Fill::NonZero,
            &anyrender::Paint::from(run.color),
            1.0,
            run.transform,
            run.glyph_transform,
            glyphs.map(|glyph| anyrender::Glyph {
                id: glyph.id,
                x: glyph.x,
                y: glyph.y,
            }),
        );
    }

    #[inline]
    fn fill(&mut self, transform: Affine, rect: Rect, color: crate::color::Color) {
        self.scene
            .fill(Fill::NonZero, transform, color, None, &rect);
    }

    #[inline]
    fn selection(&mut self, transform: Affine, rect: Rect) {
        self.scene.fill(
            Fill::NonZero,
            transform,
            crate::SELECTION_COLOR,
            None,
            &rect,
        );
    }

    #[inline]
    fn decoration(&mut self, line: &DecorationLine) {
        draw_decoration_line(self.scene, line);
    }

    fn inline_box(&mut self, inline_box: &InlineBoxPaint) {
        paint_inline_box(self.scene, inline_box);
    }

    fn push_blur(&mut self, transform: Affine, clip: Rect, deviation: f32) {
        let filter =
            anyrender::filters::Filter::single(anyrender::filters::FilterEffect::blur(deviation));
        self.scene.push_layer(
            Fill::NonZero,
            peniko::Mix::Normal,
            1.0,
            transform,
            &clip,
            Some(Arc::new(filter)),
            None,
        );
    }

    fn pop_blur(&mut self) {
        self.scene.pop_layer();
    }

    #[inline]
    fn atomic(&mut self, key: NodeKey) {
        (self.atomic)(self.scene, key);
    }

    #[inline]
    fn emboldens(&self) -> bool {
        FONT_EMBOLDEN_ENABLED
    }
}

/// How far, in CSS pixels, the decorations an element draws may reach past its box: its font
/// size, as far as a decoration sits from the text, plus its thickness and underline offset. The
/// overflow the culling reads leaves this ink out.
pub(crate) fn decoration_reach(styles: &style::properties::ComputedValues) -> f64 {
    let text = styles.get_text();
    if text.text_decoration_line.is_empty() {
        return 0.0;
    }
    let font_size = Length::new(styles.get_font().font_size.computed_size().px());
    let thickness = match &text.text_decoration_thickness {
        GenericTextDecorationLength::LengthPercentage(lp) => lp.resolve(font_size).px().abs(),
        GenericTextDecorationLength::FromFont | GenericTextDecorationLength::Auto => 0.0,
    };
    let offset = styles
        .get_inherited_text()
        .text_underline_offset
        .non_auto()
        .map_or(0.0, |lp| lp.resolve(font_size).px().abs());
    f64::from(font_size.px() + thickness + offset)
}

/// Mirrors Blink's `SelectBestDashGap`: choose the gap length (as close as possible to
/// `gap_length`) that fits a whole number of `dash_length` dashes across `stroke_length`,
/// so dashes are evenly distributed and a dash lands at each end of the line.
fn select_best_dash_gap(stroke_length: f64, dash_length: f64, gap_length: f64) -> f64 {
    let available_length = stroke_length + gap_length;
    let min_num_dashes = (available_length / (dash_length + gap_length)).floor();
    let max_num_dashes = min_num_dashes + 1.0;
    let min_num_gaps = min_num_dashes - 1.0;
    let max_num_gaps = max_num_dashes - 1.0;
    if min_num_gaps < 1.0 {
        return gap_length;
    }
    let min_gap = (stroke_length - min_num_dashes * dash_length) / min_num_gaps;
    let max_gap = (stroke_length - max_num_dashes * dash_length) / max_num_gaps;
    if max_gap <= 0.0 || (min_gap - gap_length).abs() < (max_gap - gap_length).abs() {
        min_gap
    } else {
        max_gap
    }
}

/// Draws a single decoration line of its `text-decoration-style`. Geometry is built explicitly
/// (rather than relying on renderer dash support) so it renders consistently across renderers.
fn draw_decoration_line(scene: &mut impl PaintScene, line: &DecorationLine) {
    let DecorationLine {
        transform,
        scale,
        x0,
        width,
        baseline,
        offset,
        size,
        color,
        double_dir,
        style: deco_style,
        inset_start,
        inset_end,
        clip,
    } = *line;
    let brush = &anyrender::Paint::from(color);
    let within = |at: f64| clip.0 <= at && at < clip.1;
    // Inset the line from the box's start/end edges (assumes LTR: start is the left edge).
    // Negative insets extend the line past the text.
    let x = x0 + inset_start;
    let w = width - inset_start - inset_end;
    if w <= 0.0 {
        return;
    }
    let y = (baseline - offset + size / 2.0) as f64;
    let size = size as f64;
    let butt_stroke = Stroke::new(size).with_caps(Cap::Butt);

    match deco_style {
        TextDecorationStyle::None => {
            // no line. Equivalent to `text-decoration-line: none`
        }
        // `solid`
        TextDecorationStyle::Solid => {
            let (from, to) = (x.max(clip.0), (x + w).min(clip.1));
            if from < to {
                let line = kurbo::Line::new((from, y), (to, y));
                scene.stroke(&butt_stroke, transform, brush, None, &line);
            }
        }
        // Two lines, each `size` thick, separated by a 1px (CSS) gap. This
        // matches Chrome, where the second line is offset by `thickness + 1px`
        // and sits further from the text (below for underline, above for
        // overline).
        TextDecorationStyle::Double => {
            let one_css_px = scale;
            let (from, to) = (x.max(clip.0), (x + w).min(clip.1));
            if from < to {
                for cy in [y, y + double_dir * (size + one_css_px)] {
                    let line = kurbo::Line::new((from, cy), (to, cy));
                    scene.stroke(&butt_stroke, transform, brush, None, &line);
                }
            }
        }
        // Round dots (diameter = line thickness) spaced one dot apart.
        TextDecorationStyle::Dotted => {
            let radius = size / 2.0;
            let step = 2.0 * size;
            let mut cx = x + radius;
            while cx <= x + w - radius {
                if within(cx) {
                    let dot = Circle::new((cx, y), radius);
                    scene.fill(Fill::NonZero, transform, brush, None, &dot);
                }
                cx += step;
            }
        }
        // Dashes as filled rectangles. Dash/gap lengths are relative to the
        // thickness (matching Blink): thinner lines use proportionally longer
        // dashes and gaps so they don't read as dotted or solid.
        TextDecorationStyle::Dashed => {
            let (dash_ratio, gap_ratio) = if size >= 3.0 { (2.0, 1.0) } else { (3.0, 2.0) };
            let dash = size * dash_ratio;
            let nominal_gap = size * gap_ratio;
            // Nudge the gap so a whole number of dashes fits the run evenly.
            let gap = if w > 2.0 * dash {
                select_best_dash_gap(w, dash, nominal_gap)
            } else {
                nominal_gap
            };
            let mut dx = x;
            while dx < x + w {
                if within(dx) {
                    let end = (dx + dash).min(x + w);
                    let rect = Rect::new(dx, y - size / 2.0, end, y + size / 2.0);
                    scene.fill(Fill::NonZero, transform, brush, None, &rect);
                }
                dx += dash + gap;
            }
        }
        // A squiggle built from alternating quadratic Béziers.
        TextDecorationStyle::Wavy => {
            let amplitude = size;
            let half_wave = (2.0 * size).max(1.0);
            let mut path = BezPath::new();
            let mut px = x;
            let mut up = true;
            let mut open = false;
            while px < x + w {
                let nx = (px + half_wave).min(x + w);
                if within(px) {
                    if !open {
                        path.move_to((px, y));
                        open = true;
                    }
                    // A quad Bézier reaches half the control-point offset at its
                    // midpoint, so use `2 * amplitude` to hit the target peak.
                    let ctrl_y = if up {
                        y - 2.0 * amplitude
                    } else {
                        y + 2.0 * amplitude
                    };
                    path.quad_to(((px + nx) / 2.0, ctrl_y), (nx, y));
                }
                px = nx;
                up = !up;
            }
            if open {
                let stroke = Stroke::new(size).with_caps(Cap::Round);
                scene.stroke(&stroke, transform, brush, None, &path);
            }
        }
    }
}

/// Draws a stretch of an inline box: its background, its borders and its outline.
fn paint_inline_box(scene: &mut impl PaintScene, inline_box: &InlineBoxPaint) {
    let InlineBoxPaint {
        transform,
        border_box,
        widths: insets,
        radii,
        background,
        colors,
        outline,
    } = *inline_box;
    let radii = NonUniformRoundedRectRadii {
        top_left: radii[0],
        top_right: radii[1],
        bottom_right: radii[2],
        bottom_left: radii[3],
    };
    let css_box = CssBox::new(border_box, insets, Insets::ZERO, 0.0, radii);
    if background.components[3] > 0.0 {
        scene.fill(
            Fill::NonZero,
            transform,
            background,
            None,
            &css_box.border_box_path(),
        );
    }
    let edges = [
        (Edge::Top, insets.y0, colors[0]),
        (Edge::Right, insets.x1, colors[1]),
        (Edge::Bottom, insets.y1, colors[2]),
        (Edge::Left, insets.x0, colors[3]),
    ];
    // Edges of one color around square corners are one ring, so no seam shows where two meet.
    let drawn = edges.iter().filter(|(_, width, _)| *width > 0.0);
    let mut colors_drawn = drawn.clone().map(|(_, _, color)| *color);
    let one_color = colors_drawn
        .next()
        .filter(|first| colors_drawn.all(|color| color == *first));
    match one_color {
        Some(color) if !css_box.has_border_radius() => {
            let mut ring = css_box.border_box_path();
            ring.extend(css_box.padding_box_path());
            scene.fill(Fill::EvenOdd, transform, color, None, &ring);
        }
        _ => {
            for (edge, _, color) in drawn {
                let shape = css_box.border_edge_shape(*edge);
                scene.fill(Fill::NonZero, transform, *color, None, &shape);
            }
        }
    }
    if let Some((outline_width, color)) = outline {
        let outlined = CssBox::new(border_box, insets, Insets::ZERO, outline_width, radii);
        scene.fill(Fill::NonZero, transform, color, None, &outlined.outline());
    }
}
