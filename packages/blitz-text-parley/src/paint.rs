//! Painting text laid out by Parley into a [`TextSink`].

use core::cell::RefCell;

use blitz_text::paint::{
    DecorationRunGeometry, DecorationStackEntry, Glyph, GlyphRun, LineDecoration, PaintFrame,
    PaintHost, PaintScratch, ResolvedDecoration, TextSink, composition_underline,
    flush_line_decorations,
};
use blitz_text::style::{Color, TextDecorationLine, TextStyle};
use blitz_text::{InlineText as _, Marker, MarkerFrame, NodeKey};
use kurbo::{Affine, Point, Rect};
use parley::{Line, PositionedLayoutItem};

use crate::{MarkerLayout, TextBrush, TextEditor, TextLayout};

/// The values read for an element on the path from the inline root to a run.
fn decoration_entry<S: TextStyle>(host: &impl PaintHost<S>, key: NodeKey) -> DecorationStackEntry {
    let Some(style) = host.style(key) else {
        return DecorationStackEntry {
            key,
            text_color: Color::BLACK,
            decoration: None,
        };
    };
    DecorationStackEntry {
        key,
        text_color: style.color(),
        decoration: ResolvedDecoration::of(&style),
    }
}

/// Paints an inline formatting context: its inline elements' backgrounds, the selection
/// highlight, and its text and decorations.
///
/// Parley's painter leaves atomic inlines to be painted after the text.
pub(crate) fn paint_inline_layout<S: TextStyle>(
    sink: &mut impl TextSink,
    text_layout: &TextLayout,
    host: &impl PaintHost<S>,
    scratch: &RefCell<PaintScratch>,
    frame: &PaintFrame,
) {
    let scratch = &mut *scratch.borrow_mut();
    let transform = frame.transform;
    // The inline elements' backgrounds, under the text and the selection highlight.
    draw_inline_backgrounds(
        sink,
        text_layout.layout.lines(),
        host,
        transform,
        frame.root,
    );

    if let Some((start, end)) = frame.selection {
        text_layout.for_each_selection_rect(start, end, |rect| {
            sink.selection(transform, rect);
        });
    }

    stroke_text(
        sink,
        text_layout.layout.lines(),
        host,
        transform,
        frame.scale,
        frame.root,
        scratch,
    );
}

/// Paints the text of an `<input>` or `<textarea>`, and its selection and caret where it is
/// focussed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_text_input<S: TextStyle>(
    sink: &mut impl TextSink,
    editor: &TextEditor,
    host: &impl PaintHost<S>,
    scratch: &RefCell<PaintScratch>,
    focussed: bool,
    caret_color: Color,
    transform: Affine,
    scale: f64,
    key: NodeKey,
) {
    let scratch = &mut *scratch.borrow_mut();
    let editor = editor.plain_editor();
    if focussed {
        for (rect, _line) in editor.selection_geometry().iter() {
            sink.selection(transform, convert_rect(rect));
        }
        if let Some(cursor) = editor.cursor_geometry(1.5) {
            sink.fill(transform, convert_rect(&cursor), caret_color);
        };
    }

    let Some(layout) = editor.try_layout() else {
        return;
    };
    stroke_text(sink, layout.lines(), host, transform, scale, key, scratch);
}

/// Paints a list item's outside marker, right-aligned before its border box, on the baseline of
/// the item's first line of text.
pub(crate) fn paint_marker<S: TextStyle>(
    sink: &mut impl TextSink,
    marker: &Marker,
    layout: &MarkerLayout,
    text_layout: Option<&TextLayout>,
    host: &impl PaintHost<S>,
    scratch: &RefCell<PaintScratch>,
    frame: &MarkerFrame,
) {
    let scratch = &mut *scratch.borrow_mut();
    let layout = &layout.layout;
    // A bullet is right-aligned and padded.
    let x_padding = match marker {
        Marker::Char(_) => 8.0,
        Marker::String(_) => 0.0,
    };
    // An outside marker is placed outside the list item's border box, from the origin of its
    // content box.
    let x_offset = -(layout.full_width() / layout.scale()
        + x_padding
        + frame.padding_left
        + frame.border_left);

    // The marker sits on the baseline of the first line of text in the list item.
    let y_offset = text_layout
        .and_then(|text_layout| text_layout.first_baseline())
        .zip(layout.lines().next())
        .map_or(0.0, |(baseline, marker_line)| {
            baseline - marker_line.metrics().baseline / layout.scale()
        });

    let pos = Point {
        x: frame.origin.x + x_offset as f64,
        y: frame.origin.y + y_offset as f64,
    };
    let scale = frame.scale;
    let transform = frame.transform * Affine::translate((pos.x * scale, pos.y * scale));

    stroke_text(
        sink,
        layout.lines(),
        host,
        transform,
        scale,
        frame.key,
        scratch,
    );
}

/// A Parley bounding box as a rectangle.
fn convert_rect(rect: &parley::BoundingBox) -> Rect {
    Rect::new(rect.x0, rect.y0, rect.x1, rect.y1)
}

/// Draws the backgrounds of inline elements (`<span style="background: ...">`).
///
/// Each glyph run carries the node of the innermost inline element it is in, whose
/// `background-color` fills a rectangle under the run's advance and its font's ascent and
/// descent, behind the text.
///
/// The inline root's own background is its box's, so runs of the root are skipped.
fn draw_inline_backgrounds<'a, S: TextStyle>(
    sink: &mut impl TextSink,
    lines: impl Iterator<Item = Line<'a, TextBrush>>,
    host: &impl PaintHost<S>,
    transform: Affine,
    inline_root: NodeKey,
) {
    for line in lines {
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };

            let key = glyph_run.style().brush.key;
            if key == inline_root {
                continue;
            }

            let Some(style) = host.style(key) else {
                continue;
            };

            let background = style.background_color();
            if background == Color::TRANSPARENT {
                continue;
            }

            let metrics = glyph_run.run().font_metrics();
            let x = glyph_run.offset() as f64;
            let w = glyph_run.advance() as f64;
            let baseline = glyph_run.baseline() as f64;
            let y0 = baseline - metrics.ascent as f64;
            let y1 = baseline + metrics.descent as f64;
            sink.fill(transform, Rect::new(x, y0, x + w, y1), background);
        }
    }
}

/// Draws the glyphs of `lines` and their decorations.
fn stroke_text<'a, S: TextStyle>(
    sink: &mut impl TextSink,
    lines: impl Iterator<Item = Line<'a, TextBrush>>,
    host: &impl PaintHost<S>,
    transform: Affine,
    scale: f64,
    inline_root: NodeKey,
    scratch: &mut PaintScratch,
) {
    let PaintScratch {
        stack,
        path,
        deco_boxes,
        win_ascent_ratios,
    } = scratch;
    stack.clear();
    path.clear();
    deco_boxes.clear();
    let emboldens = sink.emboldens();

    // A stack of the path from the inline root to the current run's node, as the runs are
    // walked. The `text-decoration-*` properties do not inherit: a decoration set on an
    // ancestor propagates to the text it holds. Each element's values are read once, as the
    // walk descends into it, and popped as it ascends. `path` is the run's path.
    for line in lines {
        // The decorations of this line, one for each decorating box, each painted once over the
        // runs it spans, in its own font, rather than one stepped piece a run.
        deco_boxes.clear();
        // The underline of the composing text on this line, over its runs.
        let mut composing: Option<LineDecoration> = None;

        for item in line.items() {
            if let PositionedLayoutItem::GlyphRun(glyph_run) = item {
                let run = glyph_run.run();
                let font = run.font();
                let font_size = run.font_size();
                let metrics = run.font_metrics();
                let style = glyph_run.style();
                let synthesis = run.synthesis();
                let glyph_xform = synthesis
                    .skew()
                    .map(|angle| Affine::skew(angle.to_radians().tan() as f64, 0.0));

                let css_font_size = font_size as f64 / scale;

                // The stack takes the run's path (inline root to run node): it keeps the shared
                // prefix, pops the rest, and reads the elements newly descended into.
                path.clear();
                let mut walk = Some(style.brush.key);
                while let Some(key) = walk {
                    path.push(key);
                    if key == inline_root {
                        break;
                    }
                    walk = host.parent(key);
                }
                path.reverse();

                let shared = stack
                    .iter()
                    .zip(path.iter())
                    .take_while(|(entry, key)| entry.key == **key)
                    .count();
                stack.truncate(shared);
                for &key in &path[shared..] {
                    stack.push(decoration_entry(host, key));
                }

                // The glyphs are in the color of the run's own node, the top of the stack:
                // `color` inherits, so the innermost element holds the right value.
                let text_color = stack.last().map(|e| e.text_color).unwrap_or(Color::BLACK);

                let embolden = if emboldens {
                    let fs = font_size as f64 / scale;
                    kurbo::Vec2::new((0.015125 * fs).min(0.3), (0.0121 * fs).min(0.3))
                } else {
                    kurbo::Vec2::default()
                };

                let normalized_coords: &[i16] = bytemuck::cast_slice(run.normalized_coords());
                sink.glyphs(
                    &GlyphRun {
                        font,
                        size: font_size,
                        hint: !emboldens,
                        coords: normalized_coords,
                        embolden,
                        color: text_color,
                        transform,
                        glyph_transform: glyph_xform,
                    },
                    glyph_run.positioned_glyphs().map(|glyph| Glyph {
                        id: glyph.id,
                        x: glyph.x,
                        y: glyph.y,
                    }),
                );

                // The run's part of each decorating box on its path. Each decoration is drawn
                // once for its box after the line is walked, so mixed font sizes in a box draw
                // one straight line rather than one stepped piece a run.
                let geometry = DecorationRunGeometry {
                    baseline: glyph_run.baseline(),
                    ascent: metrics.ascent,
                    descent: metrics.descent,
                    underline_offset: metrics.underline_offset,
                    underline_size: metrics.underline_size,
                    strikethrough_size: metrics.strikethrough_size,
                    font: Some(font.clone()),
                    font_size,
                    css_font_size,
                    centered: false,
                };
                let run_key = style.brush.key;
                let run_x0 = glyph_run.offset() as f64;
                let run_x1 = run_x0 + glyph_run.advance() as f64;

                for entry in stack.iter() {
                    let Some(decoration) = &entry.decoration else {
                        continue;
                    };
                    let idx = match deco_boxes.iter().position(|d| d.key == entry.key) {
                        Some(idx) => idx,
                        None => {
                            deco_boxes.push(LineDecoration {
                                key: entry.key,
                                deco: decoration.clone(),
                                min_x: f64::INFINITY,
                                max_x: f64::NEG_INFINITY,
                                piece: None,
                                own: None,
                                first: None,
                            });
                            deco_boxes.len() - 1
                        }
                    };
                    let acc = &mut deco_boxes[idx];
                    acc.min_x = acc.min_x.min(run_x0);
                    acc.max_x = acc.max_x.max(run_x1);
                    if acc.first.is_none() {
                        acc.first = Some(geometry.clone());
                    }
                    // A run whose innermost node is the box itself is the box's own text, whose
                    // font places and sizes the decoration.
                    if entry.key == run_key {
                        acc.own = Some(geometry.clone());
                    }
                }

                // A text input's editor underlines the text an input method is composing.
                if style.underline.is_some() {
                    let acc = composing.get_or_insert_with(|| LineDecoration {
                        key: run_key,
                        deco: composition_underline(text_color),
                        min_x: f64::INFINITY,
                        max_x: f64::NEG_INFINITY,
                        piece: None,
                        own: None,
                        first: None,
                    });
                    acc.min_x = acc.min_x.min(run_x0);
                    acc.max_x = acc.max_x.max(run_x1);
                    acc.first.get_or_insert(geometry);
                }
            }
        }

        flush_line_decorations(
            sink,
            transform,
            scale,
            deco_boxes,
            win_ascent_ratios,
            inline_root,
            line.metrics().baseline,
            TextDecorationLine::all(),
        );
        if let Some(composing) = composing.take() {
            flush_line_decorations(
                sink,
                transform,
                scale,
                std::slice::from_ref(&composing),
                win_ascent_ratios,
                inline_root,
                line.metrics().baseline,
                TextDecorationLine::all(),
            );
        }
    }
}
