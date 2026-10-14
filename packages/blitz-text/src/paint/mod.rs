//! Painting laid-out text: what a backend hands a renderer ([`TextSink`]), and what it asks of
//! the host's document while it paints ([`PaintHost`]).

mod decoration;

use kurbo::{Affine, Insets, Rect, Vec2};
use linebender_resource_handle::FontData;

pub use decoration::{
    DecorationRunGeometry, DecorationStackEntry, LineDecoration, ResolvedDecoration,
    composition_underline, decoration_size, flush_line_decorations,
};

use crate::NodeKey;
use crate::style::{Color, TextDecorationStyle, TextStyle};

/// A glyph, positioned in the run's coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
}

/// A run of glyphs in one font, as a renderer draws it.
#[derive(Clone, Copy, Debug)]
pub struct GlyphRun<'a> {
    pub font: &'a FontData,
    /// The font size, in device pixels.
    pub size: f32,
    /// Whether the outlines are hinted.
    pub hint: bool,
    /// The font's normalized variation coordinates.
    pub coords: &'a [i16],
    /// How far each outline is grown, for a synthesized bold.
    pub embolden: Vec2,
    pub color: Color,
    /// Takes the glyphs' positions onto the scene.
    pub transform: Affine,
    /// Applied to each glyph's outline about its origin, for a synthesized oblique.
    pub glyph_transform: Option<Affine>,
}

/// One decoration line: an underline, an overline or a line through, in a style, spanning part of
/// a line.
///
/// It is drawn from `x0` to `x0 + width`, inset by `inset_start` and `inset_end`, between the ends
/// of `clip`; `y = baseline - offset + size / 2` is its middle, growing downward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecorationLine {
    pub transform: Affine,
    /// Device pixels per CSS pixel.
    pub scale: f64,
    pub x0: f64,
    pub width: f64,
    pub baseline: f32,
    pub offset: f32,
    /// Its thickness.
    pub size: f32,
    pub color: Color,
    /// The way from the text a `double` line's second line goes: `1.0` down, `-1.0` up.
    pub double_dir: f64,
    pub style: TextDecorationStyle,
    pub inset_start: f64,
    pub inset_end: f64,
    /// Only what lies between these is drawn: of a solid or double line, that part; of a pattern,
    /// the dots centred and the dashes and waves starting there.
    pub clip: (f64, f64),
}

/// A stretch of an inline box on one line: its background, borders and outline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InlineBoxPaint {
    /// Takes the box's coordinates onto the scene.
    pub transform: Affine,
    /// Its border box.
    pub border_box: Rect,
    /// Its border widths: no border on a side where the box goes on past it.
    pub widths: Insets,
    /// The radii of its top left, top right, bottom right and bottom left corners.
    pub radii: [Vec2; 4],
    pub background: Color,
    /// Its border colors on the top, right, bottom and left.
    pub colors: [Color; 4],
    /// Its outline's width and color, where it has one.
    pub outline: Option<(f64, Color)>,
}

/// Where a backend paints laid-out text: a renderer's scene, and the document's atomic inlines.
pub trait TextSink {
    /// Draws `glyphs` as `run` says.
    fn glyphs(&mut self, run: &GlyphRun<'_>, glyphs: impl Iterator<Item = Glyph> + Clone);

    /// Fills `rect` with `color`.
    fn fill(&mut self, transform: Affine, rect: Rect, color: Color);

    /// Fills `rect` with the selection highlight.
    fn selection(&mut self, transform: Affine, rect: Rect);

    /// Draws a decoration line.
    fn decoration(&mut self, line: &DecorationLine);

    /// Draws a stretch of an inline box.
    fn inline_box(&mut self, inline_box: &InlineBoxPaint);

    /// Starts drawing into a layer blurred by `deviation` device pixels, within `clip`, which
    /// `transform` takes onto the scene.
    fn push_blur(&mut self, transform: Affine, clip: Rect, deviation: f32);

    /// Ends the layer [`push_blur`](Self::push_blur) started.
    fn pop_blur(&mut self);

    /// Paints the atomic inline `key` where the line paints it, after what comes before it in
    /// tree order and under what comes after.
    fn atomic(&mut self, key: NodeKey);

    /// Returns whether the renderer grows every glyph's outline and leaves it unhinted, as some
    /// platforms draw text, where the backend leaves that to it.
    fn emboldens(&self) -> bool {
        false
    }
}

/// What a backend asks of the host's document while it paints.
pub trait PaintHost<S: TextStyle> {
    /// Returns the computed style of `key`: an element's own, or a text node's element's.
    fn style(&self, key: NodeKey) -> Option<S>;

    /// Returns the node `key` is in.
    fn parent(&self, key: NodeKey) -> Option<NodeKey>;

    /// Returns the element `key` is or is in: a text node's parent, or the element itself.
    fn element(&self, key: NodeKey) -> Option<NodeKey>;

    /// Returns how far `position: relative` moves what `key` paints, in CSS pixels: the offsets
    /// of its inline box and every inline box it is in, up to the block the lines belong to.
    /// `containing` is the block's content box in CSS pixels, which percentages are of, and
    /// `rtl` its direction.
    fn relative_shift(&self, key: NodeKey, containing: kurbo::Size, rtl: bool) -> Vec2;

    /// Returns the styles of the boxes around `root` whose decorations propagate to its inline
    /// content, the outermost first.
    fn propagated_decorations(&self, root: NodeKey) -> Vec<S>;
}

/// Where an inline formatting context is painted.
#[derive(Clone, Copy, Debug)]
pub struct PaintFrame {
    /// Takes the content box's device pixels, moved down by the layout's block offset, onto the
    /// scene.
    pub transform: Affine,
    /// The content box's size, in device pixels.
    pub content_size: kurbo::Size,
    /// Device pixels per CSS pixel.
    pub scale: f64,
    /// The inline formatting context's root.
    pub root: NodeKey,
    /// The selected text, as byte offsets of the laid-out text.
    pub selection: Option<(usize, usize)>,
}

/// What painting keeps between uses: buffers that are cleared without releasing their
/// allocations.
#[derive(Default)]
pub struct PaintScratch {
    /// The decorating boxes on the path from the inline root to the run being painted.
    pub stack: Vec<DecorationStackEntry>,
    /// A buffer for that path.
    pub path: Vec<NodeKey>,
    /// The decorations of a line, one for each decorating box.
    pub deco_boxes: Vec<LineDecoration>,
    /// Each font's OS/2 `usWinAscent / unitsPerEm`, keyed by its data's id and its index.
    pub win_ascent_ratios: decoration::WinAscentCache,
}
