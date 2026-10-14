//! Painting text laid out by winkin into a [`TextSink`].
//!
//! winkin hands out geometry and order, and nothing it hands out is drawn by
//! it: each line's [`Line::paints`] walks what the line paints in Chrome's
//! order, and this draws each with what the node it names is styled in.
//!
//! [`Line::paints`]: winkin::Line::paints

use core::cell::{Cell, RefCell};

use blitz_text::paint::{
    DecorationRunGeometry, Glyph, GlyphRun, InlineBoxPaint, LineDecoration, PaintFrame, PaintHost,
    PaintScratch, ResolvedDecoration, TextSink, composition_underline, decoration_size,
    flush_line_decorations,
};
use blitz_text::style::{
    BorderStyle, Color, CornerRadius, Corners, OutlineStyle, TextDecorationLine,
    TextDecorationThickness, TextOrientation, TextStyle,
};
use blitz_text::{EditableText as _, InlineText as _, Marker, MarkerFrame, NodeKey};
use kurbo::{Affine, Insets, Point, Rect, Vec2};
use linebender_resource_handle::{Blob, FontData};
use winkin::paint::{Decorates, Decoration, EmphasisMark, Paint};
use winkin::selection::{SelectionRect, SelectionRectKind};
use winkin::style::WritingMode;
use winkin::{BoxFragment, FontInstance, Layout, RunOrientation, TextRun};

use crate::{COMPOSE_KEY, TextEditor, TextLayout, is_synthetic_key, line_frame, page};

/// A box's four sides in a line's own terms, from its physical ones: its
/// line-left and line-right, and its over and under sides.
#[derive(Copy, Clone)]
struct LineSides<T> {
    left: T,
    right: T,
    over: T,
    under: T,
}

impl<T> LineSides<T> {
    /// `top`, `right`, `bottom` and `left` where a line in `writing_mode`
    /// puts them.
    fn from_physical(writing_mode: WritingMode, top: T, right: T, bottom: T, left: T) -> Self {
        match writing_mode {
            WritingMode::HorizontalTb => Self {
                left,
                right,
                over: top,
                under: bottom,
            },
            WritingMode::VerticalRl | WritingMode::VerticalLr | WritingMode::SidewaysRl => Self {
                left: top,
                right: bottom,
                over: right,
                under: left,
            },
            WritingMode::SidewaysLr => Self {
                left: bottom,
                right: top,
                over: left,
                under: right,
            },
        }
    }
}

/// Paints every line of `layout`: the boxes, decorations, ruby annotations,
/// text, emphasis marks and generated text of each, in Chrome's order.
///
/// `transform` takes the content box's device pixels onto the scene, and
/// `root` is the block the layout was built for. Each atomic inline is painted
/// where the line paints it, after what comes before it in tree order and under
/// what comes after.
#[allow(clippy::too_many_arguments)]
fn paint<S: TextStyle>(
    sink: &mut impl TextSink,
    text: &TextLayout<S>,
    layout: &Layout,
    host: &impl PaintHost<S>,
    transform: Affine,
    content_width: f64,
    content_height: f64,
    scale: f64,
    root: NodeKey,
    scratch: &RefCell<PaintScratch>,
    selection: Option<(usize, usize)>,
) {
    let writing_mode = text.writing_mode();
    let page = page(writing_mode, content_width, content_height);
    let selection_rects: Vec<_> = selection
        .map(|(start, end)| layout.selection_rects(start..end).collect())
        .unwrap_or_default();
    let root_key = root.0;
    let root_styles = host.style(root);
    // Whether glyphs stand upright on the line's central baseline, which
    // decorations are measured about.
    let centered = matches!(
        writing_mode,
        WritingMode::VerticalRl | WritingMode::VerticalLr
    ) && root_styles
        .as_ref()
        .is_none_or(|styles| styles.text_orientation() != TextOrientation::Sideways);
    let rtl = root_styles
        .as_ref()
        .is_some_and(|styles| styles.direction() == blitz_text::style::Direction::Rtl);
    let propagated = host.propagated_decorations(root);
    let painter = Painter {
        text,
        host,
        transform,
        containing: kurbo::Size::new(content_width / scale, content_height / scale),
        rtl,
        shifted: Cell::new(None),
        scale,
        writing_mode,
        root_key,
        root_styles,
        propagated,
        centered,
        color: Cell::new(None),
        line: Cell::new((Affine::IDENTITY, 0.0)),
    };
    let mut items = Vec::new();
    // Where each key's bars on the line start and end, which its
    // `text-decoration-inset` is taken from.
    let mut spans: Vec<(u64, f64, f64)> = Vec::new();
    // The line's selection rectangles, each with the run it is painted
    // under, where it selects a run's text.
    let mut highlights: Vec<(SelectionRect, Option<usize>)> = Vec::new();
    for line in layout.lines() {
        // The block's first formatted line is set in its `::first-line`
        // styles, where it has them.
        let first = line.index() == 0;
        let metrics = line.metrics();
        let frame = line_frame(writing_mode, page, &metrics);
        painter.line.set((frame, f64::from(metrics.height())));
        items.clear();
        items.extend(line.paints(|key| painter.decorates(key, first)));
        spans.clear();
        for item in &items {
            if let Paint::DecorationBeforeText(bar) | Paint::DecorationAfterText(bar) = item {
                let (key, inline) = (bar.key().0, bar.inline());
                let (left, right) = (f64::from(inline.left), f64::from(inline.right));
                match spans.iter_mut().find(|span| span.0 == key) {
                    Some(span) => {
                        span.1 = span.1.min(left);
                        span.2 = span.2.max(right);
                    }
                    None => spans.push((key, left, right)),
                }
            }
        }
        highlights.clear();
        highlights.extend(
            selection_rects
                .iter()
                .filter(|rect| rect.line == line.index())
                .map(|rect| {
                    let under = (rect.kind == SelectionRectKind::Text)
                        .then(|| {
                            items.iter().position(|item| {
                                text_run(item).is_some_and(|run| {
                                    let inline = run.inline();
                                    inline.left <= rect.inline.left
                                        && rect.inline.right <= inline.right
                                })
                            })
                        })
                        .flatten();
                    (*rect, under)
                }),
        );
        // The run whose marks follow it, and whose font they are set in.
        let mut marked: Option<TextRun<'_>> = None;
        // Whether the highlight that selects no run's text is painted: over
        // the backgrounds, under the rest.
        let mut highlighted = false;
        let mut at = 0;
        while let Some(&item) = items.get(at) {
            if !highlighted && !matches!(item, Paint::Background(_) | Paint::Box(_)) {
                paint_selection(sink, &highlights, None, frame, transform);
                highlighted = true;
            }
            match item {
                Paint::Background(background) => {
                    painter.paint_first_line_background(sink, frame, first, background);
                }
                Paint::Box(fragment) => painter.paint_box(sink, frame, &fragment, first),
                Paint::Atomic(atomic) => sink.atomic(NodeKey(atomic.key().0)),
                _ => {
                    // A run of text with its bars and marks: its highlight,
                    // then its text shadows and its decorations' shadows, the
                    // last of each list lowest, then it.
                    let end = run_end(&items, at);
                    let run = &items[at..end];
                    if let Some(index) = run.iter().position(|item| text_run(item).is_some()) {
                        paint_selection(sink, &highlights, Some(at + index), frame, transform);
                    }
                    let shadows = run
                        .iter()
                        .filter_map(|item| painter.shadows_of(item, first))
                        .max()
                        .unwrap_or(0);
                    for pass in (0..shadows).rev().map(Some).chain([None]) {
                        for item in run {
                            match *item {
                                Paint::DecorationBeforeText(bar)
                                | Paint::DecorationAfterText(bar) => {
                                    let after = matches!(item, Paint::DecorationAfterText(_));
                                    let key = bar.key().0;
                                    let span = spans
                                        .iter()
                                        .find(|span| span.0 == key)
                                        .map_or((0.0, 0.0), |span| (span.1, span.2));
                                    painter.paint_decoration(
                                        sink,
                                        frame,
                                        &bar,
                                        span,
                                        first,
                                        after,
                                        pass,
                                        &mut scratch.borrow_mut(),
                                    );
                                }
                                Paint::Annotation(run) | Paint::Text(run) => {
                                    painter.draw_run(sink, frame, &run, first, pass);
                                    marked = Some(run);
                                }
                                Paint::Generated(run) => {
                                    painter.draw_run(sink, frame, &run, first, pass);
                                }
                                Paint::Emphasis(mark) => {
                                    painter.draw_emphasis(
                                        sink,
                                        frame,
                                        &mark,
                                        marked.as_ref(),
                                        first,
                                        pass,
                                    );
                                }
                                _ => {}
                            }
                        }
                    }
                    at = end;
                    continue;
                }
            }
            at += 1;
        }
        if !highlighted {
            paint_selection(sink, &highlights, None, frame, transform);
        }
    }
}

/// The run of text `item` draws, where it draws one.
fn text_run<'a>(item: &Paint<'a>) -> Option<TextRun<'a>> {
    match *item {
        Paint::Text(run) | Paint::Annotation(run) | Paint::Generated(run) => Some(run),
        _ => None,
    }
}

/// Where the run of text that starts at `items[start]` ends: after its
/// underlines and overlines, its text, its emphasis marks and its lines
/// through.
fn run_end(items: &[Paint<'_>], start: usize) -> usize {
    let mut end = start;
    while matches!(items.get(end), Some(Paint::DecorationBeforeText(_))) {
        end += 1;
    }
    if items.get(end).is_some_and(|item| text_run(item).is_some()) {
        end += 1;
    }
    while matches!(
        items.get(end),
        Some(Paint::Emphasis(_) | Paint::DecorationAfterText(_))
    ) {
        end += 1;
    }
    end.max(start + 1)
}

/// Paints the selection rectangles of `highlights` painted under the item at
/// `under`, or, where it is `None`, those painted under no run.
fn paint_selection(
    sink: &mut impl TextSink,
    highlights: &[(SelectionRect, Option<usize>)],
    under: Option<usize>,
    frame: Affine,
    transform: Affine,
) {
    for (rect, _) in highlights.iter().filter(|(_, held)| *held == under) {
        let shape = Rect::new(
            f64::from(rect.inline.left),
            f64::from(rect.block.over),
            f64::from(rect.inline.right),
            f64::from(rect.block.under),
        );
        sink.selection(transform * frame, shape);
    }
}

/// What one layout is painted with.
struct Painter<'a, S, H> {
    text: &'a TextLayout<S>,
    host: &'a H,
    transform: Affine,
    /// The block's content box in CSS pixels, which relative offsets'
    /// percentages are of.
    containing: kurbo::Size,
    /// Whether the block is right-to-left, where `right` outweighs `left`.
    rtl: bool,
    /// The scene transform `position: relative` moved last, and whose it was.
    shifted: Cell<Option<(u64, Affine)>>,
    scale: f64,
    writing_mode: WritingMode,
    root_key: u64,
    root_styles: Option<S>,
    /// The styles of the boxes around the block whose decorations it
    /// draws, the outermost first.
    propagated: Vec<S>,
    centered: bool,
    /// The text color painted last, and whose it was.
    color: Cell<Option<(u64, bool, Color)>>,
    /// The frame of the line being painted, and its height: what a blurred
    /// shadow is drawn within is found from them.
    line: Cell<(Affine, f64)>,
}

impl<S: TextStyle, H: PaintHost<S>> Painter<'_, S, H> {
    /// What takes the content box's device pixels onto the scene for what
    /// `key` paints: moved as `position: relative` moves its inline boxes.
    fn transform(&self, key: u64) -> Affine {
        if key == self.root_key {
            return self.transform;
        }
        if let Some((held, transform)) = self.shifted.get()
            && held == key
        {
            return transform;
        }
        let shift = if is_synthetic_key(key) {
            Vec2::ZERO
        } else {
            self.host
                .relative_shift(NodeKey(key), self.containing, self.rtl)
        };
        let transform = if shift == Vec2::ZERO {
            self.transform
        } else {
            self.transform * Affine::translate(shift * self.scale)
        };
        self.shifted.set(Some((key, transform)));
        transform
    }

    /// The style `key` is painted in, on the first line or not.
    fn styles(&self, key: u64, first: bool) -> Option<S> {
        self.text.computed_style(self.host, key, first)
    }

    /// The color of `key`'s text, on the first line or not.
    fn color(&self, key: u64, first: bool) -> Color {
        if let Some((held, held_first, color)) = self.color.get()
            && held == key
            && held_first == first
        {
            return color;
        }
        let color = self
            .styles(key, first)
            .map_or(Color::BLACK, |styles| styles.color());
        self.color.set(Some((key, first, color)));
        color
    }

    /// The styles whose decorations `key` draws as a decorating box: its
    /// own, and for the block, first those its ancestors propagate to it,
    /// and on the first line its `::first-line` as well, whose decorations
    /// do not inherit into the block's.
    fn decorating_styles(&self, key: u64, first: bool) -> impl Iterator<Item = S> + '_ {
        let (propagated, own, first_line) = if key == self.root_key {
            let first_line = first.then(|| self.text.first_line_style_of(key)).flatten();
            (&self.propagated[..], self.root_styles.clone(), first_line)
        } else {
            (&[][..], self.styles(key, first), None)
        };
        propagated.iter().cloned().chain(own).chain(first_line)
    }

    /// Which decoration lines `key` draws: the question [`winkin::Line::paints`]
    /// asks of the block and each box.
    fn decorates(&self, key: winkin::NodeKey, first: bool) -> Decorates {
        if key.0 & COMPOSE_KEY != 0 {
            return Decorates::BeforeText;
        }
        let mut before = false;
        let mut after = false;
        for styles in self.decorating_styles(key.0, first) {
            // A `display: contents` element is no box, and decorates nothing.
            if styles.display().is_contents() {
                continue;
            }
            let lines = styles.text_decoration_line();
            before |=
                lines.intersects(TextDecorationLine::UNDERLINE | TextDecorationLine::OVERLINE);
            after |= lines.contains(TextDecorationLine::LINE_THROUGH);
        }
        match (before, after) {
            (true, true) => Decorates::Both,
            (true, false) => Decorates::BeforeText,
            (false, true) => Decorates::AfterText,
            (false, false) => Decorates::None,
        }
    }

    /// Paints the block's `::first-line` background behind the first line's
    /// content, where it sets one. The block's own background is its box's,
    /// which the host paints with the box.
    fn paint_first_line_background(
        &self,
        sink: &mut impl TextSink,
        frame: Affine,
        first: bool,
        background: winkin::paint::Background<'_>,
    ) {
        if !first {
            return;
        }
        let Some(styles) = self.text.first_line_style_of(self.root_key) else {
            return;
        };
        let color = styles.background_color();
        if color.components[3] <= 0.0 {
            return;
        }
        let (inline, block) = (background.inline(), background.block());
        let rect = Rect::new(
            f64::from(inline.left),
            f64::from(block.over),
            f64::from(inline.right),
            f64::from(block.under),
        );
        sink.fill(self.transform * frame, rect, color);
    }

    /// Draws a stretch of an inline box: its background and its borders,
    /// and no border on a side where the box goes on past it.
    fn paint_box(
        &self,
        sink: &mut impl TextSink,
        frame: Affine,
        fragment: &BoxFragment<'_>,
        first: bool,
    ) {
        let key = fragment.key().0;
        let Some(styles) = self.styles(key, first) else {
            return;
        };
        let background = styles.background_color();
        let scale = self.scale;
        // Snapped as the layout took them: a width under a pixel to one, and
        // any other down to a whole one.
        let width = |width: f32, style: BorderStyle| {
            let width = f64::from(width) * scale;
            if style.none_or_hidden() || width <= 0.0 {
                0.0
            } else {
                width.floor().max(1.0)
            }
        };
        let (border_widths, border_styles) = (styles.border_widths(), styles.border_styles());
        let widths = LineSides::from_physical(
            self.writing_mode,
            width(border_widths.top, border_styles.top),
            width(border_widths.right, border_styles.right),
            width(border_widths.bottom, border_styles.bottom),
            width(border_widths.left, border_styles.left),
        );
        let border_colors = styles.border_colors();
        let colors = LineSides::from_physical(
            self.writing_mode,
            border_colors.top,
            border_colors.right,
            border_colors.bottom,
            border_colors.left,
        );
        let (inline, block) = (fragment.inline(), fragment.block());
        let border_box = Rect::new(
            f64::from(inline.left),
            f64::from(block.over),
            f64::from(inline.right),
            f64::from(block.under),
        );
        if border_box.width() <= 0.0 && border_box.height() <= 0.0 {
            return;
        }
        // Snapped to whole device pixels, as Chrome snaps a box's decorations, where the
        // transform only moves and scales.
        let transform = self.transform(key) * frame;
        let [_, skew_y, skew_x, ..] = transform.as_coeffs();
        let border_box = if skew_x == 0.0 && skew_y == 0.0 && transform.determinant() != 0.0 {
            let snapped = transform.transform_rect_bbox(border_box).round();
            transform.inverse().transform_rect_bbox(snapped)
        } else {
            border_box
        };
        let open_left = fragment.is_open_left();
        let open_right = fragment.is_open_right();
        let insets = Insets {
            x0: if open_left { 0.0 } else { widths.left },
            y0: widths.over,
            x1: if open_right { 0.0 } else { widths.right },
            y1: widths.under,
        };
        // Each corner of the stretch is a corner of the page's box, which one
        // being the writing mode's; in a vertical line a corner's horizontal
        // radius runs across the line.
        let [left_over, right_over, right_under, left_under] =
            line_corners(self.writing_mode, styles.border_radii());
        let across_is_horizontal = self.writing_mode != WritingMode::HorizontalTb;
        let radius = |corner: CornerRadius, open: bool| {
            if open {
                return Vec2::ZERO;
            }
            let resolve = |length: blitz_text::style::LengthPercentage, of: f64| {
                scale * f64::from(length.resolve((of / scale) as f32))
            };
            let (physical_width, physical_height) = if across_is_horizontal {
                (border_box.height(), border_box.width())
            } else {
                (border_box.width(), border_box.height())
            };
            let horizontal = resolve(corner.width, physical_width);
            let vertical = resolve(corner.height, physical_height);
            if across_is_horizontal {
                Vec2::new(vertical, horizontal)
            } else {
                Vec2::new(horizontal, vertical)
            }
        };
        let radii = [
            radius(left_over, open_left),
            radius(right_over, open_right),
            radius(right_under, open_right),
            radius(left_under, open_left),
        ];
        // The outline round the stretch's border box, as a block's is drawn. Chrome draws one
        // round an empty inline box too.
        let outline = match styles.outline_style() {
            OutlineStyle::Border(outline_style) => {
                let outline_width = width(styles.outline_width(), outline_style);
                (outline_width > 0.0).then(|| (outline_width, styles.outline_color()))
            }
            OutlineStyle::Auto => None,
        };
        sink.inline_box(&InlineBoxPaint {
            transform,
            border_box,
            widths: insets,
            radii,
            background,
            colors: [colors.over, colors.right, colors.under, colors.left],
            outline,
        });
    }

    /// Draws one bar of a decoration, with the decoration `bar`'s box sets.
    /// `span` is where that box's bars on the line start and end.
    #[allow(clippy::too_many_arguments)]
    fn paint_decoration(
        &self,
        sink: &mut impl TextSink,
        frame: Affine,
        bar: &Decoration,
        span: (f64, f64),
        first: bool,
        after_text: bool,
        pass: Option<usize>,
        scratch: &mut PaintScratch,
    ) {
        let inline = bar.inline();
        let key = bar.key().0;
        let geometry = WinkinDecoration {
            left: f64::from(inline.left),
            right: f64::from(inline.right),
            span,
            baseline: bar.baseline(),
            ascent: bar.ascent(),
            descent: bar.descent(),
            underline_gap: bar.underline_gap(),
            font_underline_gap: bar.underline_gap_from_font(),
            font_thickness: bar.underline_thickness_from_font(),
            font_size: bar.font_size(),
            centered: self.centered,
        };
        self.shadowed(
            sink,
            pass,
            self.shadow_styles(key, first).as_ref(),
            self.transform(key),
            (geometry.left, geometry.right),
            None,
            |sink, placed, color| {
                if key & COMPOSE_KEY != 0 {
                    if !after_text {
                        let deco =
                            composition_underline(color.unwrap_or_else(|| self.color(key, first)));
                        draw_bar(
                            sink,
                            scratch,
                            placed * frame,
                            self.scale,
                            deco,
                            geometry,
                            false,
                        );
                    }
                    return;
                }
                for styles in self.decorating_styles(key, first) {
                    let Some(mut deco) = ResolvedDecoration::of(&styles) else {
                        continue;
                    };
                    if let Some(color) = color {
                        deco.color = color;
                    }
                    draw_bar(
                        sink,
                        scratch,
                        placed * frame,
                        self.scale,
                        deco,
                        geometry,
                        after_text,
                    );
                }
            },
        );
    }

    /// The style whose text shadows what `key` paints casts.
    fn shadow_styles(&self, key: u64, first: bool) -> Option<S> {
        self.styles(key, first)
    }

    /// How many text shadows `item` casts, where it casts any.
    fn shadows_of(&self, item: &Paint<'_>, first: bool) -> Option<usize> {
        let key = match item {
            Paint::DecorationBeforeText(bar) | Paint::DecorationAfterText(bar) => bar.key(),
            Paint::Annotation(run) | Paint::Text(run) | Paint::Generated(run) => run.key(),
            Paint::Emphasis(mark) => mark.key,
            _ => return None,
        };
        let styles = self.shadow_styles(key.0, first)?;
        Some(styles.text_shadow_count())
    }

    /// Draws what `draw` paints: in the foreground pass, where `pass` is
    /// `None`, in `own`; in a shadow pass, as the text shadow of that index
    /// in `styles` casts it, moved by its offset, in its color and blurred
    /// by its radius, where `styles` has that many.
    ///
    /// `draw` is handed the transform taking the content box's device pixels
    /// onto the scene, `placed` moved by the shadow, and the color to paint
    /// in where it is not its own; a shadow's `currentcolor` is the style's own color, as the
    /// text's is. `inline` is where along the line what it
    /// paints starts and ends, which a blurred shadow is drawn around.
    #[allow(clippy::too_many_arguments)]
    fn shadowed<K: TextSink>(
        &self,
        sink: &mut K,
        pass: Option<usize>,
        styles: Option<&S>,
        placed: Affine,
        inline: (f64, f64),
        own: Option<Color>,
        mut draw: impl FnMut(&mut K, Affine, Option<Color>),
    ) {
        let Some(index) = pass else {
            draw(sink, placed, own);
            return;
        };
        let Some(shadow) = styles.and_then(|styles| styles.text_shadow().nth(index)) else {
            return;
        };
        let color = shadow.color;
        if color.components[3] <= 0.0 {
            return;
        }
        let offset = Vec2::new(f64::from(shadow.x), f64::from(shadow.y)) * self.scale;
        let moved = placed * Affine::translate(offset);
        // A blur radius is twice the deviation of its Gaussian.
        let deviation = f64::from(shadow.blur) * self.scale / 2.0;
        if deviation > 0.0 {
            // Glyphs reach past the line box by up to about its height, and
            // the blur past them.
            let (frame, height) = self.line.get();
            let reach = 3.0 * deviation + height;
            let clip = frame
                .transform_rect_bbox(Rect::new(inline.0, 0.0, inline.1, height))
                .inflate(reach, reach);
            sink.push_blur(moved, clip, deviation as f32);
            draw(sink, moved, Some(color));
            sink.pop_blur();
        } else {
            draw(sink, moved, Some(color));
        }
    }

    /// Draws the glyphs of a run of text, in its node's color.
    ///
    /// A glyph standing upright in a vertical line, or combined into one em,
    /// is not turned with the line: only the point it is drawn at is.
    fn draw_run(
        &self,
        sink: &mut impl TextSink,
        frame: Affine,
        run: &TextRun<'_>,
        first: bool,
        pass: Option<usize>,
    ) {
        let Some(font) = run.font() else {
            return;
        };
        let key = run.key().0;
        let color = self.color(key, first);
        let upright = matches!(
            run.orientation(),
            RunOrientation::Upright | RunOrientation::Combined
        );
        // Combined text squeezed into its em: its places already are, and
        // each glyph is drawn that much narrower about its own origin.
        let squeeze = run.combine_scale();
        let mut glyph_transform =
            (squeeze < 1.0).then(|| Affine::scale_non_uniform(f64::from(squeeze), 1.0));
        if let Some(skew) = font.skew {
            // CSS's clockwise degrees lean the glyph's top to the right.
            let lean = Affine::skew(-f64::from(skew.to_radians().tan()), 0.0);
            glyph_transform = Some(glyph_transform.map_or(lean, |squeeze| squeeze * lean));
        }
        let glyphs = || {
            run.glyphs().map(move |glyph| {
                if upright {
                    let at = frame * Point::new(f64::from(glyph.x), f64::from(glyph.y));
                    Glyph {
                        id: glyph.id,
                        x: at.x as f32,
                        y: at.y as f32,
                    }
                } else {
                    Glyph {
                        id: glyph.id,
                        x: glyph.x,
                        y: glyph.y,
                    }
                }
            })
        };
        let data = font_data(&font);
        let inline = if pass.is_some() {
            run.glyphs()
                .fold((f64::MAX, f64::MIN), |(start, end), glyph| {
                    let x = f64::from(glyph.x);
                    (start.min(x), end.max(x))
                })
        } else {
            (0.0, 0.0)
        };
        self.shadowed(
            sink,
            pass,
            self.styles(key, first).as_ref(),
            self.transform(key),
            inline,
            Some(color),
            |sink, placed, color| {
                // Transparent text draws nothing, colour glyphs included, as
                // Chrome takes a colour glyph's alpha from its paint's. Its
                // shadows still draw.
                let color = color.unwrap_or(Color::BLACK);
                if color.components[3] <= 0.0 {
                    return;
                }
                sink.glyphs(
                    &GlyphRun {
                        font: &data,
                        size: font.size,
                        hint: true,
                        coords: font.coords,
                        embolden: embolden(&font),
                        color,
                        transform: if upright { placed } else { placed * frame },
                        glyph_transform,
                    },
                    glyphs(),
                );
            },
        );
    }

    /// Draws an emphasis mark where winkin put it: the mark string laid out
    /// in the fonts of the text it marks, at the marks' size, its advance
    /// centred on the mark's middle and standing on its baseline.
    ///
    /// Upright in a vertical line, the mark is drawn unturned, its em box
    /// centred on the point winkin gives.
    fn draw_emphasis(
        &self,
        sink: &mut impl TextSink,
        frame: Affine,
        mark: &EmphasisMark,
        run: Option<&TextRun<'_>>,
        first: bool,
        pass: Option<usize>,
    ) {
        let key = mark.key.0;
        let Some(styles) = self.styles(key, first) else {
            return;
        };
        // The marks are laid out per element: a text node's is its parent's.
        let element = self.host.element(NodeKey(key));
        let Some(layout) = element.and_then(|element| self.text.emphasis_mark(element.0)) else {
            return;
        };
        let Some(line) = layout.lines().next() else {
            return;
        };
        let metrics = line.metrics();
        // Laid out at the size of the marks off the first line, which may
        // differ on it.
        let built = line
            .paints(|_| Decorates::None)
            .find_map(|item| match item {
                Paint::Text(run) => run.font().map(|font| font.size),
                _ => None,
            })
            .unwrap_or(mark.size);
        let ratio = if built > 0.0 { mark.size / built } else { 1.0 };
        // `currentcolor` is the text's own, which on the first line is its `::first-line`
        // color: `styles` is the style there.
        let color = styles.text_emphasis_color();
        // A mark on combined text turns with the line, as Chrome draws it: the unit is set
        // upright, but its mark is the line's.
        let upright = run.is_some_and(|run| run.orientation() == RunOrientation::Upright);
        let baseline = metrics.baseline - metrics.top;
        let half = metrics.width / 2.0;
        // Where the mark's own line's origin goes, and what it is drawn
        // with: along the line's frame, or unturned about the point.
        let (origin, frame_after) = if upright {
            let center = frame * Point::new(f64::from(mark.x), f64::from(mark.baseline));
            let middle = f64::from((metrics.ascent - metrics.descent) / 2.0 * ratio);
            (Point::new(center.x, center.y + middle), Affine::IDENTITY)
        } else {
            (
                Point::new(f64::from(mark.x), f64::from(mark.baseline)),
                frame,
            )
        };
        let place = move |x: f32, y: f32| Glyph {
            id: 0,
            x: (origin.x + f64::from((x - half) * ratio)) as f32,
            y: (origin.y + f64::from((y - baseline) * ratio)) as f32,
        };
        self.shadowed(
            sink,
            pass,
            Some(&styles),
            self.transform(key),
            (f64::from(mark.x - mark.size), f64::from(mark.x + mark.size)),
            Some(color),
            |sink, placed, color| {
                for item in line.paints(|_| Decorates::None) {
                    let Paint::Text(mark_run) = item else {
                        continue;
                    };
                    let Some(font) = mark_run.font() else {
                        continue;
                    };
                    sink.glyphs(
                        &GlyphRun {
                            font: &font_data(&font),
                            size: font.size * ratio,
                            hint: true,
                            coords: font.coords,
                            embolden: embolden(&font),
                            color: color.unwrap_or(Color::BLACK),
                            transform: placed * frame_after,
                            glyph_transform: None,
                        },
                        mark_run.glyphs().map(|glyph| Glyph {
                            id: glyph.id,
                            ..place(glyph.x, glyph.y)
                        }),
                    );
                }
            },
        );
    }
}

/// A box's corners in a line's own terms: at its line-left end on its over
/// side, at its line-right end on its over side, then under it at its
/// line-right and line-left ends.
fn line_corners(writing_mode: WritingMode, radii: Corners<CornerRadius>) -> [CornerRadius; 4] {
    let (top_left, top_right, bottom_right, bottom_left) = (
        radii.top_left,
        radii.top_right,
        radii.bottom_right,
        radii.bottom_left,
    );
    match writing_mode {
        WritingMode::HorizontalTb => [top_left, top_right, bottom_right, bottom_left],
        // Lines down the page, over on their right.
        WritingMode::VerticalRl | WritingMode::VerticalLr | WritingMode::SidewaysRl => {
            [top_right, bottom_right, bottom_left, top_left]
        }
        // Lines up the page, over on their left.
        WritingMode::SidewaysLr => [bottom_left, top_left, top_right, bottom_right],
    }
}

/// A run's font as the renderer draws it, sharing its bytes and their id so
/// the renderer's caches key it as fontwich does.
fn font_data(font: &FontInstance<'_>) -> FontData {
    FontData::new(
        Blob::from_raw_parts(font.bytes.arc().clone(), font.bytes.id()),
        font.index,
    )
}

/// How far a glyph's outline is grown where its family has no bold and it
/// is emboldened: Skia's fake bold, a 24th of the size at 9 pixels to a 32nd
/// at 36, half each side.
fn embolden(font: &FontInstance<'_>) -> Vec2 {
    if !font.embolden {
        return Vec2::ZERO;
    }
    let size = f64::from(font.size);
    let ratio = {
        let t = ((size - 9.0) / 27.0).clamp(0.0, 1.0);
        (1.0 / 24.0) * (1.0 - t) + (1.0 / 32.0) * t
    };
    let extra = size * ratio / 2.0;
    Vec2::new(extra, extra)
}

/// Paints an inline formatting context laid out by winkin.
pub(crate) fn paint_inline_layout<S: TextStyle>(
    sink: &mut impl TextSink,
    text_layout: &TextLayout<S>,
    host: &impl PaintHost<S>,
    scratch: &RefCell<PaintScratch>,
    frame: &PaintFrame,
) {
    let Some(layout) = text_layout.layout() else {
        return;
    };
    paint(
        sink,
        text_layout,
        layout,
        host,
        frame.transform,
        frame.content_size.width,
        frame.content_size.height,
        frame.scale,
        frame.root,
        scratch,
        frame.selection,
    );
}

/// Paints the text of an `<input>` or `<textarea>`, and its selection and caret where it is
/// focussed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_text_input<S: TextStyle>(
    sink: &mut impl TextSink,
    editor: &TextEditor<S>,
    host: &impl PaintHost<S>,
    scratch: &RefCell<PaintScratch>,
    focussed: bool,
    caret_color: Color,
    transform: Affine,
    scale: f64,
    key: NodeKey,
) {
    let text_layout = editor.text_layout();
    let Some(layout) = text_layout.layout() else {
        return;
    };
    let size = editor
        .metrics()
        .map(|metrics| metrics.size)
        .unwrap_or_default();
    let selection = editor.layout_selection_range();
    let selection = (focussed && !selection.is_empty()).then_some((selection.start, selection.end));
    paint(
        sink,
        text_layout,
        layout,
        host,
        transform,
        size.width,
        size.height,
        scale,
        key,
        scratch,
        selection,
    );
    if focussed
        && editor.is_selection_collapsed()
        && let Some(caret) = editor.caret_rect()
    {
        sink.fill(transform, caret, caret_color);
    }
}

/// Paints a list item's outside marker, right-aligned before its border box, on the baseline of
/// the item's first line of text.
pub(crate) fn paint_marker<S: TextStyle>(
    sink: &mut impl TextSink,
    marker: &Marker,
    marker_layout: &TextLayout<S>,
    text_layout: Option<&TextLayout<S>>,
    host: &impl PaintHost<S>,
    scratch: &RefCell<PaintScratch>,
    frame: &MarkerFrame,
) {
    let Some(layout) = marker_layout.layout() else {
        return;
    };
    let Some(line) = layout.lines().next() else {
        return;
    };
    let metrics = line.metrics();
    let marker_scale = marker_layout.scale();
    // A bullet is right-aligned and padded.
    let x_padding = match marker {
        Marker::Char(_) => 8.0,
        Marker::String(_) => 0.0,
    };
    // An outside marker is placed outside the list item's border box, from the origin of its
    // content box.
    let x_offset = -((metrics.left + metrics.width) / marker_scale
        + x_padding
        + frame.padding_left
        + frame.border_left);
    // The marker sits on the baseline of the first line of text in the list item.
    let y_offset = text_layout
        .and_then(|text_layout| {
            let first = text_layout.layout()?.lines().next()?.metrics();
            Some((first.baseline - metrics.baseline) / marker_scale + text_layout.block_offset())
        })
        .unwrap_or(0.0);
    let pos = Point {
        x: frame.origin.x + f64::from(x_offset),
        y: frame.origin.y + f64::from(y_offset),
    };
    let scale = frame.scale;
    let transform = frame.transform * Affine::translate((pos.x * scale, pos.y * scale));
    paint(
        sink,
        marker_layout,
        layout,
        host,
        transform,
        0.0,
        0.0,
        scale,
        frame.key,
        scratch,
        None,
    );
}

/// One bar of a decoration on a line of winkin-laid text, and the font
/// geometry it is drawn with, in device pixels along and across its line.
#[derive(Copy, Clone)]
struct WinkinDecoration {
    /// Where it starts and ends along the line.
    left: f64,
    right: f64,
    /// Where its box's bars on the line start and end, which
    /// `text-decoration-inset` is taken from.
    span: (f64, f64),
    /// Its box's baseline, and its primary font's ascent and descent about it.
    baseline: f32,
    ascent: f32,
    descent: f32,
    /// Where an `auto` underline's top is under the baseline: Chrome's, as winkin works it out.
    underline_gap: f32,
    /// The font's own underline: where its top is under the baseline, and
    /// how thick it is, for `from-font`. Each is the `auto` one where the
    /// font gives none.
    font_underline_gap: f32,
    font_thickness: f32,
    /// Its box's computed font size, scaled where `text-fit` scales its line.
    font_size: f32,
    /// Whether the metrics are measured about a central baseline, as a
    /// vertical line's are.
    centered: bool,
}

/// Paints one bar of `deco`: its underline and overline where `after_text` is false, and its line
/// through where it is true.
fn draw_bar(
    sink: &mut impl TextSink,
    scratch: &mut PaintScratch,
    transform: Affine,
    scale: f64,
    deco: ResolvedDecoration,
    bar: WinkinDecoration,
    after_text: bool,
) {
    // An `auto` underline's gap is half its thickness, rounded up and at
    // least a pixel, as Chrome's `ComputeUnderlineOffsetAuto` takes it:
    // the thickness drawn, not the `auto` one, where the two differ.
    let underline_offset = if deco.underline_from_font {
        -bar.font_underline_gap
    } else if matches!(deco.thickness, TextDecorationThickness::Auto) {
        -bar.underline_gap
    } else {
        let size = decoration_size(
            &deco.thickness,
            bar.font_thickness,
            f64::from(bar.font_size) / scale,
            scale,
        );
        -(size / 2.0).ceil().max(1.0)
    };
    scratch.deco_boxes.clear();
    scratch.deco_boxes.push(LineDecoration {
        key: NodeKey(0),
        deco,
        min_x: bar.span.0,
        max_x: bar.span.1,
        piece: Some((bar.left, bar.right)),
        own: Some(DecorationRunGeometry {
            baseline: bar.baseline,
            ascent: bar.ascent,
            descent: bar.descent,
            // Placed from the baseline, up: an underline's top is its gap
            // under it. `from-font` takes the font's own place.
            underline_offset,
            // Read only for `from-font` thickness: `auto` and lengths are
            // worked out from the font size.
            underline_size: bar.font_thickness,
            strikethrough_size: bar.font_thickness,
            // winkin's ascent is already the one the platform's browser
            // draws an overline against: its line metrics are.
            font: None,
            font_size: bar.font_size,
            css_font_size: f64::from(bar.font_size) / scale,
            centered: bar.centered,
        }),
        first: None,
    });
    let drawn = if after_text {
        TextDecorationLine::LINE_THROUGH
    } else {
        TextDecorationLine::UNDERLINE | TextDecorationLine::OVERLINE
    };
    flush_line_decorations(
        sink,
        transform,
        scale,
        &scratch.deco_boxes,
        &mut scratch.win_ascent_ratios,
        NodeKey(0),
        bar.baseline,
        drawn,
    );
    scratch.deco_boxes.clear();
}
