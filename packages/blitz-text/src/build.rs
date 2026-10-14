//! Building an inline formatting context's content: what a host pushes into a backend's builder,
//! and the host's side of a build.

use crate::NodeKey;
use crate::style::{Clear, TextStyle};

/// What a box opened in an inline formatting context's content is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanKind {
    /// A non-atomic inline box.
    Span,
    /// A `display: contents` element, which has no box but whose children inherit from it, or an
    /// annotation container that holds `<rt>`s of its own.
    Contents,
    /// A ruby container, `<ruby>`, where the backend sets ruby.
    Ruby,
    /// A ruby annotation, `<rt>` or an `<rtc>` holding no `<rt>`, where the backend sets ruby.
    /// `in_container` where it is an `<rt>` inside an `<rtc>`.
    Annotation { in_container: bool },
}

/// The side of the block a float goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatSide {
    Left,
    Right,
}

/// A list item's marker: a bullet, set in the bullet font, or a string.
#[derive(Debug, PartialEq, Clone)]
pub enum Marker {
    /// A bullet character.
    Char(char),
    /// A counter or a `list-style-type` string.
    String(String),
}

/// The content of one inline formatting context as a backend builds it, pushed in content order
/// by the host's walk of its document.
///
/// Every box opened is closed. Keys name the host's nodes; the backend hands them back when the
/// content is read and painted.
pub trait InlineBuilder<S: TextStyle> {
    /// Whether the backend applies `text-transform` itself. Where it does, text is pushed as the
    /// document holds it; where it does not, the host transforms it first, reading
    /// [`pushed_text`](Self::pushed_text) for context.
    const TRANSFORMS_TEXT: bool;

    /// Whether the backend sets ruby. Where it does, `<ruby>`, `<rt>` and `<rtc>` are opened as
    /// ruby and the fallback `<rp>` is dropped; where it does not, they are spans.
    const SETS_RUBY: bool;

    /// Whether the backend reads each box's style on the block's first formatted line. Where it
    /// does, and the block has a `::first-line`, the host hands it to
    /// [`open_box`](Self::open_box).
    const READS_FIRST_LINE: bool;

    /// Whether the backend reads the `lang` attributes of the elements pushed. Where it does
    /// not, the host hands it none.
    const READS_LANG: bool;

    /// Returns the text pushed so far, where the backend keeps it.
    fn pushed_text(&self) -> &str;

    /// Returns whether collapsible white space is pending at the end of the pushed text, which
    /// ends a word.
    fn has_pending_whitespace(&self) -> bool;

    /// Pushes the marker of a list item whose marker is inside it, before its content.
    fn marker(&mut self, marker: &Marker);

    /// Opens a box for the element `key`, set in `style`, and on the block's first formatted
    /// line in `first_line`. `lang` is the element's own `lang` attribute, where it has one.
    fn open_box(
        &mut self,
        key: NodeKey,
        style: &S,
        kind: SpanKind,
        first_line: Option<&S>,
        lang: Option<&str>,
    );

    /// Closes the box opened last, which was opened as `kind`.
    fn close_box(&mut self, kind: SpanKind);

    /// Returns whether the backend looks for the block's `::first-letter` in the text pushed
    /// next.
    fn wants_first_letter(&self) -> bool {
        false
    }

    /// Sets the style the block's `::first-letter` has in the box the text pushed next is in,
    /// and its style there on the first formatted line, where the block has a `::first-line`.
    fn first_letter(&mut self, style: &S, first_line: Option<&S>) {
        let _ = (style, first_line);
    }

    /// Pushes the text of the text node `key`, whose parent is the element `parent`.
    fn text(&mut self, key: NodeKey, parent: NodeKey, text: &str);

    /// Pushes the atomic inline `key`, set in `style`. `lang` is its own `lang` attribute.
    fn atomic(&mut self, key: NodeKey, style: &S, lang: Option<&str>);

    /// Pushes the float `key`, set in `style`, which goes to `side`. `lang` is its own `lang`
    /// attribute.
    fn float(&mut self, key: NodeKey, style: &S, side: FloatSide, lang: Option<&str>);

    /// Pushes the absolutely positioned box `key`, set in `style`, whose static position the lines
    /// find: inline-level where its display was inline before it was positioned.
    fn absolute(&mut self, key: NodeKey, style: &S, inline_level: bool);

    /// Pushes the forced line break of the `<br>` element `key`, set in `style`, which clears the
    /// floats on `clear`.
    fn line_break(&mut self, key: NodeKey, style: &S, clear: Option<Clear>);

    /// Pushes the line break opportunity of the `<wbr>` element `key`, and returns whether the
    /// backend took it. Where it did not, the element is pushed as a span.
    fn break_opportunity(&mut self, key: NodeKey) -> bool;
}

/// The block an inline formatting context belongs to, as the host styles it.
#[derive(Clone, Debug)]
pub struct BlockInputs<'a, S> {
    /// The style the content is set in: the block's, or for an anonymous block, the style of
    /// the box around it.
    pub style: S,
    /// The box whose overflow decides whether `text-overflow` applies: the block, or for an
    /// anonymous block, the box around it.
    pub container: S,
    /// Whether the block is anonymous.
    pub anonymous: bool,
    /// The block's `::first-line` style, where it has one.
    pub first_line: Option<S>,
    /// The `lang` of the block or its nearest ancestor that has one.
    pub language: Option<&'a str>,
    /// The direction the block's lines run in where it differs from its style's: the body's
    /// direction, which CSS propagates to the root element.
    pub direction: Option<crate::style::Direction>,
    /// Whether the block comes after another box of the element it is in, so that it holds no
    /// first formatted line of that element.
    pub follows_box: bool,
}

/// The block of a document's `@font-feature-values` rules a name is looked up in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureBlock {
    Stylistic,
    Swash,
    Ornaments,
    Annotation,
    Styleset,
    CharacterVariant,
}

/// A document's `@font-feature-values` rules, which name the values `font-variant-alternates`
/// asks for.
pub trait FontFeatureValues {
    /// Returns whether there are no rules.
    fn is_empty(&self) -> bool;

    /// Returns whether a rule names the family `family`, ignoring ASCII case.
    fn names_family(&self, family: &str) -> bool;

    /// Returns the values the last rule for `family` that defines `name` in `block` gives it.
    fn find(&self, family: &str, block: FeatureBlock, name: &str) -> Option<Vec<u32>>;
}

/// No `@font-feature-values` rules.
impl FontFeatureValues for () {
    fn is_empty(&self) -> bool {
        true
    }

    fn names_family(&self, _family: &str) -> bool {
        false
    }

    fn find(&self, _family: &str, _block: FeatureBlock, _name: &str) -> Option<Vec<u32>> {
        None
    }
}

/// A host's inline formatting contexts, which a backend builds from: each one's block, and its
/// content, pushed into the backend's builder.
pub trait InlineSource<S: TextStyle>: Sync {
    /// Returns the block the inline formatting context rooted at `root` belongs to, or `None`
    /// where nothing styles it.
    fn block(&self, root: NodeKey) -> Option<BlockInputs<'_, S>>;

    /// Returns the style the content of the inline formatting context rooted at `root` is set
    /// in, as [`BlockInputs::style`] has it.
    fn block_style(&self, root: NodeKey) -> Option<S>;

    /// Returns the computed style of `key`, where it has one.
    fn style(&self, key: NodeKey) -> Option<S>;

    /// Pushes the content of the inline formatting context rooted at `root` into `builder`: an
    /// inside list marker, then its boxes and text in content order.
    fn push_content<B: InlineBuilder<S>>(&self, root: NodeKey, builder: &mut B);

    /// Returns the document's `@font-feature-values` rules.
    fn font_feature_values(&self) -> impl FontFeatureValues + '_;
}
