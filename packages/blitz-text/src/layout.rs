//! Laying out an inline formatting context, and reading it back.

use core::cell::RefCell;
use core::ops::Range;

use kurbo::Rect;

use crate::NodeKey;
use crate::build::{FloatSide, InlineSource};
use crate::paint::{PaintFrame, PaintHost, PaintScratch, TextSink};
use crate::style::{Sides, TextStyle};

/// A width and a height, in the units the method names.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

/// A piece of an inline formatting context's laid-out content, in logical order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentItem {
    /// Text of node `key`: the bytes `range` of the laid-out text. The node is the text node, or
    /// the element it is in, as the backend maps the text.
    Text { key: NodeKey, range: Range<usize> },
    /// An inline box.
    Box(NodeKey),
}

/// A position in an inline formatting context's laid-out text, and the node it is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextHit {
    /// The node the position is in.
    pub key: NodeKey,
    /// The byte offset in the laid-out text, or where the backend maps its text, in the node's.
    pub offset: usize,
}

/// An inline formatting context, laid out and read back.
///
/// Positions in the layout's text are byte offsets into the backend's laid-out text, which
/// [`text_len`](Self::text_len) bounds; [`source_offset`](Self::source_offset) maps a text node's
/// own offsets into it where the backend can.
pub trait InlineText: Default + Clone + core::fmt::Debug {
    /// Returns device pixels per CSS pixel that the layout was built at.
    fn scale(&self) -> f32;

    /// Returns how far `align-content` moves the line boxes down the content box, in CSS pixels.
    fn block_offset(&self) -> f32;

    /// Returns the laid-out text, which the layout's byte offsets index.
    fn text(&self) -> &str;

    /// Returns the byte length of the laid-out text.
    fn text_len(&self) -> usize;

    /// Returns the text between byte offsets `start` and `end`, as it is copied, in the pieces of
    /// the laid-out text it is made of. Nothing where the range is not within the text.
    fn selected_text(&self, start: usize, end: usize) -> impl Iterator<Item = &str>;

    /// Returns the laid-out text and inline boxes in logical order, as the lines place them, read
    /// as it is walked. Each stretch of text one node holds comes as one item, whose range
    /// indexes [`text`](Self::text), and each inline box comes once.
    fn logical_content(&self) -> impl Iterator<Item = ContentItem>;

    /// Returns where byte `offset` of the text node `key`'s content falls in the laid-out text,
    /// where [`maps_source`](Self::maps_source).
    fn source_offset(&self, key: NodeKey, offset: usize) -> Option<usize>;

    /// Returns whether the backend maps a text node's offsets into the laid-out text. Where it
    /// does not, hosts follow white space collapsing and text transforms themselves.
    fn maps_source(&self) -> bool;

    /// Returns the baseline of the first line, in CSS pixels below the top of the inline root's
    /// content box, or `None` where the layout has no lines.
    fn first_baseline(&self) -> Option<f32>;

    /// Returns the text position at physical CSS-pixel coordinates relative to the inline root's
    /// content box. `exact` requires the point to be within a line and its text.
    fn hit_test(&self, x: f32, y: f32, exact: bool) -> Option<TextHit>;

    /// Calls `f` with each rectangle that highlights the text between `start` and `end`, in
    /// device pixels relative to the inline root's content box moved down by
    /// [`block_offset`](Self::block_offset).
    fn for_each_selection_rect(&self, start: usize, end: usize, f: impl FnMut(Rect));

    /// Returns the boxes the non-atomic inline element `key` has on each line, first line first,
    /// in CSS pixels relative to the content box's top left, moved down by
    /// [`block_offset`](Self::block_offset).
    ///
    /// `content` is the content box's size in device pixels. `contains` answers whether a node
    /// is `key` or inside it, for backends that keep only each run's innermost node.
    fn fragment_rects(
        &self,
        key: NodeKey,
        content: kurbo::Size,
        contains: impl Fn(NodeKey) -> bool,
    ) -> impl Iterator<Item = Rect>;

    /// Prints the layout's lines and what they hold, for debugging.
    fn debug_print(&self);
}

/// What a pass of layout needs of an inline formatting context's atomic inlines and floats,
/// which says how far each is laid out to measure it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Measure {
    /// How far each reaches along the line: an intrinsic inline-size pass.
    InlineSizes,
    /// Each one's size, and the baselines the lines read: a pass that sets the lines to size the
    /// block but places nothing.
    Sizes,
    /// Each one laid out, as the pass that places them lays them out.
    Layout,
}

/// An atomic inline or a float as the host measured it, which the lines are broken with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxMeasure {
    /// The box's node.
    pub key: NodeKey,
    /// Its border box, in CSS pixels.
    pub size: Size,
    /// Its margins, in CSS pixels.
    pub margin: Sides<f32>,
    /// Its baseline down from its border box's top, in CSS pixels, where it has one.
    pub baseline: Option<f32>,
    /// The side it floats to, where it is a float.
    pub float: Option<FloatSide>,
}

/// What the content of an inline formatting context is readied with.
#[derive(Clone, Copy, Debug)]
pub struct LinesInputs<'a> {
    /// The atomic inlines and floats as the host measured them, in content order.
    pub sizes: &'a [BoxMeasure],
    /// What the pass measured them for.
    pub pass: Measure,
    /// The containing block's inline size in CSS pixels, which percentages are of.
    pub basis: f32,
    /// Whether the lines run down the page, so that what the host measured across the page
    /// reaches along them.
    pub vertical: bool,
}

/// The room an inline formatting context's lines are broken in, in device pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineArea {
    /// How long a line is where no float is in the way.
    pub width: f32,
    /// How much room over the first line its ruby annotations and emphasis marks may take before
    /// the line moves down for them.
    pub room_above: f32,
    /// Where the content box ends, which `line-clamp: auto` keeps the lines within.
    pub block_end: Option<f32>,
    /// Whether the pass only sizes the block, so that its lines need no positions.
    pub sizes_only: bool,
    /// The layout pass, which a block placed in it is placed through to its end.
    pub pass: u64,
}

/// Which way an inline formatting context's lines run, and the way they stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineFlow {
    /// Across the page, stacking down it.
    Horizontal,
    /// Down the page, stacking from the right.
    VerticalRl,
    /// Down the page, stacking from the left.
    VerticalLr,
    /// Down the page with sideways glyphs, stacking from the right.
    SidewaysRl,
    /// Up the page, stacking from the left.
    SidewaysLr,
}

impl LineFlow {
    /// Returns whether the lines run down or up the page.
    pub fn is_vertical(self) -> bool {
        self != Self::Horizontal
    }

    /// Returns whether the lines stack from the right.
    pub fn stacks_from_right(self) -> bool {
        matches!(self, Self::VerticalRl | Self::SidewaysRl)
    }
}

/// Where a block container's last line box puts its baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LastBaseline {
    /// This far down.
    At(f32),
    /// It has no line box.
    None,
    /// Something in it the walk does not read, which the host answers for.
    Unknown,
}

/// The min-content and max-content widths of an inline formatting context's content.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContentWidths {
    pub min: f32,
    pub max: f32,
}

/// How far an inline formatting context's lines reach, and where their baselines are, in device
/// pixels from the content box's top left.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LinesExtent {
    /// How far down the line boxes reach.
    pub height: f32,
    /// How far along the longest line reaches.
    pub width: f32,
    /// The first line's baseline, where there is a line.
    pub first_baseline: Option<f32>,
    /// The last line's baseline, where there is a line.
    pub last_baseline: Option<f32>,
}

/// Where the lines put an inline box, in device pixels from the content box's top left. Its style
/// says which of the positions apply.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// The box's node.
    pub key: NodeKey,
    /// Where an atomic inline's margin box's left edge is, or where an inline-level absolutely
    /// positioned box would have been along its line.
    pub x: f32,
    /// Where an atomic inline's margin box's top edge is.
    pub top: f32,
    /// Where the line the box is on reaches from and to.
    pub line_top: f32,
    pub line_bottom: f32,
    /// Where a block-level absolutely positioned box would have started.
    pub block_start: f32,
    /// Whether an absolutely positioned box's static position faces right to left across the
    /// page, where the lines say: its inline start's way in a horizontal block, and its block
    /// start's in a vertical one.
    pub rtl: Option<bool>,
}

/// A float the lines reached, to be placed: its margin box's size and the line's top, in device
/// pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloatRequest {
    /// The key the content holds the float by.
    pub key: u64,
    pub side: FloatSide,
    pub inline_size: f32,
    pub block_size: f32,
    /// The top of the line that reached it, which it goes no higher than.
    pub block_start: f32,
    /// Whether it is the room a block's initial letter takes rather than a float.
    pub initial_letter: bool,
}

/// Where a float was placed: its margin box, in device pixels from the content box's top left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedFloat {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

/// The floats an inline formatting context's lines flow around, which also places the floats its
/// text reaches. Positions are in device pixels from the content box's top left.
pub trait LineExclusions {
    /// Returns the left and right edges of the room a line has that reaches across the block from
    /// `start` to `end`.
    fn band(&self, start: f32, end: f32) -> (f32, f32);

    /// Returns the next position below `top` where the room changes, or `None` where it never
    /// does.
    fn below(&self, top: f32) -> Option<f32>;

    /// Places a float, which later bands account for, and returns where it went.
    fn place(&mut self, float: FloatRequest) -> PlacedFloat;

    /// Returns a checkpoint that [`rewind`](Self::rewind) takes the placements back to.
    fn checkpoint(&self) -> usize;

    /// Takes back the floats placed after `to`, as a trial break that is rejected does.
    fn rewind(&mut self, to: usize);
}

/// No floats.
impl LineExclusions for () {
    fn band(&self, _start: f32, _end: f32) -> (f32, f32) {
        (0.0, f32::MAX)
    }

    fn below(&self, _top: f32) -> Option<f32> {
        None
    }

    fn place(&mut self, float: FloatRequest) -> PlacedFloat {
        PlacedFloat {
            left: 0.0,
            right: float.inline_size,
            top: float.block_start,
            bottom: float.block_start + float.block_size,
        }
    }

    fn checkpoint(&self) -> usize {
        0
    }

    fn rewind(&mut self, _to: usize) {}
}

/// Building, measuring, placing and painting an inline formatting context: the part of
/// [`InlineText`] a host's layout and paint run.
pub trait InlineLayout<S: TextStyle>: InlineText {
    /// What a document keeps for its text, which the content is built and broken with.
    type Context;

    /// Whether the backend sets lines in vertical writing modes itself, so that an inline
    /// formatting context runs in physical axes and meets the host's writing modes only at its
    /// edges.
    const SETS_WRITING_MODES: bool;

    /// Whether the backend's lines take room over the first line for ruby annotations and
    /// emphasis marks, which the blocks before lend them: then [`LineArea::room_above`] is
    /// worked out from those blocks, and [`room_below`](Self::room_below) read from them.
    const READS_ROOM_ABOVE: bool;

    /// Whether the backend paints a box inside non-atomic inline boxes moved as their
    /// `position: relative` moves them, so that the host places atomic inlines and fragments
    /// moved the same way.
    const SHIFTS_RELATIVE_INLINES: bool;

    /// Builds the content of each inline formatting context in `layouts`, rooted at its key,
    /// from `source`. This is the deferred, possibly parallel, part of box construction.
    fn build_layouts<C: InlineSource<S>>(
        cx: &mut Self::Context,
        source: &C,
        scale: f32,
        layouts: &mut [(NodeKey, Box<Self>)],
    );

    /// Returns whether an element's style changing from `old` to `new` changes what the backend
    /// built from it, beyond what the host rebuilds for: then the inline formatting context it
    /// is in is built again.
    ///
    /// The styles may be of another type than the ones the content is built from: a style
    /// engine may compare styles it does not keep.
    fn style_change_rebuilds<T: TextStyle>(old: &T, new: &T) -> bool {
        let _ = (old, new);
        false
    }

    /// Returns whether the content's first line may take room above it, where the backend reads
    /// room above: it holds ruby annotations or emphasis marks. Without them the room is not
    /// worked out.
    fn takes_room_above(&self) -> bool {
        Self::READS_ROOM_ABOVE
    }

    /// Returns whether the content is known to hold no text and no inline boxes before it is
    /// laid out.
    fn is_empty(&self) -> bool;

    /// Readies the content of the inline formatting context rooted at `root` to be broken into
    /// lines at `scale`, with its atomic inlines and floats as `lines` says the host measured
    /// them, building it again from `source` where it is out of date. `text-indent` percentages
    /// count as zero until the lines are broken, and are then of the width they are broken in.
    fn prepare<C: InlineSource<S>>(
        &mut self,
        cx: &mut Self::Context,
        source: &C,
        root: NodeKey,
        scale: f32,
        lines: LinesInputs<'_>,
    );

    /// Returns the content's min-content and max-content widths, in device pixels, with its
    /// floats: the widest of them is as wide as the content gets at min-content, and at
    /// max-content each paragraph is as wide as its text and the floats anchored in it.
    fn content_widths(&mut self) -> ContentWidths;

    /// Breaks the content into lines in `area`, or as narrow as `exclusions` leaves them, placing
    /// each float into it as a line reaches it, and aligns them as `style`, the inline root's
    /// computed style, says.
    fn break_lines(
        &mut self,
        cx: &mut Self::Context,
        area: LineArea,
        style: Option<&S>,
        exclusions: &mut impl LineExclusions,
    );

    /// Returns how far the lines reach and where their baselines are, in device pixels from the
    /// content box's top, where `end_padding` device pixels of padding after the last line let
    /// what reaches past it in.
    fn extent(&self, end_padding: f32) -> LinesExtent;

    /// Sets how far `align-content` moves the line boxes down the content box, in CSS pixels, and
    /// the size of the content box the lines are placed in, in device pixels.
    fn set_frame(&mut self, block_offset: f32, content: Size);

    /// Returns where the lines put each atomic inline and the static position of each absolutely
    /// positioned box, in the order they are on the lines, on the page: lines that run down the
    /// page are turned onto the content box [`set_frame`](Self::set_frame) gave.
    fn placements(&self) -> impl Iterator<Item = Placement>;

    /// Returns which way the lines run, as the content was last built.
    fn line_flow(&self) -> LineFlow;

    /// Returns where a box that reaches `along` the lines from `along[0]` to `along[1]` and
    /// across them from `across[0]` to `across[1]` has its top left on the page, in device
    /// pixels.
    fn place_on_page(&self, along: [f32; 2], across: [f32; 2]) -> (f32, f32);

    /// Returns where the last line put its baseline when the lines were last broken, in device
    /// pixels from the content box's top, before `align-content` moved them.
    fn last_line_baseline(&self) -> LastBaseline;

    /// Returns how far below the last line its ruby annotations and emphasis marks leave room
    /// for the block after, in device pixels; negative where they reach past it.
    fn room_below(&self) -> f32;

    /// Returns the node of the float the content holds by `key`, where it is a node's.
    fn float_node(key: u64) -> Option<NodeKey>;

    /// Paints the content: its inline boxes' backgrounds and borders, the selection highlight,
    /// its text and decorations, and each atomic inline where its line paints it, into `sink`.
    fn paint<K: TextSink, H: PaintHost<S>>(
        &self,
        sink: &mut K,
        host: &H,
        scratch: &RefCell<PaintScratch>,
        frame: &PaintFrame,
    );
}
