//! The Parley text backend for Blitz: inline formatting contexts, form controls and list
//! markers laid out by [Parley](https://github.com/linebender/parley), behind
//! [`blitz_text`]'s traits.

mod build;
mod editor;
mod fonts;
mod layout;
mod paint;
mod style;

#[cfg(feature = "parallel")]
use std::cell::RefCell;
use std::ops::Range;
use std::sync::{Arc, Mutex};

use blitz_text::{
    Backend, ContentItem, ContentWidths, FloatSide, InlineLayout, InlineSource, InlineText,
    LastBaseline, LineArea, LineExclusions, LineFlow, LinesExtent, LinesInputs, NodeKey,
    PaintFrame, PaintHost, PaintScratch, Placement, TextHit, TextSink, TextStyle,
};
use kurbo::{Rect, Vec2};
use parley::{
    Affinity, BreakReason, Cluster, ClusterSide, Cursor, LayoutContext, PositionedLayoutItem, Run,
    Selection,
};
#[cfg(feature = "parallel")]
use thread_local::ThreadLocal;

pub use ::parley;
pub use editor::TextEditor;
pub use fonts::FontContext;

/// The Parley backend.
pub struct Parley;

impl<S: TextStyle + 'static> Backend<S> for Parley {
    const NAME: &'static str = "parley";
    type Fonts = FontContext;
    type Context = TextContext;
    type Layout = TextLayout;
    type Editor = TextEditor;
    type Marker = MarkerLayout;
}

/// What a run of text carries through Parley: the node its text is in.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TextBrush {
    /// The node of the run's innermost element, or of the inline root.
    pub key: NodeKey,
}

impl TextBrush {
    pub(crate) fn from_key(key: NodeKey) -> Self {
        Self { key }
    }
}

/// An outside list marker, laid out by Parley.
#[derive(Clone)]
pub struct MarkerLayout {
    /// The marker's layout, which the renderer paints.
    pub layout: parley::Layout<TextBrush>,
}

/// What a document keeps for its text under Parley: its fonts, and Parley's contexts.
pub struct TextContext {
    /// The fonts.
    pub font_ctx: Arc<Mutex<parley::FontContext>>,
    /// Each construction thread's copy of the fonts.
    #[cfg(feature = "parallel")]
    pub(crate) thread_font_contexts: ThreadLocal<RefCell<Box<parley::FontContext>>>,
    /// Parley's layout context.
    pub layout_ctx: LayoutContext<TextBrush>,
}

#[cfg(feature = "parallel")]
thread_local! {
    static LAYOUT_CTX: RefCell<Option<Box<LayoutContext<TextBrush>>>> = const { RefCell::new(None) };
}

/// An inline formatting context laid out by Parley.
#[derive(Clone, Default)]
pub struct TextLayout {
    pub(crate) text: String,
    /// Parley's layout of the text and inline boxes, which the renderer paints.
    pub layout: parley::Layout<TextBrush>,
    /// How far `align-content` moves the line boxes down the content box, in CSS pixels.
    pub(crate) block_offset: f32,
    /// The floats, as the lines place them: each one's key, side and margin box's size in device
    /// pixels.
    floats: Vec<(u64, FloatSide, f32, f32)>,
}

impl std::fmt::Debug for TextLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TextLayout")
    }
}

impl InlineText for TextLayout {
    #[inline]
    fn scale(&self) -> f32 {
        self.layout.scale()
    }

    #[inline]
    fn block_offset(&self) -> f32 {
        self.block_offset
    }

    #[inline]
    fn text(&self) -> &str {
        &self.text
    }

    #[inline]
    fn text_len(&self) -> usize {
        self.text.len()
    }

    #[inline]
    fn selected_text(&self, start: usize, end: usize) -> impl Iterator<Item = &str> {
        self.text.get(start..end).into_iter()
    }

    fn logical_content(&self) -> impl Iterator<Item = ContentItem> {
        let mut lines = self.layout.lines();
        let mut boxes = self.layout.inline_boxes().peekable();
        // The current line's runs, sorted from visual into logical order for bidi text
        let mut runs: Vec<Run<'_, TextBrush>> = Vec::new();
        let mut next_run = 0;
        let mut clusters = None;
        // The node of the last cluster's style, which the clusters after it mostly share
        let mut style = None;
        // A cluster read but not yet taken in, which waits for the items before it
        let mut held: Option<(NodeKey, Range<usize>)> = None;
        // The text taken in since the last item, all of one node
        let mut pending: Option<(NodeKey, Range<usize>)> = None;
        let text = |(key, range)| ContentItem::Text { key, range };
        std::iter::from_fn(move || {
            loop {
                let (key, range) = match held.take() {
                    Some(cluster) => cluster,
                    None => {
                        let Some(cluster) = clusters.as_mut().and_then(Iterator::next) else {
                            while next_run >= runs.len() {
                                let Some(line) = lines.next() else {
                                    // The text read last, then the boxes after the last line
                                    if let Some(read) = pending.take() {
                                        return Some(text(read));
                                    }
                                    let ibox = boxes.next()?;
                                    return Some(ContentItem::Box(NodeKey(ibox.id)));
                                };
                                runs.clear();
                                runs.extend(line.runs());
                                runs.sort_by_key(|run| run.text_range().start);
                                next_run = 0;
                            }
                            clusters = runs.get(next_run).map(Run::clusters);
                            next_run += 1;
                            continue;
                        };
                        let index = cluster.style_index();
                        let key = match style {
                            Some((styled, key)) if styled == index => key,
                            _ => {
                                let key = cluster.style().brush.key;
                                style = Some((index, key));
                                key
                            }
                        };
                        (key, cluster.text_range())
                    }
                };
                // The boxes before the cluster come first, after the text read before them
                if boxes.peek().is_some_and(|ibox| ibox.index <= range.start) {
                    held = Some((key, range));
                    if let Some(read) = pending.take() {
                        return Some(text(read));
                    }
                    if let Some(ibox) = boxes.next() {
                        return Some(ContentItem::Box(NodeKey(ibox.id)));
                    }
                    continue;
                }
                match &mut pending {
                    Some((read_key, read)) if *read_key == key && read.end == range.start => {
                        read.end = range.end;
                    }
                    _ => {
                        if let Some(read) = pending.replace((key, range)) {
                            return Some(text(read));
                        }
                    }
                }
            }
        })
    }

    #[inline]
    fn source_offset(&self, key: NodeKey, offset: usize) -> Option<usize> {
        let _ = (key, offset);
        None
    }

    #[inline]
    fn maps_source(&self) -> bool {
        false
    }

    fn first_baseline(&self) -> Option<f32> {
        let line = self.layout.lines().next()?;
        Some(line.metrics().baseline / self.layout.scale() + self.block_offset)
    }

    fn hit_test(&self, x: f32, y: f32, exact: bool) -> Option<TextHit> {
        let layout = &self.layout;
        let point = (x * layout.scale(), (y - self.block_offset) * layout.scale());
        let (cluster, side) = if exact {
            Cluster::from_point_exact(layout, point.0, point.1)?
        } else {
            Cluster::from_point(layout, point.0, point.1)?
        };
        let leading = side == ClusterSide::Left;
        let offset = if cluster.is_rtl() {
            if leading {
                cluster.text_range().end
            } else {
                cluster.text_range().start
            }
        } else if leading || cluster.is_line_break() == Some(BreakReason::Explicit) {
            cluster.text_range().start
        } else {
            cluster.text_range().end
        };
        Some(TextHit {
            key: cluster.style().brush.key,
            offset,
        })
    }

    fn for_each_selection_rect(&self, start: usize, end: usize, mut f: impl FnMut(Rect)) {
        let layout = &self.layout;
        let anchor = Cursor::from_byte_index(layout, start, Affinity::Downstream);
        let focus = Cursor::from_byte_index(layout, end, Affinity::Downstream);
        let selection = Selection::new(anchor, focus);
        selection.geometry_with(layout, |rect, _| {
            f(Rect::new(rect.x0, rect.y0, rect.x1, rect.y1));
        });
    }

    fn fragment_rects(
        &self,
        _key: NodeKey,
        origin: (f32, f32),
        _content: kurbo::Size,
        _shift: Vec2,
        contains: impl Fn(NodeKey) -> bool,
    ) -> impl Iterator<Item = Rect> {
        let layout = &self.layout;
        let scale = layout.scale();
        let origin_x = origin.0;
        let origin_y = origin.1 + self.block_offset;

        fn union(acc: &mut Option<[f32; 4]>, left: f32, top: f32, right: f32, bottom: f32) {
            *acc = Some(match *acc {
                Some([l, t, r, b]) => [l.min(left), t.min(top), r.max(right), b.max(bottom)],
                None => [left, top, right, bottom],
            });
        }

        // One rect a line box: the union of the node's fragments on that line
        layout.lines().filter_map(move |line| {
            let line_metrics = line.metrics();
            let mut line_rect: Option<[f32; 4]> = None;

            for item in line.items() {
                match item {
                    PositionedLayoutItem::GlyphRun(glyph_run) => {
                        if !contains(glyph_run.style().brush.key) {
                            continue;
                        }
                        let x0 = glyph_run.offset();
                        let x1 = x0 + glyph_run.advance();
                        // The line box's block extent rather than the run's font ascent and
                        // descent: fonts with small typographic metrics would otherwise give
                        // rects that clip the drawn glyphs. This matches the geometry of the
                        // selection highlight.
                        let y0 = line_metrics.block_min_coord;
                        let y1 = line_metrics.block_max_coord;
                        union(&mut line_rect, x0, y0, x1, y1);
                    }
                    PositionedLayoutItem::InlineBox(inline_box) => {
                        if !contains(NodeKey(inline_box.id)) {
                            continue;
                        }
                        let x0 = inline_box.x;
                        let y0 = inline_box.y;
                        union(
                            &mut line_rect,
                            x0,
                            y0,
                            x0 + inline_box.width,
                            y0 + inline_box.height,
                        );
                    }
                }
            }

            line_rect.map(|[left, top, right, bottom]| {
                Rect::new(
                    f64::from(origin_x + left / scale),
                    f64::from(origin_y + top / scale),
                    f64::from(origin_x + right / scale),
                    f64::from(origin_y + bottom / scale),
                )
            })
        })
    }

    fn debug_print(&self) {
        println!("Size: {}x{}", self.layout.width(), self.layout.height());
        println!("Text content: {:?}", self.text);
        println!("Inline Boxes:");
        for ibox in self.layout.inline_boxes() {
            print!("(id: {}) ", ibox.id);
        }
        println!();
        println!("Lines:");
        for (i, line) in self.layout.lines().enumerate() {
            let metrics = line.metrics();
            let x = metrics.inline_min_coord;
            let y = metrics.block_min_coord;
            let w = metrics.inline_max_coord - metrics.inline_min_coord;
            let h = metrics.block_max_coord - metrics.block_min_coord;
            println!("Line {i}: x:{x} y:{y} width:{w} height:{h}");
            for item in line.items() {
                print!("  ");
                match item {
                    PositionedLayoutItem::GlyphRun(run) => {
                        print!(
                            "RUN (x: {}, w: {}) ",
                            run.offset().round(),
                            run.run().advance()
                        )
                    }
                    PositionedLayoutItem::InlineBox(ibox) => print!(
                        "BOX {:?} (id: {} x: {} y: {} w: {}, h: {})",
                        ibox.kind,
                        ibox.id,
                        ibox.x.round(),
                        ibox.y.round(),
                        ibox.width.round(),
                        ibox.height.round()
                    ),
                }
                println!();
            }
        }
    }
}

impl<S: TextStyle> InlineLayout<S> for TextLayout {
    type Context = TextContext;

    const SETS_WRITING_MODES: bool = false;
    const READS_ROOM_ABOVE: bool = false;
    const SHIFTS_RELATIVE_INLINES: bool = false;

    fn build_layouts<C: InlineSource<S>>(
        cx: &mut TextContext,
        source: &C,
        scale: f32,
        layouts: &mut [(NodeKey, Box<Self>)],
    ) {
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            let cx = &*cx;
            layouts.par_iter_mut().for_each(|(key, layout)| {
                let mut layout_ctx = LAYOUT_CTX
                    .take()
                    .unwrap_or_else(|| Box::new(LayoutContext::new()));
                let mut font_ctx = cx
                    .thread_font_contexts
                    .get_or(|| RefCell::new(Box::new(cx.font_ctx.lock().unwrap().clone())))
                    .borrow_mut();
                build::build_inline_layout_into(
                    source,
                    &mut layout_ctx,
                    &mut font_ctx,
                    layout,
                    scale,
                    *key,
                );
                LAYOUT_CTX.set(Some(layout_ctx));
            });
        }
        #[cfg(not(feature = "parallel"))]
        for (key, layout) in layouts {
            build::build_inline_layout_into(
                source,
                &mut cx.layout_ctx,
                &mut cx.font_ctx.lock().unwrap(),
                layout,
                scale,
                *key,
            );
        }
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.holds_nothing()
    }

    fn prepare<C: InlineSource<S>>(
        &mut self,
        _cx: &mut TextContext,
        source: &C,
        root: NodeKey,
        _scale: f32,
        lines: LinesInputs<'_>,
    ) {
        let style = source.style(root);
        self.prepare_lines(lines.sizes, style.as_ref());
    }

    #[inline]
    fn content_widths(&mut self) -> ContentWidths {
        self.widths()
    }

    fn break_lines(
        &mut self,
        _cx: &mut TextContext,
        area: LineArea,
        style: Option<&S>,
        exclusions: &mut impl LineExclusions,
    ) {
        self.break_into_lines(area.width, style, exclusions);
    }

    #[inline]
    fn extent(&self, _end_padding: f32) -> LinesExtent {
        self.lines_extent()
    }

    #[inline]
    fn set_frame(&mut self, block_offset: f32, _content: blitz_text::Size) {
        self.block_offset = block_offset;
    }

    fn placements(&self) -> impl Iterator<Item = Placement> {
        self.box_placements()
    }

    fn last_line_baseline(&self) -> LastBaseline {
        match self.lines_extent().last_baseline {
            Some(baseline) => LastBaseline::At(baseline),
            None => LastBaseline::None,
        }
    }

    #[inline]
    fn line_flow(&self) -> LineFlow {
        LineFlow::Horizontal
    }

    #[inline]
    fn place_on_page(&self, along: [f32; 2], across: [f32; 2]) -> (f32, f32) {
        (along[0], across[0])
    }

    #[inline]
    fn room_below(&self) -> f32 {
        0.0
    }

    #[inline]
    fn float_node(key: u64) -> Option<NodeKey> {
        Some(NodeKey(key))
    }

    fn paint<K: TextSink, H: PaintHost<S>>(
        &self,
        sink: &mut K,
        host: &H,
        scratch: &std::cell::RefCell<PaintScratch>,
        frame: &PaintFrame,
    ) {
        paint::paint_inline_layout(sink, self, host, scratch, frame);
    }
}
