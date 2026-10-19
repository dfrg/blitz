//! The winkin text backend for Blitz: inline formatting contexts, form controls and list
//! markers laid out by [winkin](https://github.com/dfrg/winkin), behind [`blitz_text`]'s traits.
//!
//! The content of an inline formatting context is built from what the host pushes the first time
//! it is laid out, and again whenever it changes or its atomic inlines measure differently:
//! winkin takes each atomic inline's size as it is pushed, and the host only knows it once it has
//! measured the box.

mod calc;
mod editor;
mod fonts;
mod paint;
mod style;
#[cfg(test)]
mod tests;

use blitz_text::style::{Clear, TextStyle};
use blitz_text::{
    BlockInputs, ContentItem, ContentWidths, FloatRequest, FontFeatureValues, InlineBuilder,
    InlineLayout, InlineSource, InlineText, LastBaseline, LineArea, LineExclusions, LineFlow,
    LinesExtent, LinesInputs, Marker, MarkerEngine, MarkerFrame, Measure, PaintFrame, PaintHost,
    PaintScratch, Placement, SpanKind, TextHit, TextSink,
};
use fontwich::{Collection, LayerBuilder};
use kurbo::Affine;
use winkin::style::{
    BaseDirection, BidiGroup, EdgesGroup, FontFamilyName, GenericFamily, Language, LineGroup,
    UnicodeBidi, VerticalAlign, WritingMode,
};
use winkin::{
    Area, BlockExtents, BoxSize, BuildOptions, ComputedBlockStyle, ComputedStyle, Context,
    Exclusions, ExclusionsCheckpoint, FloatSide, InlineExtents, IntrinsicSizes, Item, Layout,
    LayoutBuilder, LineMetrics, NodeKey, OriginalDisplay, PlacedFloat,
};

/// The fontwich crate, for the renderer and for hosts that build fonts.
pub use ::fontwich;
/// The winkin crate, for the renderer's winkin painter.
pub use ::winkin;
pub use editor::TextEditor;
pub use fonts::{FontContext, WinkinFontMetrics};

/// The winkin backend.
pub struct Winkin;

impl<S: TextStyle + 'static> blitz_text::Backend<S> for Winkin {
    const NAME: &'static str = "winkin";
    type Fonts = FontContext;
    type Context = TextContext;
    type Layout = TextLayout<S>;
    type Editor = TextEditor<S>;
    type Marker = TextLayout<S>;
}

/// An outside list marker, laid out by winkin.
pub type MarkerLayout<S> = TextLayout<S>;

/// The key of a block's `::first-letter` box, which has no node of its own:
/// the block's node with this bit set.
pub const FIRST_LETTER_KEY: u64 = 1 << 62;

/// The key of a block's inside list marker, which has no node of its own:
/// the block's node with this bit set.
pub const MARKER_KEY: u64 = 1 << 61;

/// The key of the box holding the text an input method is composing in an `<input>` or a
/// `<textarea>`, which has no node of its own: the control's node with this bit set.
pub const COMPOSE_KEY: u64 = 1 << 60;

/// The bits that number the pieces an `<input>` or a `<textarea>` lays its text out in.
///
/// The editor lays the text out in up to three pieces: before, inside and after the text an input
/// method is composing or a password field shows in the clear. Each piece is a text node keyed
/// by the control's node with its number in these bits, so that offsets in each map back to the
/// text. The box of a password's character shown in the clear is keyed by both bits.
pub const PIECE_KEYS: u64 = 3 << 58;

/// The number [`PIECE_KEYS`] gives the first piece of an editor's text.
const PIECE_SHIFT: u32 = 58;

/// The key of an anonymous ruby container or annotation, which CSS Ruby's box fixup makes and
/// which has no node.
pub const ANONYMOUS_KEY: u64 = 1 << 57;

/// Returns whether `key` names something the backend made rather than one of the host's nodes:
/// a first letter, an inside marker, the composing text, a piece of an editor's text or an
/// anonymous ruby box.
#[inline]
pub fn is_synthetic_key(key: u64) -> bool {
    key & (FIRST_LETTER_KEY | MARKER_KEY | COMPOSE_KEY | PIECE_KEYS | ANONYMOUS_KEY) != 0
}

/// What a document keeps for its text under winkin: the fonts winkin chooses from, and the
/// context it builds and breaks in.
///
/// The platform's fonts and the fonts Blitz ships are the collection's
/// lower layers; a document's `@font-face` fonts are a layer of their own
/// above them, which a face joins as it loads, the context being handed
/// the new collection.
pub struct TextContext {
    /// What every layout is broken with, and built with outside box
    /// construction.
    pub(crate) cx: Context,
    /// How many times the context's collection has changed, which a
    /// thread's context catches up with before it builds.
    pub(crate) generation: u64,
    /// The context each thread builds with in box construction, and the
    /// generation of its collection.
    #[cfg(feature = "parallel")]
    pub(crate) threads: thread_local::ThreadLocal<std::cell::RefCell<(u64, Context)>>,
    /// The fonts the document was handed, which its iframes are handed too.
    given: FontContext,
    /// The installed and shipped fonts.
    base: Collection,
    /// The document's `@font-face` faces.
    document: LayerBuilder,
    /// The collection the context has, which font metrics are read from.
    metrics: std::sync::Arc<std::sync::RwLock<Collection>>,
}

/// An inline formatting context laid out by winkin, set in styles of type `S`.
pub struct TextLayout<S> {
    /// The content, and its lines once they are broken. Its allocations are
    /// reused by the next build.
    layout: Layout,
    /// The `calc()` values the content's styles hold as handles, kept until it is built again.
    calcs: calc::CalcValues,
    /// Whether the layout holds content built for the node as it is now.
    built: bool,
    /// The style the content was last built in, which its alignment and the block's other text
    /// properties were read from.
    built_style: Option<S>,
    /// Whether the content has been broken into lines since it was built.
    laid: bool,
    /// The layout pass its lines were last placed in.
    placed_in: Option<u64>,
    /// What the atomic inlines and floats were last measured at, in content
    /// order, and the width in device pixels that percentages of the
    /// containing block were last taken of.
    boxes: Vec<BuiltBox>,
    basis: f32,
    /// The styles painted with that no node of the content's is styled in.
    styles: PaintStyles<S>,
    /// Which way the content's lines run.
    writing_mode: WritingMode,
    /// Device pixels per CSS pixel used to build the layout.
    scale: f32,
    /// Block-axis offset (in CSS px) of the line boxes from the top of the
    /// container's content box, as applied by `align-content`.
    pub(crate) block_offset: f32,
    /// The size of the content box the lines were last placed in, in device pixels.
    content_size: kurbo::Size,
}

impl<S> Default for TextLayout<S> {
    fn default() -> Self {
        Self {
            layout: Layout::default(),
            calcs: calc::CalcValues::default(),
            built: false,
            built_style: None,
            laid: false,
            placed_in: None,
            boxes: Vec::new(),
            basis: 0.0,
            styles: PaintStyles::default(),
            writing_mode: WritingMode::default(),
            scale: 0.0,
            block_offset: 0.0,
            content_size: kurbo::Size::ZERO,
        }
    }
}

/// A copy starts empty, and is built again the first time it is laid out.
impl<S> Clone for TextLayout<S> {
    fn clone(&self) -> Self {
        Self::default()
    }
}

/// The styles a context's text is painted with where they are not its nodes' own.
struct PaintStyles<S> {
    /// Each element's style on the first formatted line, by node, where the block has a
    /// `::first-line`. The block's own is its `::first-line` style.
    first_line: Vec<(u64, S)>,
    /// The `::first-letter` box's style, and its style on the first line where the block has a
    /// `::first-line`.
    first_letter: Option<(S, Option<S>)>,
    /// The inside list marker's style.
    marker: Option<S>,
    /// Each element's emphasis mark, by node, where its text takes marks:
    /// the mark's string laid out on one line in the element's fonts at the
    /// marks' size, which a mark is drawn as.
    marks: Vec<(u64, Layout)>,
}

impl<S> Default for PaintStyles<S> {
    fn default() -> Self {
        Self {
            first_line: Vec::new(),
            first_letter: None,
            marker: None,
            marks: Vec::new(),
        }
    }
}

/// An atomic inline or a float as the content was last measured with it.
#[derive(Copy, Clone, PartialEq, Debug)]
pub(crate) struct BuiltBox {
    pub(crate) node: u64,
    /// Its border box in device pixels, and its baseline where it has one.
    pub(crate) size: BoxSize,
}

impl<S> std::fmt::Debug for TextLayout<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TextLayout")
    }
}

impl<S: TextStyle> TextLayout<S> {
    /// Whether the content, as it was last built, has lines that run down the
    /// page.
    pub(crate) fn is_vertical(&self) -> bool {
        self.writing_mode != WritingMode::HorizontalTb
    }

    /// Which way the content's lines run, as it was last built.
    pub fn writing_mode(&self) -> WritingMode {
        self.writing_mode
    }

    /// The style `node` is set in on the block's first formatted line, where `::first-line`
    /// restyles it: the block's own is its `::first-line` style.
    pub fn first_line_style_of(&self, node: u64) -> Option<S> {
        self.first_line_style(node).cloned()
    }

    /// Returns the last layout, where there is one.
    pub fn layout(&self) -> Option<&Layout> {
        self.laid.then_some(&self.layout)
    }

    /// The emphasis mark the text of element `node` is marked with, laid
    /// out on one line at the marks' size, where its text takes marks.
    pub fn emphasis_mark(&self, node: u64) -> Option<&Layout> {
        self.styles
            .marks
            .iter()
            .find(|(held, _)| *held == node)
            .map(|(_, layout)| layout)
    }

    /// The style the text and boxes of `key` are painted in, on the block's first formatted
    /// line where `first_line`.
    ///
    /// A text node's is the element it is in, as `host` answers. The block's `::first-letter`
    /// box and its inside list marker have styles of their own, and on the first line every
    /// element is set in its style reparented onto its parent's `::first-line` style.
    pub fn computed_style(
        &self,
        host: &impl PaintHost<S>,
        key: u64,
        first_line: bool,
    ) -> Option<S> {
        if key & FIRST_LETTER_KEY != 0 {
            let (letter, on_first_line) = self.styles.first_letter.as_ref()?;
            return Some(
                on_first_line
                    .as_ref()
                    .filter(|_| first_line)
                    .unwrap_or(letter)
                    .clone(),
            );
        }
        if key & MARKER_KEY != 0 {
            let node = key & !MARKER_KEY;
            if first_line && let Some(style) = self.first_line_style(node) {
                return Some(style.clone());
            }
            return self.styles.marker.clone();
        }
        // The composing text's box, and each piece of an editor's text, are set in the control's
        // style.
        let node = blitz_text::NodeKey(key & !(COMPOSE_KEY | PIECE_KEYS));
        let element = host.element(node)?;
        if first_line && let Some(style) = self.first_line_style(element.0) {
            return Some(style.clone());
        }
        host.style(element)
    }

    /// The style `node` is set in on the first line, if `::first-line` restyles it.
    fn first_line_style(&self, node: u64) -> Option<&S> {
        self.styles
            .first_line
            .iter()
            .find(|(held, _)| *held == node)
            .map(|(_, style)| style)
    }

    /// Measures the content again with its atomic inlines and floats
    /// measured as `sizes` are, and percentages of the containing block
    /// taken of `basis` device pixels.
    ///
    /// winkin keeps the content's analysis, fonts and shaping, and keeps the
    /// lines where nothing changed.
    pub(crate) fn measure(&mut self, cx: &mut Context, sizes: &[BuiltBox], basis: f32) {
        let pairs = sizes.iter().map(|built| (NodeKey(built.node), built.size));
        if self.layout.measure(cx, basis, pairs) {
            self.laid = false;
        }
        self.boxes.clear();
        self.boxes.extend_from_slice(sizes);
        self.basis = basis;
    }

    /// Gives each of `sizes`, measured along the line alone, the block size
    /// and baseline its box was last measured with, where it was.
    pub(crate) fn keep_built_extents(&self, sizes: &mut [BuiltBox]) {
        for (at, size) in sizes.iter_mut().enumerate() {
            // The boxes keep their order, so a box is mostly where it was.
            let built = self
                .boxes
                .get(at)
                .filter(|built| built.node == size.node)
                .or_else(|| self.boxes.iter().find(|built| built.node == size.node));
            if let Some(built) = built {
                size.size.block = built.size.block;
                size.size.baseline = built.size.baseline;
            }
        }
    }

    /// Returns the content's min-content and max-content widths.
    pub(crate) fn intrinsic_widths(&self) -> IntrinsicSizes {
        if self.built {
            self.layout.intrinsic_sizes()
        } else {
            IntrinsicSizes::default()
        }
    }

    /// Breaks the content into lines in `area`, in the room `exclusions`
    /// leaves, and returns the layout.
    pub(crate) fn lay_out(
        &mut self,
        cx: &mut Context,
        area: Area,
        exclusions: &mut dyn Exclusions,
    ) -> Option<&Layout> {
        if !self.built {
            return None;
        }
        self.layout.break_lines(cx, area, exclusions);
        self.laid = true;
        Some(&self.layout)
    }
}

/// A `lang` as winkin takes it: an empty or unreadable one is no language.
fn parse_language(lang: &str) -> Option<Language> {
    Language::parse(lang.trim()).ok()
}

/// The font families a list marker's bullet is set in before the block's
/// own: Blitz's bullet font, which Parley's list markers ask for too.
const BULLET_FAMILIES: [FontFamilyName<'static>; 2] = [
    FontFamilyName::Named(std::borrow::Cow::Borrowed("Bullet")),
    FontFamilyName::Generic(GenericFamily::Monospace),
];

/// Builds the content of the inline formatting context rooted at `root`, as `source` describes
/// it.
///
/// The atomic inlines and floats take the sizes `text` last measured them
/// at, and percentages of the containing block its last basis: none before
/// the first measure, which then sets them.
pub(crate) fn build<S: TextStyle, C: InlineSource<S>>(
    source: &C,
    cx: &mut Context,
    text: &mut TextLayout<S>,
    scale: f32,
    root: blitz_text::NodeKey,
) {
    let key = NodeKey(root.0);
    text.built = false;
    text.laid = false;
    text.scale = scale;
    text.styles.first_line.clear();
    text.styles.first_letter = None;
    text.styles.marker = None;
    text.styles.marks.clear();
    // The handles the old content holds are read no more: the build replaces them all.
    text.calcs.clear();

    let block = source.block(root);
    text.built_style = block.as_ref().map(|block| block.style.clone());
    let Some(BlockInputs {
        style: root_computed,
        container,
        anonymous,
        first_line: first_line_computed,
        language,
        direction: body_direction,
        follows_box,
    }) = block
    else {
        // Nothing styled: an empty layout, so that reading it reads nothing.
        let initial = ComputedStyle::initial();
        text.layout
            .builder(
                cx,
                key,
                &ComputedBlockStyle::new(&initial),
                BuildOptions::default(),
            )
            .finish();
        text.built = true;
        return;
    };
    let language = language.and_then(parse_language);
    let feature_values = source.font_feature_values();
    let root_lists = style::FontLists::of(&root_computed, &feature_values);
    let calcs = &text.calcs;
    let mut root_style = style::computed_style(&root_lists, &root_computed, calcs, scale, language);
    // An anonymous block's lines are the element's around it, whose
    // `unicode-bidi` applies to them: an override, or `plaintext`.
    if anonymous {
        root_style.bidi.unicode_bidi = style::unicode_bidi(&container);
    }
    // The block's `::first-line`, which the first line's elements inherit from.
    let first_line_lists = first_line_computed
        .as_ref()
        .map(|computed| style::FontLists::of(computed, &feature_values));
    let first_line_style = match (&first_line_computed, &first_line_lists) {
        (Some(computed), Some(lists)) => Some(style::computed_style(
            lists, computed, calcs, scale, language,
        )),
        _ => None,
    };
    if let Some(computed) = &first_line_computed {
        text.styles.first_line.push((root.0, computed.clone()));
    }

    let mut block = style::block_style(
        &root_style,
        first_line_style.as_ref(),
        &root_computed,
        &container,
        calcs,
        scale,
    );
    // The root element's lines run in the body's direction, which CSS
    // Writing Modes propagates to the root as its used value.
    if block.direction != BaseDirection::Auto
        && let Some(direction) = body_direction
    {
        block.direction = match direction {
            blitz_text::style::Direction::Rtl => BaseDirection::Rtl,
            blitz_text::style::Direction::Ltr => BaseDirection::Ltr,
        };
    }
    // `text-indent` indents the element's first formatted line, which an
    // anonymous block after another box of the element's does not hold.
    if anonymous && !block.text_indent.hanging && follows_box {
        block.text_indent.amount = winkin::style::LengthPercentageOrCalc::ZERO;
    }
    text.writing_mode = block.writing_mode;

    let TextLayout {
        layout,
        calcs,
        styles,
        boxes,
        basis,
        ..
    } = text;
    let mut options = BuildOptions::default();
    options.percentage_basis = *basis;
    // Hit testing needs to map a layout position back to its DOM node.
    options.map_source = true;
    let mut builder = WinkinBuilder {
        builder: layout.builder(cx, key, &block, options),
        root: root.0,
        boxes: boxes.as_slice(),
        calcs,
        styles,
        first_letter_open: true,
        feature_values: &feature_values,
        marked: Vec::new(),
        scale,
        parents: vec![Parent {
            computed: root_computed.clone(),
            language,
        }],
    };
    source.push_content(root, &mut builder);
    // Moving the builder whole drops its other fields here, ending its
    // borrow of `cx`.
    let WinkinBuilder {
        builder,
        marked,
        styles,
        ..
    } = { builder };
    builder.finish();
    for (node, computed, language) in marked {
        if let Some(layout) = emphasis_mark(cx, &computed, &feature_values, scale, language) {
            styles.marks.push((node, layout));
        }
    }
    text.built = true;
}

/// The emphasis mark of text set in `computed`, laid out on one line: the
/// style's mark string in its fonts at half its size, rounded to a pixel,
/// as Chrome draws the string in the text's font at that size.
fn emphasis_mark<S: TextStyle>(
    cx: &mut Context,
    computed: &S,
    feature_values: &impl FontFeatureValues,
    scale: f32,
    language: Option<Language>,
) -> Option<Layout> {
    let string = style::emphasis_mark_string(computed)?;
    let lists = style::FontLists::of(computed, feature_values);
    // An unboxed style keeps no `calc()` handle.
    let calcs = calc::CalcValues::default();
    let own = unboxed(style::computed_style(
        &lists, computed, &calcs, scale, language,
    ));
    let mark = ComputedStyle {
        font: winkin::style::FontGroup {
            size: (own.font.size / 2.0).round(),
            ..own.font
        },
        text: winkin::style::TextGroup {
            language: own.text.language,
            white_space_collapse: winkin::style::WhiteSpaceCollapse::Preserve,
            wrap_mode: winkin::style::TextWrapMode::NoWrap,
            ..winkin::style::TextGroup::INITIAL
        },
        line: LineGroup::INITIAL,
        decorates: false,
        ..own
    };
    let block = ComputedBlockStyle {
        writing_mode: WritingMode::HorizontalTb,
        direction: BaseDirection::Ltr,
        ..ComputedBlockStyle::new(&mark)
    };
    let mut layout = Layout::default();
    let key = NodeKey(0);
    let mut builder = layout.builder(cx, key, &block, BuildOptions::default());
    builder.text(key, &string);
    builder.finish();
    layout.break_lines(cx, Area::new(f32::MAX / 4.0), &mut winkin::NoExclusions);
    Some(layout)
}

/// `style` as the style of a box that has no box: no edges, nothing
/// painted, and the properties that do not inherit at their initial
/// values, so that the text inside it is set in `style` and nothing else of
/// it shows.
fn unboxed(style: ComputedStyle<'_>) -> ComputedStyle<'_> {
    ComputedStyle {
        edges: EdgesGroup::INITIAL,
        paints: false,
        outlines: false,
        decorates: false,
        line: LineGroup {
            vertical_align: VerticalAlign::Baseline,
            ..style.line
        },
        bidi: BidiGroup {
            unicode_bidi: UnicodeBidi::Normal,
            ..style.bidi
        },
        ..style
    }
}

/// What a box's children are set in: the box around them.
struct Parent<S> {
    computed: S,
    language: Option<Language>,
}

/// A winkin layout builder, as the host pushes into it.
struct WinkinBuilder<'a, 'b, 'c, S, V> {
    builder: LayoutBuilder<'c>,
    /// The key of the block the content is for.
    root: u64,
    boxes: &'a [BuiltBox],
    /// Where the styles keep the `calc()` values winkin resolves later.
    calcs: &'b calc::CalcValues,
    styles: &'b mut PaintStyles<S>,
    /// Whether the block's `::first-letter` may still be found in the text pushed next.
    first_letter_open: bool,
    /// The document's `@font-feature-values` rules.
    feature_values: &'a V,
    /// The elements whose text takes emphasis marks, in the styles and
    /// languages their marks are set in.
    marked: Vec<(u64, S, Option<Language>)>,
    scale: f32,
    /// The boxes open around what is pushed next, the block's first.
    parents: Vec<Parent<S>>,
}

impl<S: TextStyle, V: FontFeatureValues> WinkinBuilder<'_, '_, '_, S, V> {
    /// The box what is pushed next is in.
    fn parent(&self) -> &Parent<S> {
        // The block itself is never popped.
        &self.parents[self.parents.len() - 1]
    }

    /// The size the host measured `node` at.
    fn measured(&self, node: u64) -> BoxSize {
        self.boxes
            .iter()
            .find(|size| size.node == node)
            .map_or(BoxSize::default(), |size| size.size)
    }

    /// The language of a box whose own `lang` is `lang`, in the box what is pushed next is in.
    fn language(&self, lang: Option<&str>) -> Option<Language> {
        match lang {
            Some(lang) => parse_language(lang),
            None => self.parent().language,
        }
    }

    /// The style of an atomic inline or a float, `computed`, as winkin sets
    /// it: where it sits on the line and its margins. Its border and padding
    /// are inside the border box the host measured, and the host paints it.
    fn boxed_style<'s>(
        &self,
        lists: &'s style::FontLists,
        computed: &'s S,
        language: Option<Language>,
    ) -> ComputedStyle<'s> {
        let own = style::computed_style(lists, computed, self.calcs, self.scale, language);
        ComputedStyle {
            edges: EdgesGroup {
                border: EdgesGroup::INITIAL.border,
                padding: EdgesGroup::INITIAL.padding,
                ..own.edges
            },
            paints: false,
            outlines: false,
            ..own
        }
    }
}

impl<S: TextStyle, V: FontFeatureValues> InlineBuilder<S> for WinkinBuilder<'_, '_, '_, S, V> {
    const TRANSFORMS_TEXT: bool = true;
    const SETS_RUBY: bool = true;
    const READS_FIRST_LINE: bool = true;
    const READS_LANG: bool = true;

    /// winkin reads no text back: it applies `text-transform` itself.
    fn pushed_text(&self) -> &str {
        ""
    }

    fn has_pending_whitespace(&self) -> bool {
        false
    }

    /// A marker inside the list item comes before its content, in the bullet
    /// font where it is a bullet.
    fn marker(&mut self, marker: &Marker) {
        let marker_key = NodeKey(MARKER_KEY | self.root);
        let root = &self.parents[0];
        let root_computed = root.computed.clone();
        let language = root.language;
        self.styles.marker = Some(root_computed.clone());
        match marker {
            Marker::Char(char) => {
                let lists = style::FontLists::of(&root_computed, self.feature_values)
                    .with_families_first(&BULLET_FAMILIES);
                let bullet = unboxed(style::computed_style(
                    &lists,
                    &root_computed,
                    self.calcs,
                    self.scale,
                    language,
                ));
                self.builder.open_box(marker_key, &bullet, None);
                self.builder.text(marker_key, &format!("{char} "));
                self.builder.close_box();
            }
            Marker::String(string) => self.builder.text(marker_key, string),
        }
    }

    /// Opens an inline box -- a span, a ruby container, an annotation, or
    /// the box a `display: contents` element's children inherit from.
    fn open_box(
        &mut self,
        key: blitz_text::NodeKey,
        style: &S,
        kind: SpanKind,
        first_line: Option<&S>,
        lang: Option<&str>,
    ) {
        let computed = style.clone();
        let language = self.language(lang);
        // An `<rt>` in an `<rtc>` takes the side of its container, which
        // `ruby-position` applies to.
        let position = match kind {
            SpanKind::Annotation {
                container: Some(position),
            } => Some(style::ruby_position_value(position)),
            _ => None,
        };
        if kind == SpanKind::Ruby {
            self.first_letter_open = false;
        }
        if let Some(style) = first_line {
            self.styles.first_line.push((key.0, style.clone()));
        }
        {
            let lists = style::FontLists::of(&computed, self.feature_values);
            let own = style::computed_style(&lists, &computed, self.calcs, self.scale, language);
            let first_line_lists =
                first_line.map(|computed| style::FontLists::of(computed, self.feature_values));
            let first_line_style = match (first_line, &first_line_lists) {
                (Some(computed), Some(lists)) => Some(style::computed_style(
                    lists, computed, self.calcs, self.scale, language,
                )),
                _ => None,
            };
            let key = NodeKey(key.0);
            let builder = &mut self.builder;
            match (kind, position) {
                (SpanKind::Span, _) => builder.open_box(key, &own, first_line_style.as_ref()),
                (SpanKind::Ruby, _) => builder.open_ruby(key, &own, first_line_style.as_ref()),
                (SpanKind::Annotation { .. }, Some(position)) => builder
                    .open_annotation_with_position(key, &own, first_line_style.as_ref(), position),
                (SpanKind::Annotation { .. }, None) => {
                    builder.open_annotation(key, &own, first_line_style.as_ref())
                }
                // No box, but its children inherit from it: a box with no
                // edges and nothing painted, in its style.
                (SpanKind::Contents, _) => {
                    let first_line_style = first_line_style.map(unboxed);
                    builder.open_box(key, &unboxed(own), first_line_style.as_ref());
                }
            }
        }
        self.parents.push(Parent { computed, language });
    }

    /// Opens an anonymous ruby container or annotation, keyed by no node, with
    /// no edges, in the style of the `<rtc>` or else of the box it is in.
    fn open_anonymous_box(
        &mut self,
        style: Option<&S>,
        kind: SpanKind,
        first_line: Option<&S>,
        lang: Option<&str>,
    ) {
        let computed = style.unwrap_or(&self.parent().computed).clone();
        let language = self.language(lang);
        let key = NodeKey(ANONYMOUS_KEY);
        {
            let lists = style::FontLists::of(&computed, self.feature_values);
            let own = unboxed(style::computed_style(
                &lists, &computed, self.calcs, self.scale, language,
            ));
            let first_line_lists =
                first_line.map(|computed| style::FontLists::of(computed, self.feature_values));
            let first_line_style = match (first_line, &first_line_lists) {
                (Some(computed), Some(lists)) => Some(unboxed(style::computed_style(
                    lists, computed, self.calcs, self.scale, language,
                ))),
                _ => None,
            };
            let builder = &mut self.builder;
            match kind {
                SpanKind::Annotation {
                    container: Some(position),
                } => builder.open_annotation_with_position(
                    key,
                    &own,
                    first_line_style.as_ref(),
                    style::ruby_position_value(position),
                ),
                SpanKind::Annotation { container: None } => {
                    builder.open_annotation(key, &own, first_line_style.as_ref())
                }
                SpanKind::Ruby => builder.open_ruby(key, &own, first_line_style.as_ref()),
                SpanKind::Span | SpanKind::Contents => {
                    builder.open_box(key, &own, first_line_style.as_ref())
                }
            }
        }
        self.parents.push(Parent { computed, language });
    }

    fn start_ruby_column(&mut self) {
        self.builder.start_column();
    }

    fn close_box(&mut self, kind: SpanKind) {
        if self.parents.len() > 1 {
            self.parents.pop();
        }
        match kind {
            SpanKind::Span | SpanKind::Contents => self.builder.close_box(),
            SpanKind::Ruby => self.builder.close_ruby(),
            SpanKind::Annotation { .. } => self.builder.close_annotation(),
        }
    }

    fn wants_first_letter(&self) -> bool {
        self.first_letter_open
    }

    /// Asks for the block's `::first-letter` before the text pushed next, in the style the
    /// pseudo-element has in the box the text is in.
    ///
    /// The builder finds the letter as the text is written; it is found once a text holds
    /// anything but white space and punctuation, and asking again after is ignored, so the
    /// asking stops there.
    fn first_letter(&mut self, style: &S, first_line: Option<&S>) {
        let parent = self.parent();
        let language = parent.language;
        {
            let lists = style::FontLists::of(style, self.feature_values);
            let letter = style::computed_style(&lists, style, self.calcs, self.scale, language);
            let first_line_lists =
                first_line.map(|computed| style::FontLists::of(computed, self.feature_values));
            let first_line_style = match (first_line, &first_line_lists) {
                (Some(computed), Some(lists)) => Some(style::computed_style(
                    lists, computed, self.calcs, self.scale, language,
                )),
                _ => None,
            };
            self.builder.set_first_letter(
                NodeKey(FIRST_LETTER_KEY | self.root),
                &letter,
                first_line_style.as_ref(),
            );
        }
        self.styles.first_letter = Some((style.clone(), first_line.cloned()));
    }

    /// A text node carries no style of its own: the box it is in is what sets
    /// it, and a painter reads the element it is in.
    fn text(&mut self, key: blitz_text::NodeKey, parent_key: blitz_text::NodeKey, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.first_letter_open
            && text
                .chars()
                .any(|ch| !ch.is_whitespace() && !is_punctuation(ch))
        {
            self.first_letter_open = false;
        }
        let parent = self.parent();
        if style::emphasis_mark_string(&parent.computed).is_some()
            && !self.marked.iter().any(|(held, ..)| *held == parent_key.0)
        {
            let marked = (parent_key.0, parent.computed.clone(), parent.language);
            self.marked.push(marked);
        }
        self.builder.text(NodeKey(key.0), text);
    }

    /// Pushes an atomic inline, sized as the host measured it.
    fn atomic(&mut self, key: blitz_text::NodeKey, computed: &S, lang: Option<&str>) {
        // A block has one first letter, and it comes before any atomic
        // inline.
        self.first_letter_open = false;
        let language = self.language(lang);
        let lists = style::FontLists::of(computed, self.feature_values);
        let own = self.boxed_style(&lists, computed, language);
        let size = self.measured(key.0);
        self.builder.atomic(NodeKey(key.0), &own, None, size);
    }

    /// Pushes a float, sized as the host measured it.
    fn float(
        &mut self,
        key: blitz_text::NodeKey,
        computed: &S,
        side: blitz_text::FloatSide,
        lang: Option<&str>,
    ) {
        let side = match side {
            blitz_text::FloatSide::Left => FloatSide::Left,
            blitz_text::FloatSide::Right => FloatSide::Right,
        };
        let language = self.language(lang);
        let lists = style::FontLists::of(computed, self.feature_values);
        let own = self.boxed_style(&lists, computed, language);
        let size = self.measured(key.0);
        self.builder.float(NodeKey(key.0), &own, side, size);
    }

    /// Pushes the anchor of an absolutely positioned box, which takes no room and whose static
    /// position the lines find.
    fn absolute(&mut self, key: blitz_text::NodeKey, _style: &S, inline_level: bool) {
        let display = if inline_level {
            OriginalDisplay::Inline
        } else {
            OriginalDisplay::Block
        };
        self.builder.absolute(NodeKey(key.0), display);
    }

    fn line_break(&mut self, key: blitz_text::NodeKey, _style: &S, clear: Option<Clear>) {
        self.first_letter_open = false;
        let key = NodeKey(key.0);
        match clear {
            Some(Clear::Left) => self.builder.line_break_clearing(key, winkin::Clear::Left),
            Some(Clear::Right) => self.builder.line_break_clearing(key, winkin::Clear::Right),
            Some(Clear::Both) => self.builder.line_break_clearing(key, winkin::Clear::Both),
            None => self.builder.line_break(key),
        }
    }

    fn break_opportunity(&mut self, _key: blitz_text::NodeKey) -> bool {
        self.builder.break_opportunity();
        true
    }
}

/// Whether `ch` is punctuation, which a first letter may take before it
/// and goes on looking past.
fn is_punctuation(ch: char) -> bool {
    ch.is_ascii_punctuation()
        || matches!(
            ch,
            '\u{00A1}' | '\u{00A7}' | '\u{00AB}' | '\u{00B6}' | '\u{00B7}' | '\u{00BB}' | '\u{00BF}'
                | '\u{2010}'..='\u{2027}'
                | '\u{2030}'..='\u{205E}'
                | '\u{3001}'..='\u{3003}'
                | '\u{3008}'..='\u{3011}'
                | '\u{3014}'..='\u{301F}'
                | '\u{FF01}'..='\u{FF0F}'
        )
}

/// What takes a point of the area the lines were broken in -- along the lines
/// from their line-left end, across them from the block-start edge -- onto the
/// content box, which is `content_width` device pixels wide and
/// `content_height` tall.
///
/// The identity for horizontal text. A vertical block's lines run down the
/// box and stack across it, from the right in `vertical-rl` and from the left
/// in `vertical-lr`; `sideways-lr` runs its lines up the box, so its origin is
/// the bottom edge.
pub fn page(writing_mode: WritingMode, content_width: f64, content_height: f64) -> Affine {
    match writing_mode {
        WritingMode::VerticalRl | WritingMode::SidewaysRl => {
            Affine::new([0.0, 1.0, -1.0, 0.0, content_width, 0.0])
        }
        WritingMode::VerticalLr => Affine::new([0.0, 1.0, 1.0, 0.0, 0.0, 0.0]),
        WritingMode::SidewaysLr => Affine::new([0.0, -1.0, 1.0, 0.0, 0.0, content_height]),
        WritingMode::HorizontalTb => Affine::IDENTITY,
    }
}

/// What takes a point of a line -- along it from its line box's left, across
/// it from its over edge -- onto the content box.
///
/// `vertical-lr` stacks its lines from the left while each line's over side
/// is its right, so a line is mirrored across its own box there.
pub fn line_frame(writing_mode: WritingMode, page: Affine, metrics: &LineMetrics) -> Affine {
    let placed = Affine::translate((f64::from(metrics.left), f64::from(metrics.top)));
    if writing_mode == WritingMode::VerticalLr {
        let across = f64::from(2.0 * metrics.top + metrics.height());
        page * Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, across]) * placed
    } else {
        page * placed
    }
}

/// The parts of `key`'s inline box on each line, as rects in the content
/// box, in device pixels: one a line, the union of its parts there, as
/// `getClientRects` answers.
///
/// The content box is `content` device pixels in size, which the vertical
/// writing modes place their lines in from the right or the bottom.
pub(crate) fn box_rects(
    layout: &Layout,
    writing_mode: WritingMode,
    content: kurbo::Size,
    key: u64,
) -> impl Iterator<Item = kurbo::Rect> + '_ {
    let page = page(writing_mode, content.width, content.height);
    let mut fragments = layout.box_fragments(NodeKey(key)).peekable();
    core::iter::from_fn(move || {
        let first = fragments.next()?;
        let line = first.line();
        let mut united = fragment_rect(&first);
        while let Some(next) = fragments.next_if(|fragment| fragment.line() == line) {
            united = united.union(fragment_rect(&next));
        }
        let metrics = layout.line(line)?.metrics();
        Some(line_frame(writing_mode, page, &metrics).transform_rect_bbox(united))
    })
}

/// A box part's border box in its line's terms: along the line from its
/// line box's left, across it from its over edge.
fn fragment_rect(fragment: &winkin::BoxFragment<'_>) -> kurbo::Rect {
    let inline = fragment.inline();
    let block = fragment.block();
    kurbo::Rect::new(
        f64::from(inline.left),
        f64::from(block.over),
        f64::from(inline.right),
        f64::from(block.under),
    )
}

/// A piece of a line's content, as [`logical_content`] reads it.
#[derive(Clone)]
enum LinePiece {
    /// The text between these byte offsets of the layout text, of the node the key names, or of
    /// the node the layout maps its start to.
    Text(Option<u64>, core::ops::Range<usize>),
    /// An atomic inline, a float or an absolutely positioned box.
    Box(u64),
}

/// The laid-out text and inline boxes of `layout` in logical order, as its lines place them, read
/// as it is walked: each stretch of the layout text one node holds as one item. A float or an
/// absolutely positioned box follows the content of the line it is anchored on, and one anchored
/// on no line follows the last line.
pub(crate) fn logical_content(layout: &Layout) -> impl Iterator<Item = ContentItem> + '_ {
    coalesce(content_pieces(layout))
}

/// Joins each run of text items of one node that follow on in the text into one item.
fn coalesce(mut items: impl Iterator<Item = ContentItem>) -> impl Iterator<Item = ContentItem> {
    let mut pending: Option<ContentItem> = None;
    core::iter::from_fn(move || {
        loop {
            let Some(item) = items.next() else {
                return pending.take();
            };
            if let (
                Some(ContentItem::Text { key, range }),
                ContentItem::Text {
                    key: next_key,
                    range: next,
                },
            ) = (&mut pending, &item)
                && key == next_key
                && range.end == next.start
            {
                range.end = next.end;
                continue;
            }
            if let Some(read) = pending.replace(item) {
                return Some(read);
            }
        }
    })
}

/// The pieces of `layout`'s content in logical order: each slice the text copies as, and each
/// box.
fn content_pieces(layout: &Layout) -> impl Iterator<Item = ContentItem> + '_ {
    use winkin::selection::{Affinity, CopyKind, Position};
    let shown = |key: u64| key & (FIRST_LETTER_KEY | MARKER_KEY) == 0;
    let text = layout.text();
    let mut lines = layout.lines();
    let mut statics = layout.static_positions().peekable();
    let mut line_floats = 0;
    let mut past_lines = false;
    // The current line's items, sorted into logical order, and its pieces
    let mut items: Vec<(core::ops::Range<usize>, u64, bool)> = Vec::new();
    let mut pieces = Vec::new();
    let mut next_piece = 0;
    // The current text piece's node, its start and the slices it copies as
    let mut slices = None;
    core::iter::from_fn(move || {
        loop {
            if slices.is_none() {
                while next_piece == pieces.len() {
                    pieces.clear();
                    next_piece = 0;
                    if let Some(line) = lines.next() {
                        items.clear();
                        items.extend(line.all_items().filter_map(|item| match item {
                            Item::Text(run) => Some((run.text_range(), run.key().0, false)),
                            Item::Atomic(atomic) => {
                                Some((atomic.text_range(), atomic.key().0, true))
                            }
                            _ => None,
                        }));
                        items.sort_by_key(|(range, ..)| range.start);
                        // What the runs leave of the line, such as a forced break or a space the
                        // line wraps at, goes with the node the layout maps it to.
                        let line_range = line.text_range();
                        let mut cursor = line_range.start;
                        for (range, key, atomic) in items.drain(..) {
                            if range.start > cursor {
                                pieces.push(LinePiece::Text(None, cursor..range.start));
                            }
                            if !atomic {
                                pieces.push(LinePiece::Text(Some(key), range.clone()));
                            } else if shown(key) {
                                pieces.push(LinePiece::Box(key));
                            }
                            cursor = cursor.max(range.end);
                        }
                        if line_range.end > cursor {
                            pieces.push(LinePiece::Text(None, cursor..line_range.end));
                        }
                        for float in line.floats() {
                            line_floats += 1;
                            if shown(float.key.0) {
                                pieces.push(LinePiece::Box(float.key.0));
                            }
                        }
                        let index = line.index();
                        while let Some(position) = statics
                            .next_if(|position| position.line.is_some_and(|line| line <= index))
                        {
                            pieces.push(LinePiece::Box(position.key.0));
                        }
                    } else if !past_lines {
                        past_lines = true;
                        let floats = layout.floats().skip(line_floats);
                        pieces.extend(
                            floats
                                .map(|float| float.key.0)
                                .filter(|&key| shown(key))
                                .map(LinePiece::Box),
                        );
                        pieces.extend(
                            statics
                                .by_ref()
                                .map(|position| LinePiece::Box(position.key.0)),
                        );
                    } else {
                        return None;
                    }
                }
                let piece = pieces[next_piece].clone();
                next_piece += 1;
                match piece {
                    LinePiece::Box(key) => {
                        return Some(ContentItem::Box(blitz_text::NodeKey(key)));
                    }
                    LinePiece::Text(key, range) => {
                        let key = key.or_else(|| {
                            let position = Position::new(range.start, Affinity::Downstream);
                            layout.node_position(position).map(|node| node.key.0)
                        });
                        let Some(key) = key.filter(|&key| shown(key)) else {
                            continue;
                        };
                        let copied = layout.selected_text(range, CopyKind::Text);
                        slices = Some((blitz_text::NodeKey(key), copied));
                    }
                }
            }
            let (key, copied) = slices.as_mut()?;
            let Some(slice) = copied.next() else {
                slices = None;
                continue;
            };
            // The slices borrow from the layout text, which says where each is.
            let at = (slice.as_ptr() as usize).wrapping_sub(text.as_ptr() as usize);
            if slice.is_empty() || at >= text.len() {
                continue;
            }
            return Some(ContentItem::Text {
                key: *key,
                range: at..at + slice.len(),
            });
        }
    })
}

impl<S: TextStyle> InlineText for TextLayout<S> {
    #[inline]
    fn scale(&self) -> f32 {
        self.scale
    }

    #[inline]
    fn block_offset(&self) -> f32 {
        self.block_offset
    }

    #[inline]
    fn text(&self) -> &str {
        self.layout().map_or("", |layout| layout.text())
    }

    #[inline]
    fn text_len(&self) -> usize {
        self.text().len()
    }

    fn selected_text(&self, start: usize, end: usize) -> impl Iterator<Item = &str> {
        self.layout()
            .filter(|layout| start < end && end <= layout.text().len())
            .into_iter()
            .flat_map(move |layout| {
                layout.selected_text(start..end, winkin::selection::CopyKind::Text)
            })
    }

    fn logical_content(&self) -> impl Iterator<Item = ContentItem> {
        self.layout().into_iter().flat_map(logical_content)
    }

    fn source_offset(&self, key: blitz_text::NodeKey, offset: usize) -> Option<usize> {
        let layout = self.layout()?;
        let position = layout.position(
            NodeKey(key.0),
            offset,
            winkin::selection::Affinity::Downstream,
        )?;
        Some(position.offset)
    }

    #[inline]
    fn maps_source(&self) -> bool {
        true
    }

    fn first_baseline(&self) -> Option<f32> {
        let line = self.layout()?.lines().next()?;
        Some(line.metrics().baseline / self.scale + self.block_offset)
    }

    /// Hit tests physical CSS-pixel coordinates relative to the inline root's content box.
    fn hit_test(&self, x: f32, y: f32, exact: bool) -> Option<TextHit> {
        let layout = self.layout()?;
        let scale = self.scale;
        let page = page(
            self.writing_mode(),
            self.content_size.width,
            self.content_size.height,
        );
        let point = page.inverse()
            * kurbo::Point::new(
                f64::from(x * scale),
                f64::from((y - self.block_offset) * scale),
            );
        let (inline, block) = (point.x as f32, point.y as f32);
        if exact
            && !layout.lines().any(|line| {
                let metrics = line.metrics();
                block >= metrics.top
                    && block < metrics.top + metrics.height()
                    && inline >= metrics.left
                    && inline < metrics.left + metrics.width
            })
        {
            return None;
        }
        let position = layout.hit_test(inline, block, winkin::config::PastLines::Column)?;
        let node = layout.node_position(position)?;
        Some(TextHit {
            key: blitz_text::NodeKey(node.key.0),
            offset: position.offset,
        })
    }

    /// Calls `f` with each rectangle that highlights the text between `start` and `end`, in
    /// physical device pixels relative to the content box.
    fn for_each_selection_rect(&self, start: usize, end: usize, mut f: impl FnMut(kurbo::Rect)) {
        let Some(layout) = self.layout() else {
            return;
        };
        let mode = self.writing_mode();
        let page = page(mode, self.content_size.width, self.content_size.height);
        for selection in layout.selection_rects(start..end) {
            let Some(line) = layout.line(selection.line) else {
                continue;
            };
            let rect = kurbo::Rect::new(
                f64::from(selection.inline.left),
                f64::from(selection.block.over),
                f64::from(selection.inline.right),
                f64::from(selection.block.under),
            );
            f(line_frame(mode, page, &line.metrics()).transform_rect_bbox(rect));
        }
    }

    fn fragment_rects(
        &self,
        key: blitz_text::NodeKey,
        origin: (f32, f32),
        content: kurbo::Size,
        shift: kurbo::Vec2,
        _contains: impl Fn(blitz_text::NodeKey) -> bool,
    ) -> impl Iterator<Item = kurbo::Rect> {
        let (origin_x, origin_y) = (origin.0, origin.1 + self.block_offset);
        let scale = f64::from(self.scale);
        let writing_mode = self.writing_mode;
        self.layout().into_iter().flat_map(move |layout| {
            box_rects(layout, writing_mode, content, key.0).map(move |rect| {
                kurbo::Rect::new(
                    f64::from(origin_x + (shift.x + rect.x0 / scale) as f32),
                    f64::from(origin_y + (shift.y + rect.y0 / scale) as f32),
                    f64::from(origin_x + (shift.x + rect.x1 / scale) as f32),
                    f64::from(origin_y + (shift.y + rect.y1 / scale) as f32),
                )
            })
        })
    }

    fn debug_print(&self) {
        let Some(layout) = self.layout() else {
            println!("Not laid out");
            return;
        };
        println!("Text content: {:?}", layout.text());
        println!("Lines:");
        for line in layout.lines() {
            let metrics = line.metrics();
            println!(
                "Line {}: left:{} top:{} width:{} height:{} text:{:?}",
                line.index(),
                metrics.left,
                metrics.top,
                metrics.width,
                metrics.height(),
                line.text_range(),
            );
        }
    }
}

impl<S: TextStyle> InlineLayout<S> for TextLayout<S> {
    type Context = TextContext;

    const SETS_WRITING_MODES: bool = true;
    const READS_ROOM_ABOVE: bool = true;
    const SHIFTS_RELATIVE_INLINES: bool = true;

    fn style_change_rebuilds<T: TextStyle>(old: &T, new: &T) -> bool {
        // The build reads every inherited text and box property, the text
        // properties that do not inherit, and an inline-level element's box,
        // margins, borders and padding; measuring takes only the boxes'
        // sizes and the basis percentages are taken of. The properties read
        // only into the block's facts are the root's alone, which `prepare`
        // compares and builds again for in place.
        if !old.same_inline_text(new) || !old.same_inherited_box(new) || !old.same_text(new) {
            return true;
        }
        let inline =
            |style: &T| style.display().outside == blitz_text::style::DisplayOutside::Inline;
        (inline(old) || inline(new))
            && (!old.same_box(new)
                || !old.same_margin(new)
                || !old.same_border(new)
                || !old.same_padding(new))
    }

    fn takes_room_above(&self) -> bool {
        self.built && self.layout.has_annotations()
    }

    fn build_layouts<C: InlineSource<S>>(
        cx: &mut TextContext,
        source: &C,
        scale: f32,
        layouts: &mut [(blitz_text::NodeKey, Box<Self>)],
    ) {
        // Each atomic inline and float takes the size it was last measured
        // at, which the first measure in layout replaces.
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            // Each thread builds with a context of its own, which takes the
            // document's collection and configuration. Its caches stay warm
            // from one construction to the next.
            let (generation, threads) = (cx.generation, &cx.threads);
            let (collection, config) = (cx.cx.collection(), *cx.cx.config());
            let shared = cx.given.shared_fonts();
            layouts.par_iter_mut().for_each(|(node, layout)| {
                let mut thread = threads
                    .get_or(|| {
                        let mut own =
                            Context::with_shared_fonts(collection.clone(), shared.clone());
                        own.set_config(config);
                        own.set_calc_resolver(Some(Box::new(calc::HostCalc)));
                        std::cell::RefCell::new((generation, own))
                    })
                    .borrow_mut();
                let (held, own) = &mut *thread;
                if *held != generation {
                    own.set_collection(collection.clone());
                    *held = generation;
                }
                build(source, own, layout, scale, *node);
            });
        }
        #[cfg(not(feature = "parallel"))]
        for (node, layout) in layouts {
            build(source, &mut cx.cx, layout, scale, *node);
        }
    }

    fn is_empty(&self) -> bool {
        // The content is built when it is laid out.
        false
    }

    fn prepare<C: InlineSource<S>>(
        &mut self,
        cx: &mut TextContext,
        source: &C,
        root: blitz_text::NodeKey,
        scale: f32,
        lines: LinesInputs<'_>,
    ) {
        // Each border box, and where an atomic inline's baseline is, in device
        // pixels. In a vertical line a box's height is along it, and it sits on
        // no baseline of its own.
        let mut sizes: Vec<BuiltBox> = lines
            .sizes
            .iter()
            .map(|measured| BuiltBox {
                node: measured.key.0,
                size: if lines.vertical {
                    BoxSize {
                        inline: measured.size.height * scale,
                        block: measured.size.width * scale,
                        baseline: None,
                    }
                } else {
                    BoxSize {
                        inline: measured.size.width * scale,
                        block: measured.size.height * scale,
                        baseline: measured.baseline.map(|baseline| baseline * scale),
                    }
                },
            })
            .collect();
        // An inline-size pass reads only how far each box reaches along the
        // line: each box keeps the block size and baseline it was last
        // measured with, which a later pass that sets the lines checks.
        if lines.pass == Measure::InlineSizes {
            self.keep_built_extents(&mut sizes);
        }
        // Box construction builds the content. A context it left unbuilt is
        // built here, as is one whose root's indent, wrap style or fit changed
        // since it was built, which keeps its box. A change to the alignment
        // alone is set on the built content, whose lines are then broken
        // again.
        if self.built
            && let Some(built) = &self.built_style
            && let Some(now) = source.block_style(root)
            && !built.same_block_text(&now)
        {
            if built.same_block_text_but_alignment(&now) {
                let (align, last) = style::text_align(&now);
                if self.layout.set_text_align(align, last) {
                    self.laid = false;
                    self.placed_in = None;
                }
                self.built_style = Some(now);
            } else {
                self.built = false;
            }
        }
        if !self.built {
            build(source, &mut cx.cx, self, scale, root);
        }
        self.measure(&mut cx.cx, &sizes, lines.basis * scale);
    }

    fn content_widths(&mut self) -> ContentWidths {
        let widths = self.intrinsic_widths();
        ContentWidths {
            min: widths.min_content,
            max: widths.max_content,
        }
    }

    fn break_lines(
        &mut self,
        cx: &mut TextContext,
        line_area: LineArea,
        _style: Option<&S>,
        exclusions: &mut impl LineExclusions,
    ) {
        let area = Area {
            room_above: line_area.room_above,
            block_end: line_area.block_end,
            ..Area::new(line_area.width)
        };
        // A pass that only sizes the block breaks without placing, unless the block was placed in
        // this pass already: its lines then stay placed for painting.
        let pass = Some(line_area.pass);
        if line_area.sizes_only && self.placed_in != pass {
            if self.built {
                self.layout
                    .size_lines(&mut cx.cx, area, &mut ExclusionsOf(exclusions));
                self.laid = true;
            }
        } else {
            self.lay_out(&mut cx.cx, area, &mut ExclusionsOf(exclusions));
            self.placed_in = pass;
        }
    }

    fn extent(&self, end_padding: f32) -> LinesExtent {
        let Some(layout) = self.layout() else {
            return LinesExtent::default();
        };
        let metrics = layout.metrics();
        // Only annotations reaching under the last line reach into the padding, so with none
        // there is nothing to work out.
        let into_padding = if end_padding > 0.0 && layout.has_annotations() {
            into_end_padding(layout.room_below(), end_padding)
        } else {
            0.0
        };
        LinesExtent {
            height: metrics.block_end - into_padding,
            width: layout
                .lines()
                .map(|line| {
                    let metrics = line.metrics();
                    metrics.left + metrics.width
                })
                .fold(0.0, f32::max),
            first_baseline: metrics.first_baseline,
            last_baseline: metrics.last_baseline,
        }
    }

    fn set_frame(&mut self, block_offset: f32, content: blitz_text::Size) {
        self.block_offset = block_offset;
        self.content_size = kurbo::Size::new(f64::from(content.width), f64::from(content.height));
    }

    fn placements(&self) -> impl Iterator<Item = Placement> {
        let mut placed = Vec::new();
        if let Some(layout) = self.layout() {
            if self.is_vertical() {
                self.vertical_placements(layout, &mut placed);
            } else {
                horizontal_placements(layout, &mut placed);
            }
        }
        placed.into_iter()
    }

    fn line_flow(&self) -> LineFlow {
        match self.writing_mode {
            WritingMode::HorizontalTb => LineFlow::Horizontal,
            WritingMode::VerticalRl => LineFlow::VerticalRl,
            WritingMode::VerticalLr => LineFlow::VerticalLr,
            WritingMode::SidewaysRl => LineFlow::SidewaysRl,
            WritingMode::SidewaysLr => LineFlow::SidewaysLr,
        }
    }

    fn place_on_page(&self, along: [f32; 2], across: [f32; 2]) -> (f32, f32) {
        let content = self.content_size;
        let (width, height) = (content.width as f32, content.height as f32);
        let [left, right] = along;
        let [start, end] = across;
        match self.writing_mode {
            WritingMode::VerticalRl | WritingMode::SidewaysRl => (width - end, left),
            WritingMode::VerticalLr => (start, left),
            WritingMode::SidewaysLr => (start, height - right),
            WritingMode::HorizontalTb => (left, start),
        }
    }

    fn last_line_baseline(&self) -> LastBaseline {
        let Some(layout) = self.layout() else {
            return LastBaseline::Unknown;
        };
        if self.is_vertical() {
            return LastBaseline::Unknown;
        }
        match layout.metrics().last_baseline {
            Some(baseline) => LastBaseline::At(baseline),
            None => LastBaseline::None,
        }
    }

    fn room_below(&self) -> f32 {
        self.layout().map_or(0.0, |layout| layout.room_below())
    }

    fn float_node(key: u64) -> Option<blitz_text::NodeKey> {
        // The block's initial letter and its inside marker have no node.
        (key & (FIRST_LETTER_KEY | MARKER_KEY) == 0).then_some(blitz_text::NodeKey(key))
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

impl<S: TextStyle> TextLayout<S> {
    /// What the lines of a vertical layout placed, turned onto the page: each
    /// atomic inline's margin box, and where each absolutely positioned box
    /// would have been.
    fn vertical_placements(&self, layout: &Layout, placed: &mut Vec<Placement>) {
        let writing_mode = self.writing_mode;
        let at = |node: u64, (x, y): (f32, f32), rtl| Placement {
            key: blitz_text::NodeKey(node),
            x,
            top: y,
            line_top: y,
            line_bottom: y,
            block_start: y,
            rtl,
        };
        for line in layout.lines() {
            let metrics = line.metrics();
            // `vertical-lr` stacks its lines from the left while each line's
            // over side is its right.
            let block = |over: f32, under: f32| {
                if writing_mode == WritingMode::VerticalLr {
                    let bottom = metrics.top + metrics.height();
                    [bottom - under, bottom - over]
                } else {
                    [metrics.top + over, metrics.top + under]
                }
            };
            for item in line.items() {
                if let Item::Atomic(atomic) = item {
                    let inline = atomic.inline();
                    let cross = atomic.block();
                    let page = InlineLayout::<S>::place_on_page(
                        self,
                        [metrics.left + inline.left, metrics.left + inline.right],
                        block(cross.over, cross.under),
                    );
                    placed.push(at(atomic.key().0, page, None));
                }
            }
        }
        // An absolutely positioned box's block-start edge, across the page,
        // faces right where the lines stack from the right.
        let from_right = matches!(
            writing_mode,
            WritingMode::VerticalRl | WritingMode::SidewaysRl
        );
        for position in layout.static_positions() {
            let page = InlineLayout::<S>::place_on_page(
                self,
                [position.inline, position.inline],
                [position.block, position.block],
            );
            placed.push(at(position.key.0, page, Some(from_right)));
        }
    }
}

/// What the lines of a horizontal layout placed: each atomic inline's margin
/// box on its line, and where each absolutely positioned box would have been,
/// as winkin finds its static position: its line box spans the block axis of
/// an inline-level box's static-position rectangle.
fn horizontal_placements(layout: &Layout, placed: &mut Vec<Placement>) {
    for line in layout.lines() {
        let metrics = line.metrics();
        for item in line.items() {
            if let Item::Atomic(atomic) = item {
                let inline = atomic.inline();
                let block = atomic.block();
                placed.push(Placement {
                    key: blitz_text::NodeKey(atomic.key().0),
                    x: metrics.left + inline.left,
                    top: metrics.top + block.over,
                    line_top: metrics.top,
                    line_bottom: metrics.top + metrics.height(),
                    block_start: metrics.top,
                    rtl: None,
                });
            }
        }
    }
    for at in layout.static_positions() {
        let bottom = at
            .line
            .and_then(|line| layout.line(line))
            .map_or(at.block, |line| {
                let metrics = line.metrics();
                (metrics.top + metrics.height()).max(at.block)
            });
        placed.push(Placement {
            key: blitz_text::NodeKey(at.key.0),
            x: at.inline,
            top: at.block,
            line_top: at.block,
            line_bottom: bottom,
            block_start: at.block,
            rtl: Some(at.direction == winkin::style::Direction::Rtl),
        });
    }
}

/// How far the last line's ruby annotations and emphasis marks reach into
/// the block's end padding, `padding` device pixels, where they reach past
/// its line box: the layout's end counts them, and Chrome lets them into
/// the padding rather than make the block taller.
fn into_end_padding(room_below: f32, padding: f32) -> f32 {
    (-room_below).clamp(0.0, padding.max(0.0))
}

/// The floats the host keeps an inline formatting context's lines clear of, as winkin asks
/// after them.
struct ExclusionsOf<'a, E>(&'a mut E);

impl<E: LineExclusions> Exclusions for ExclusionsOf<'_, E> {
    fn band(&self, _line: usize, block: BlockExtents) -> InlineExtents {
        let (left, right) = self.0.band(block.start, block.end);
        InlineExtents { left, right }
    }

    fn below(&self, top: f32) -> Option<f32> {
        self.0.below(top)
    }

    fn place(&mut self, float: winkin::FloatRequest) -> PlacedFloat {
        let placed = self.0.place(FloatRequest {
            key: float.key.0,
            side: match float.side {
                FloatSide::Left => blitz_text::FloatSide::Left,
                FloatSide::Right => blitz_text::FloatSide::Right,
            },
            inline_size: float.inline_size,
            block_size: float.block_size,
            block_start: float.block_start,
            initial_letter: float.key.0 & FIRST_LETTER_KEY != 0,
        });
        PlacedFloat {
            inline: InlineExtents {
                left: placed.left,
                right: placed.right,
            },
            block: BlockExtents {
                start: placed.top,
                end: placed.bottom,
            },
        }
    }

    fn checkpoint(&self) -> ExclusionsCheckpoint {
        ExclusionsCheckpoint(self.0.checkpoint() as u64)
    }

    fn rewind(&mut self, to: ExclusionsCheckpoint) {
        self.0.rewind(to.0 as usize);
    }
}

/// Builds `content`, the value of the `<input>` or `<textarea>` `node`, into `text`: set in
/// `computed`, its computed style, with white space preserved (as `break-spaces` where the style
/// asks for it). The lines wrap at `width` device pixels as the style wraps them, or not at all
/// where it is `None`.
///
/// The text is laid out in up to three pieces, keyed as [`PIECE_KEYS`] says: before, inside and
/// after `composition`, the text an input method is composing, which is underlined, or else
/// `revealed`, the text a password field shows in the clear. Where the style masks the text
/// (`-webkit-text-security`) or transforms it (`text-transform`), the layout maps each piece's
/// offsets back to `content`, and this returns true. Otherwise offsets in the layout's text are
/// offsets in `content`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_plain_text<S: TextStyle>(
    cx: &mut TextContext,
    text: &mut TextLayout<S>,
    node: u64,
    computed: &S,
    content: &str,
    composition: Option<std::ops::Range<usize>>,
    revealed: Option<std::ops::Range<usize>>,
    scale: f32,
    width: Option<f32>,
) -> bool {
    let lists = style::FontLists::of(computed, &());
    text.calcs.clear();
    let own = style::computed_style(&lists, computed, &text.calcs, scale, None);
    let own = ComputedStyle {
        text: winkin::style::TextGroup {
            white_space_collapse: match own.text.white_space_collapse {
                winkin::style::WhiteSpaceCollapse::BreakSpaces => {
                    winkin::style::WhiteSpaceCollapse::BreakSpaces
                }
                _ => winkin::style::WhiteSpaceCollapse::Preserve,
            },
            wrap_mode: match width {
                Some(_) => own.text.wrap_mode,
                None => winkin::style::TextWrapMode::NoWrap,
            },
            ..own.text
        },
        ..own
    };
    let masked = own.text.security != winkin::style::TextSecurity::None;
    let mapped = masked || own.text.transform != winkin::style::TextGroup::INITIAL.transform;
    let block = style::block_style(&own, None, computed, computed, &text.calcs, scale);
    let key = NodeKey(node);
    text.built = false;
    text.laid = false;
    text.boxes.clear();
    text.basis = 0.0;
    text.scale = scale;
    text.writing_mode = block.writing_mode;
    text.styles.first_line.clear();
    text.styles.first_letter = None;
    text.styles.marker = None;
    text.styles.marks.clear();
    let mut options = BuildOptions::default();
    options.map_source = mapped;
    let mut builder = text.layout.builder(&mut cx.cx, key, &block, options);
    let piece = |number: u64| NodeKey(node | number << PIECE_SHIFT);
    // The piece in the middle, in a box of its own: the composing text, which the renderer
    // underlines, or the text shown in the clear.
    let middle = match (composition, revealed.filter(|_| masked)) {
        (Some(range), _) => Some((
            range,
            NodeKey(COMPOSE_KEY | node),
            ComputedStyle {
                decorates: true,
                ..unboxed(own)
            },
        )),
        (None, Some(range)) => Some((
            range,
            NodeKey(PIECE_KEYS | node),
            ComputedStyle {
                text: winkin::style::TextGroup {
                    security: winkin::style::TextSecurity::None,
                    ..own.text
                },
                ..unboxed(own)
            },
        )),
        (None, None) => None,
    };
    match middle {
        Some((range, box_key, box_style)) => {
            for (number, part, boxed) in [
                (0, &content[..range.start], false),
                (1, &content[range.clone()], true),
                (2, &content[range.end..], false),
            ] {
                if boxed {
                    builder.open_box(box_key, &box_style, None);
                }
                if !part.is_empty() {
                    builder.text(piece(number), part);
                }
                if boxed {
                    builder.close_box();
                }
            }
        }
        None => builder.text(key, content),
    }
    builder.finish();
    text.built = true;
    let area = Area::new(width.unwrap_or(f32::MAX / 4.0));
    text.lay_out(&mut cx.cx, area, &mut winkin::NoExclusions);
    mapped
}

impl<S: TextStyle> MarkerEngine<S> for TextLayout<S> {
    type Context = TextContext;
    type Lines = TextLayout<S>;

    fn build(
        cx: &mut TextContext,
        key: blitz_text::NodeKey,
        computed: &S,
        marker: &Marker,
        bullet: bool,
        scale: f32,
    ) -> Self {
        let mut text = TextLayout {
            scale,
            ..TextLayout::default()
        };
        let marker_key = NodeKey(MARKER_KEY | key.0);
        text.styles.marker = Some(computed.clone());
        let mut lists = style::FontLists::of(computed, &());
        if bullet {
            lists = lists.with_families_first(&BULLET_FAMILIES);
        }
        let own = unboxed(style::computed_style(
            &lists,
            computed,
            &text.calcs,
            scale,
            None,
        ));
        let block = ComputedBlockStyle {
            writing_mode: WritingMode::HorizontalTb,
            ..ComputedBlockStyle::new(&own)
        };
        let mut builder =
            text.layout
                .builder(&mut cx.cx, marker_key, &block, BuildOptions::default());
        match marker {
            Marker::Char(char) => builder.text(marker_key, char.encode_utf8(&mut [0; 4])),
            Marker::String(string) => builder.text(marker_key, string),
        }
        builder.finish();
        text.built = true;
        text.lay_out(
            &mut cx.cx,
            Area::new(f32::MAX / 4.0),
            &mut winkin::NoExclusions,
        );
        text
    }
    fn paint<K: TextSink, H: PaintHost<S>>(
        &self,
        sink: &mut K,
        host: &H,
        scratch: &std::cell::RefCell<PaintScratch>,
        marker: &Marker,
        lines: Option<&TextLayout<S>>,
        frame: &MarkerFrame,
    ) {
        paint::paint_marker(sink, marker, self, lines, host, scratch, frame);
    }
}
