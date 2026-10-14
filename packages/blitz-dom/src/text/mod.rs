//! Text layout, text editing and fonts, through blitz-text's interface to a text backend.
//!
//! Blitz lays out text with one backend, chosen when it is built: `blitz-text-parley` (the
//! `parley` feature) or `blitz-text-winkin` (the `winkin` feature); with both, Parley. The
//! backends implement [`blitz_text`]'s traits over [`StyloStyle`], Stylo's computed values as
//! blitz-text reads them, and the type aliases here name the types of the backend in use, so the
//! rest of Blitz is written once, against the aliases and the traits.
//!
//! - [`FontContext`] ([`TextFonts`]): the fonts documents lay out text with.
//! - [`TextLayout`] ([`InlineText`]): an inline formatting context, built from the DOM, measured,
//!   broken into lines and placed, and read back for hit testing, selection and `innerText`.
//! - [`TextEditor`] ([`EditableText`]): the text of an `<input>` or `<textarea>`, and its editing.
//! - [`MarkerLayout`]: an outside list marker.
//! - [`FaceDescriptors`]: an `@font-face` rule's descriptors, which a backend registers a face by.
//!
//! What reads the DOM or Stylo stays here: the walk of an inline formatting context's content,
//! the cascades for `::first-line` and `::first-letter`, the font metrics Stylo resolves
//! font-relative units with, and `@font-face` rules.

#[cfg(not(any(text_parley, text_winkin)))]
compile_error!("Enable one of the `parley` or `winkin` features");

use std::sync::Arc;

pub use blitz_text;
pub use blitz_text::parlance;
pub(crate) use blitz_text::{
    BoxMeasure, ContentWidths, FloatRequest, FloatSide, InlineBuilder, InlineLayout, LastBaseline,
    LineArea, LineExclusions, LineFlow, LinesExtent, LinesInputs, Measure, PlacedFloat, Placement,
    SpanKind,
};
pub use blitz_text::{
    Edit, EditableText, EditorMetrics, FaceDescriptors, InlineText, Motion, NodeKey, TextFonts,
};
use blitz_traits::net::Bytes;
use blitz_traits::node_id::NodeId;
use parlance::{FontFeature, FontStyle, FontVariation, FontWeight, FontWidth, Tag};

mod stylo;
pub use crate::layout::inline::relative_shift;
pub use stylo::{StyloStyle, StyloStyleRef};

#[cfg(text_parley)]
pub use blitz_text_parley as parley;
#[cfg(text_winkin)]
pub use blitz_text_winkin as winkin;

/// The text backend in use.
#[cfg(text_parley)]
pub type Backend = blitz_text_parley::Parley;
/// The text backend in use.
#[cfg(text_winkin)]
pub type Backend = blitz_text_winkin::Winkin;

/// The fonts documents lay out text with. Clones share their fonts.
///
/// This is the text backend's own type, and its methods beyond [`TextFonts`] are the backend's:
/// code written for every backend uses [`TextFonts`].
pub type FontContext = <Backend as blitz_text::Backend<StyloStyle>>::Fonts;
/// An inline formatting context, as the text backend lays it out.
pub type TextLayout = <Backend as blitz_text::Backend<StyloStyle>>::Layout;
/// The text of an `<input>` or `<textarea>`, as the text backend edits and lays it out.
pub type TextEditor = <Backend as blitz_text::Backend<StyloStyle>>::Editor;
/// An outside list marker, as the text backend lays it out.
pub type MarkerLayout = <Backend as blitz_text::Backend<StyloStyle>>::Marker;
/// What a document keeps for its text: its fonts, and the backend's contexts.
pub(crate) type TextContext = <Backend as blitz_text::Backend<StyloStyle>>::Context;

/// The key a node is handed to the text backend by.
#[inline]
pub fn node_key(id: NodeId) -> NodeKey {
    NodeKey(id.as_u64())
}

/// The node a text backend's key names.
#[inline]
pub fn key_node(key: NodeKey) -> NodeId {
    NodeId::from_u64(key.0)
}

/// The text backends Blitz can be built with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextBackend {
    /// [Parley](https://github.com/linebender/parley), the `parley` feature.
    Parley,
    /// [winkin](https://github.com/dfrg/winkin), the `winkin` feature.
    Winkin,
}

/// The text backend this build of Blitz lays out text with.
#[cfg(text_parley)]
pub const BACKEND: TextBackend = TextBackend::Parley;
/// The text backend this build of Blitz lays out text with.
#[cfg(text_winkin)]
pub const BACKEND: TextBackend = TextBackend::Winkin;

/// Expands the arm for the text backend in use, and drops the others, so that a crate which
/// holds code for each backend compiles the one blitz-dom was built with.
///
/// Each arm is a backend's name and its tokens in braces. The arms of backends other than the one
/// in use are dropped, whether or not this build knows them, so callers can list every backend
/// they hold code for.
///
/// ```ignore
/// blitz_dom::cfg_text_backend! {
///     parley => { mod parley; use parley as backend; }
///     winkin => { mod winkin; use winkin as backend; }
/// }
/// ```
#[macro_export]
macro_rules! cfg_text_backend {
    ($($arms:tt)*) => {
        $crate::__text_backend_arms! { $($arms)* }
    };
}

/// Expands the arm of [`cfg_text_backend!`] named for the text backend in use.
#[cfg(text_parley)]
#[doc(hidden)]
#[macro_export]
macro_rules! __text_backend_arms {
    () => {};
    (parley => { $($tokens:tt)* } $($rest:tt)*) => {
        $($tokens)*
        $crate::__text_backend_arms! { $($rest)* }
    };
    ($other:ident => { $($tokens:tt)* } $($rest:tt)*) => {
        $crate::__text_backend_arms! { $($rest)* }
    };
}

/// Expands the arm of [`cfg_text_backend!`] named for the text backend in use.
#[cfg(text_winkin)]
#[doc(hidden)]
#[macro_export]
macro_rules! __text_backend_arms {
    () => {};
    (winkin => { $($tokens:tt)* } $($rest:tt)*) => {
        $($tokens)*
        $crate::__text_backend_arms! { $($rest)* }
    };
    ($other:ident => { $($tokens:tt)* } $($rest:tt)*) => {
        $crate::__text_backend_arms! { $($rest)* }
    };
}

/// What a document keeps for its text, as Blitz drives it: blitz-text's `DocumentText`, with
/// Stylo's font metrics and `@font-face` rules.
pub(crate) trait DocumentText: Sized {
    /// The context for a document handed `fonts`, or the default fonts.
    fn new(fonts: Option<FontContext>) -> Self;

    /// The fonts the document was handed, as it hands them to its iframes.
    fn fonts(&self) -> FontContext;

    /// Adds a web font that has loaded, under its `@font-face` rule's descriptors.
    fn add_web_font(&mut self, bytes: Bytes, overrides: &crate::net::FontFaceOverrides);

    /// What Stylo measures font-relative units (`ex`, `ch`, `cap`, `ic`) with.
    fn font_metrics_provider(&self) -> Box<dyn style::device::servo::FontMetricsProvider>;
}

impl DocumentText for TextContext {
    #[inline]
    fn new(fonts: Option<FontContext>) -> Self {
        <Self as blitz_text::DocumentText>::new(fonts)
    }

    #[inline]
    fn fonts(&self) -> FontContext {
        <Self as blitz_text::DocumentText>::fonts(self)
    }

    fn add_web_font(&mut self, bytes: Bytes, overrides: &crate::net::FontFaceOverrides) {
        <Self as blitz_text::DocumentText>::add_web_font(
            self,
            bytes,
            overrides.family_name.as_deref(),
            &overrides.descriptors,
        );
    }

    fn font_metrics_provider(&self) -> Box<dyn style::device::servo::FontMetricsProvider> {
        Box::new(StyloFontMetrics(Arc::new(
            <Self as blitz_text::DocumentText>::font_metrics(self),
        )))
    }
}

/// A backend's font metrics, as Stylo asks for them.
struct StyloFontMetrics<M>(Arc<M>);

impl<M> core::fmt::Debug for StyloFontMetrics<M> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "StyloFontMetrics")
    }
}

impl<M: blitz_text::FontMetricsSource + Send + Sync + 'static>
    style::device::servo::FontMetricsProvider for StyloFontMetrics<M>
{
    fn query_font_metrics(
        &self,
        vertical: bool,
        font_styles: &style::properties::style_structs::Font,
        font_size: style::values::computed::CSSPixelLength,
        _flags: style::values::computed::font::QueryFontMetricsFlags,
    ) -> style::font_metrics::FontMetrics {
        use blitz_text::style::FontSizeAdjust;
        use style::values::computed::CSSPixelLength;
        use style::values::generics::font::GenericFontSizeAdjust;
        let variations: Vec<FontVariation> = font_styles
            .font_variation_settings
            .0
            .iter()
            .map(|setting| FontVariation {
                tag: Tag::from_bytes(setting.tag.0.to_be_bytes()),
                value: setting.value,
            })
            .collect();
        let metrics = self.0.query(blitz_text::FontQuery {
            families: font_styles
                .font_family
                .families
                .list
                .iter()
                .map(stylo::font_family_name),
            weight: FontWeight::new(font_styles.font_weight.value()),
            width: FontWidth::from_percentage(font_styles.font_width.0.to_float()),
            style: stylo::font_style(font_styles.font_style),
            variations: &variations,
            size_adjust: match font_styles.font_size_adjust {
                GenericFontSizeAdjust::None => FontSizeAdjust::None,
                GenericFontSizeAdjust::ExHeight(value) => FontSizeAdjust::ExHeight(value.0),
                GenericFontSizeAdjust::CapHeight(value) => FontSizeAdjust::CapHeight(value.0),
                GenericFontSizeAdjust::ChWidth(value) => FontSizeAdjust::ChWidth(value.0),
                GenericFontSizeAdjust::IcWidth(value) => FontSizeAdjust::IcWidth(value.0),
                GenericFontSizeAdjust::IcHeight(value) => FontSizeAdjust::IcHeight(value.0),
            },
            size: font_size.px(),
            vertical,
        });
        style::font_metrics::FontMetrics {
            ascent: CSSPixelLength::new(metrics.ascent),
            x_height: metrics.x_height.map(CSSPixelLength::new),
            cap_height: metrics.cap_height.map(CSSPixelLength::new),
            zero_advance_measure: metrics.zero_advance.map(CSSPixelLength::new),
            ic_width: metrics.ic_width.map(CSSPixelLength::new),
            script_percent_scale_down: None,
            script_script_percent_scale_down: None,
        }
    }

    fn base_size_for_generic(
        &self,
        generic: style::values::computed::font::GenericFontFamily,
    ) -> style::values::computed::Length {
        let size = match generic {
            style::values::computed::font::GenericFontFamily::Monospace => 13.0,
            _ => 16.0,
        };
        style::values::computed::Length::from(app_units::Au::from_f32_px(size))
    }
}

/// The text backend's inline formatting contexts, as Blitz's layout drives them over Stylo's
/// styles and the DOM: [`InlineLayout`] for [`StyloStyle`], keyed by node.
pub(crate) trait InlineLayoutEngine: InlineText {
    /// Whether the backend sets lines in vertical writing modes itself.
    const SETS_WRITING_MODES: bool;
    /// Whether the backend paints what is inside inline boxes moved by their relative offsets.
    const SHIFTS_RELATIVE_INLINES: bool;

    /// Builds the content of each inline formatting context in `layouts` from the DOM.
    fn build_layouts(
        cx: &mut TextContext,
        source: &crate::layout::inline::DomSource<'_>,
        scale: f32,
        layouts: &mut [(NodeKey, Box<Self>)],
    );

    /// Whether an element's style changing from `old` to `new` rebuilds its inline formatting
    /// context.
    fn style_change_rebuilds(
        old: &style::properties::ComputedValues,
        new: &style::properties::ComputedValues,
    ) -> bool;

    /// Whether the content's first line may take room above it.
    fn takes_room_above(&self) -> bool;

    /// Whether the content is known to hold no text and no inline boxes.
    fn is_empty(&self) -> bool;

    /// Readies the content rooted at `root` to be broken into lines.
    fn prepare(
        &mut self,
        cx: &mut TextContext,
        source: &crate::layout::inline::DomSource<'_>,
        root: NodeId,
        scale: f32,
        lines: LinesInputs<'_>,
    );

    /// The content's min-content and max-content widths.
    fn content_widths(&mut self) -> ContentWidths;

    /// Breaks the content into lines.
    fn break_lines(
        &mut self,
        cx: &mut TextContext,
        area: LineArea,
        style: Option<&StyloStyle>,
        exclusions: &mut impl LineExclusions,
    );

    /// How far the lines reach.
    fn extent(&self, end_padding: f32) -> LinesExtent;

    /// Sets the block offset and the content box's size.
    fn set_frame(&mut self, block_offset: f32, content: taffy::Size<f32>);

    /// Where the lines put each atomic inline and absolutely positioned box.
    fn placements(&self) -> impl Iterator<Item = Placement>;

    /// Which way the lines run.
    fn line_flow(&self) -> LineFlow;

    /// Where a box along and across the lines has its top left on the page.
    #[cfg_attr(not(feature = "floats"), allow(dead_code))]
    fn place_on_page(&self, along: [f32; 2], across: [f32; 2]) -> (f32, f32);

    /// Where the last line put its baseline.
    fn last_line_baseline(&self) -> LastBaseline;

    /// How far below the last line its annotations leave room.
    fn room_below(&self) -> f32;

    /// The node of the float the content holds by `key`.
    #[cfg_attr(not(feature = "floats"), allow(dead_code))]
    fn float_node(key: u64) -> Option<NodeId>;
}

impl InlineLayoutEngine for TextLayout {
    const SETS_WRITING_MODES: bool = <Self as InlineLayout<StyloStyle>>::SETS_WRITING_MODES;
    const SHIFTS_RELATIVE_INLINES: bool =
        <Self as InlineLayout<StyloStyle>>::SHIFTS_RELATIVE_INLINES;

    #[inline]
    fn build_layouts(
        cx: &mut TextContext,
        source: &crate::layout::inline::DomSource<'_>,
        scale: f32,
        layouts: &mut [(NodeKey, Box<Self>)],
    ) {
        <Self as InlineLayout<StyloStyle>>::build_layouts(cx, source, scale, layouts);
    }

    #[inline]
    fn style_change_rebuilds(
        old: &style::properties::ComputedValues,
        new: &style::properties::ComputedValues,
    ) -> bool {
        <Self as InlineLayout<StyloStyle>>::style_change_rebuilds(
            &StyloStyleRef::of(old),
            &StyloStyleRef::of(new),
        )
    }

    #[inline]
    fn takes_room_above(&self) -> bool {
        <Self as InlineLayout<StyloStyle>>::takes_room_above(self)
    }

    #[inline]
    fn is_empty(&self) -> bool {
        <Self as InlineLayout<StyloStyle>>::is_empty(self)
    }

    #[inline]
    fn prepare(
        &mut self,
        cx: &mut TextContext,
        source: &crate::layout::inline::DomSource<'_>,
        root: NodeId,
        scale: f32,
        lines: LinesInputs<'_>,
    ) {
        <Self as InlineLayout<StyloStyle>>::prepare(self, cx, source, node_key(root), scale, lines);
    }

    #[inline]
    fn content_widths(&mut self) -> ContentWidths {
        <Self as InlineLayout<StyloStyle>>::content_widths(self)
    }

    #[inline]
    fn break_lines(
        &mut self,
        cx: &mut TextContext,
        area: LineArea,
        style: Option<&StyloStyle>,
        exclusions: &mut impl LineExclusions,
    ) {
        <Self as InlineLayout<StyloStyle>>::break_lines(self, cx, area, style, exclusions);
    }

    #[inline]
    fn extent(&self, end_padding: f32) -> LinesExtent {
        <Self as InlineLayout<StyloStyle>>::extent(self, end_padding)
    }

    #[inline]
    fn set_frame(&mut self, block_offset: f32, content: taffy::Size<f32>) {
        <Self as InlineLayout<StyloStyle>>::set_frame(
            self,
            block_offset,
            blitz_text::Size {
                width: content.width,
                height: content.height,
            },
        );
    }

    #[inline]
    fn placements(&self) -> impl Iterator<Item = Placement> {
        <Self as InlineLayout<StyloStyle>>::placements(self)
    }

    #[inline]
    fn line_flow(&self) -> LineFlow {
        <Self as InlineLayout<StyloStyle>>::line_flow(self)
    }

    #[inline]
    fn place_on_page(&self, along: [f32; 2], across: [f32; 2]) -> (f32, f32) {
        <Self as InlineLayout<StyloStyle>>::place_on_page(self, along, across)
    }

    #[inline]
    fn last_line_baseline(&self) -> LastBaseline {
        <Self as InlineLayout<StyloStyle>>::last_line_baseline(self)
    }

    #[inline]
    fn room_below(&self) -> f32 {
        <Self as InlineLayout<StyloStyle>>::room_below(self)
    }

    #[inline]
    fn float_node(key: u64) -> Option<NodeId> {
        <Self as InlineLayout<StyloStyle>>::float_node(key).map(key_node)
    }
}

/// The text backend's editor, as Blitz drives it over Stylo's styles: [`blitz_text::EditEngine`]
/// for [`StyloStyle`].
pub(crate) trait EditEngine: EditableText + Sized {
    /// An empty editor.
    fn new(is_multiline: bool) -> Self;
    /// Sets the text the editor starts with.
    fn set_initial_text(&mut self, text: &str);
    /// Sets the text, where it changed.
    fn set_text(&mut self, text: &str);
    /// Sets the text in `style`, `node`'s computed style, at `scale`.
    fn set_style(
        &mut self,
        node: NodeId,
        style: Option<&style::servo_arc::Arc<style::properties::ComputedValues>>,
        scale: f32,
    );
    /// Takes `node`'s style after a restyle that kept its box; returns whether the text is to
    /// be laid out again.
    fn restyle(
        &mut self,
        node: NodeId,
        style: &style::servo_arc::Arc<style::properties::ComputedValues>,
    ) -> bool;
    /// Sets the scale.
    fn set_scale(&mut self, scale: f32);
    /// Sets the width lines wrap at, in device pixels.
    fn set_width(&mut self, width: Option<f32>);
    /// Lays the text out again, where a setter changed it.
    fn refresh(&mut self, cx: &mut TextContext);
    /// Applies `edit`.
    fn edit(&mut self, cx: &mut TextContext, edit: Edit<'_>);
}

impl EditEngine for TextEditor {
    #[inline]
    fn new(is_multiline: bool) -> Self {
        <Self as blitz_text::EditEngine<StyloStyle>>::new(is_multiline)
    }

    #[inline]
    fn set_initial_text(&mut self, text: &str) {
        <Self as blitz_text::EditEngine<StyloStyle>>::set_initial_text(self, text);
    }

    #[inline]
    fn set_text(&mut self, text: &str) {
        <Self as blitz_text::EditEngine<StyloStyle>>::set_text(self, text);
    }

    #[inline]
    fn set_style(
        &mut self,
        node: NodeId,
        style: Option<&style::servo_arc::Arc<style::properties::ComputedValues>>,
        scale: f32,
    ) {
        <Self as blitz_text::EditEngine<StyloStyle>>::set_style(
            self,
            node_key(node),
            style.map(StyloStyle::from_arc),
            scale,
        );
    }

    #[inline]
    fn restyle(
        &mut self,
        node: NodeId,
        style: &style::servo_arc::Arc<style::properties::ComputedValues>,
    ) -> bool {
        <Self as blitz_text::EditEngine<StyloStyle>>::restyle(
            self,
            node_key(node),
            StyloStyle::from_arc(style),
        )
    }

    #[inline]
    fn set_scale(&mut self, scale: f32) {
        <Self as blitz_text::EditEngine<StyloStyle>>::set_scale(self, scale);
    }

    #[inline]
    fn set_width(&mut self, width: Option<f32>) {
        <Self as blitz_text::EditEngine<StyloStyle>>::set_width(self, width);
    }

    #[inline]
    fn refresh(&mut self, cx: &mut TextContext) {
        <Self as blitz_text::EditEngine<StyloStyle>>::refresh(self, cx);
    }

    #[inline]
    fn edit(&mut self, cx: &mut TextContext, edit: Edit<'_>) {
        <Self as blitz_text::EditEngine<StyloStyle>>::edit(self, cx, edit);
    }
}

/// Laying out an outside list marker over Stylo's styles.
pub(crate) trait MarkerEngine: Sized {
    /// Lays out `marker` for `node`, set in its computed style `style`, in the bullet font where
    /// `bullet`.
    fn build(
        cx: &mut TextContext,
        node: NodeId,
        style: &style::servo_arc::Arc<style::properties::ComputedValues>,
        marker: &crate::node::Marker,
        bullet: bool,
        scale: f32,
    ) -> Self;
}

impl MarkerEngine for MarkerLayout {
    #[inline]
    fn build(
        cx: &mut TextContext,
        node: NodeId,
        style: &style::servo_arc::Arc<style::properties::ComputedValues>,
        marker: &crate::node::Marker,
        bullet: bool,
        scale: f32,
    ) -> Self {
        <Self as blitz_text::MarkerEngine<StyloStyle>>::build(
            cx,
            node_key(node),
            StyloStyle::from_arc(style),
            marker,
            bullet,
            scale,
        )
    }
}

/// What restyles an element for a block's first formatted line: the document's stylist and the
/// guards its stylesheets are read under.
#[derive(Copy, Clone)]
pub(crate) struct DocumentCascade<'a> {
    pub(crate) stylist: &'a style::stylist::Stylist,
    pub(crate) guards: &'a style::shared_lock::StylesheetGuards<'a>,
}

/// An `<input>` or `<textarea>`'s editor with the document's text context, handed to
/// [`BaseDocument::with_text_input`](crate::BaseDocument::with_text_input).
pub struct TextInputDriver<'a> {
    pub(crate) editor: &'a mut TextEditor,
    pub(crate) cx: &'a mut TextContext,
}

impl TextInputDriver<'_> {
    /// The editor, to read back.
    #[inline]
    pub fn editor(&self) -> &TextEditor {
        self.editor
    }

    /// The text, with any text an input method is composing.
    #[inline]
    pub fn raw_text(&self) -> &str {
        self.editor.raw_text()
    }

    /// Applies `edit`.
    #[inline]
    pub fn edit(&mut self, edit: Edit<'_>) {
        EditEngine::edit(self.editor, self.cx, edit);
    }

    /// Selects all the text.
    pub fn select_all(&mut self) {
        self.edit(Edit::SelectAll);
    }

    /// Selects the bytes from `start` to `end`.
    pub fn select_byte_range(&mut self, start: usize, end: usize) {
        self.edit(Edit::SelectByteRange(start, end));
    }
}

/// Reads the descriptors of an `@font-face` rule.
pub(crate) fn face_descriptors(descriptors: &style::font_face::Descriptors) -> FaceDescriptors {
    use style::font_face::FontStyleRange;
    use style::values::specified::font::MetricsOverride;
    let metric = |value: &Option<MetricsOverride>| match value {
        Some(MetricsOverride::Override(percentage)) => percentage.0.get(),
        _ => None,
    };
    FaceDescriptors {
        weight: descriptors
            .font_weight
            .as_ref()
            .and_then(|range| range.compute())
            .map(|range| {
                (
                    FontWeight::new(range.0.value()),
                    FontWeight::new(range.1.value()),
                )
            }),
        style: descriptors.font_style.as_ref().map(|style| match style {
            FontStyleRange::Italic => (FontStyle::Italic, FontStyle::Italic),
            FontStyleRange::Oblique(min, max) => {
                let (min, max) = (min.degrees(), max.degrees());
                // Stylo parses `normal` as an oblique of no angle.
                if min.is_none_or(|angle| angle == 0.0) && max.is_none_or(|angle| angle == 0.0) {
                    (FontStyle::Normal, FontStyle::Normal)
                } else {
                    (FontStyle::Oblique(min), FontStyle::Oblique(max.or(min)))
                }
            }
        }),
        width: descriptors
            .font_width
            .as_ref()
            .and_then(|range| range.compute())
            .map(|range| {
                (
                    FontWidth::from_percentage(range.0.to_percentage().0 * 100.0),
                    FontWidth::from_percentage(range.1.to_percentage().0 * 100.0),
                )
            }),
        unicode_range: descriptors
            .unicode_range
            .as_ref()
            .map(|ranges| ranges.iter().map(|range| range.start..=range.end).collect())
            .unwrap_or_default(),
        // The rule's settings are specified values, whose numbers are known once parsed.
        feature_settings: descriptors
            .font_feature_settings
            .as_ref()
            .map(|settings| {
                settings
                    .0
                    .iter()
                    .filter_map(|setting| {
                        let value = setting.value.resolve()?;
                        Some(FontFeature::new(
                            Tag::from_bytes(setting.tag.0.to_be_bytes()),
                            value.clamp(0, i32::from(u16::MAX)) as u16,
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default(),
        variation_settings: descriptors
            .font_variation_settings
            .as_ref()
            .map(|settings| {
                settings
                    .0
                    .iter()
                    .filter_map(|setting| {
                        Some(FontVariation::new(
                            Tag::from_bytes(setting.tag.0.to_be_bytes()),
                            setting.value.get()?,
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default(),
        size_adjust: descriptors
            .size_adjust
            .as_ref()
            .and_then(|percentage| percentage.0.get()),
        ascent_override: metric(&descriptors.ascent_override),
        descent_override: metric(&descriptors.descent_override),
        line_gap_override: metric(&descriptors.line_gap_override),
    }
}

/// Fonts with only the fonts in `font_data`, which every generic family resolves to, and no
/// platform fonts: the standard setup for WASM, where browsers do not expose them. WOFF and WOFF2
/// are decoded.
pub fn build_single_font_ctx(font_data: &[u8]) -> FontContext {
    FontContext::with_single_font(font_data)
}

#[cfg(test)]
mod tests {
    use style::stylesheets::{CssRule, Origin, StylesheetInDocument as _};

    use parlance::{FontStyle, FontWeight};

    use super::{BACKEND, FaceDescriptors, TextBackend, face_descriptors};
    use crate::{BaseDocument, DocumentConfig};

    /// The descriptors of an `@font-face` rule with `declarations`.
    fn face(declarations: &str) -> FaceDescriptors {
        let doc = BaseDocument::new(DocumentConfig::default());
        let css = format!("@font-face {{ font-family: face; {declarations} }}");
        let sheet = doc.make_stylesheet(css, Origin::Author);
        let guard = doc.guard.read();
        let rules = sheet.0.contents(&guard).rules(&guard);
        rules
            .iter()
            .find_map(|rule| match rule {
                CssRule::FontFace(face) => {
                    Some(face_descriptors(&face.read_with(&guard).descriptors))
                }
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn face_weight_is_a_value_or_a_range() {
        assert_eq!(face("").weight, None);
        let weight = |min, max| Some((FontWeight::new(min), FontWeight::new(max)));
        assert_eq!(face("font-weight: 700").weight, weight(700.0, 700.0));
        assert_eq!(face("font-weight: bold").weight, weight(700.0, 700.0));
        assert_eq!(face("font-weight: 300 600").weight, weight(300.0, 600.0));
    }

    #[test]
    fn face_style_is_normal_italic_or_oblique() {
        let style = |style| Some((style, style));
        assert_eq!(face("").style, None);
        assert_eq!(face("font-style: normal").style, style(FontStyle::Normal));
        assert_eq!(face("font-style: italic").style, style(FontStyle::Italic));
        assert_eq!(
            face("font-style: oblique").style,
            style(FontStyle::Oblique(Some(14.0)))
        );
        assert_eq!(
            face("font-style: oblique 10deg 20deg").style,
            Some((
                FontStyle::Oblique(Some(10.0)),
                FontStyle::Oblique(Some(20.0))
            ))
        );
    }

    /// Parley is the backend wherever its feature is on, winkin's or not.
    #[test]
    fn parley_wins_when_both_backends_are_enabled() {
        let expected = if cfg!(feature = "parley") {
            TextBackend::Parley
        } else {
            TextBackend::Winkin
        };
        assert_eq!(BACKEND, expected);
    }
}

#[cfg(all(test, text_parley))]
mod font_face_override_tests {
    use crate::net::{FontFaceOverrides, Resource, ResourceLoadResponse};
    use crate::text::FaceDescriptors;
    use crate::text::parlance::{FontStyle, FontWeight};
    use crate::{BaseDocument, DocumentConfig};

    /// Regression-pin for the `@font-face` descriptor-honouring fix.
    ///
    /// The bug was that `Resource::Font` carried only the raw font bytes,
    /// so `load_resource` registered fonts with `info_override = None` and
    /// parley fell back to the TTF's internal `name` table. After the fix,
    /// `Resource::Font` carries `FontFaceOverrides` and `load_resource`
    /// builds a `FontInfoOverride` from them — meaning a CSS-declared
    /// `font-family` alias wins over the file's own metadata.
    ///
    /// We drive `load_resource` directly with a fabricated response rather
    /// than go through HTML parsing → `fetch_font_face`, because the
    /// downstream HTML parser lives in `blitz-html` (would be a circular
    /// crate dependency). The mapping from `@font-face` descriptors into
    /// `FontFaceOverrides` is covered by the unit tests in this module; this
    /// test pins the load-side of the pipeline.
    #[test]
    fn font_face_overrides_alias_family_name() {
        const ALIAS: &str = "AliasedFamily";

        let mut document = BaseDocument::new(DocumentConfig::default());

        // Sanity: the alias name is not registered before we feed the font.
        {
            let mut ctx = document.text.font_ctx.lock().unwrap();
            assert!(
                ctx.collection.family_id(ALIAS).is_none(),
                "alias must not exist before registration",
            );
        }

        // Drive `load_resource` with a `Resource::Font` whose overrides
        // assert the CSS-side family name. We use the bullet font as a
        // valid font payload — its internal `name` table is irrelevant to
        // the assertion; what matters is whether the override wins.
        let response = ResourceLoadResponse {
            request_id: 0,
            node_id: None,
            resolved_url: Some(String::from("test://aliased-family")),
            result: Ok(Resource::Font(
                blitz_traits::net::Bytes::from_static(crate::BULLET_FONT),
                FontFaceOverrides {
                    family_name: Some(String::from(ALIAS)),
                    descriptors: FaceDescriptors {
                        weight: Some((FontWeight::new(800.0), FontWeight::new(800.0))),
                        style: Some((FontStyle::Italic, FontStyle::Italic)),
                        ..Default::default()
                    },
                },
            )),
        };
        document.load_resource(response);

        // The override must have taken effect: parley's `Collection` now
        // resolves the CSS-declared alias to a registered family.
        let mut ctx = document.text.font_ctx.lock().unwrap();
        let family_id = ctx
            .collection
            .family_id(ALIAS)
            .expect("CSS-declared family name should be registered as a family alias");
        let resolved_name = ctx
            .collection
            .family_name(family_id)
            .expect("family id should resolve back to a name");
        assert_eq!(
            resolved_name, ALIAS,
            "registered family should report the CSS-declared name, \
             not the font file's internal `name` table entry",
        );
    }
}

#[doc(hidden)]
pub static PERF_COUNTS: [core::sync::atomic::AtomicUsize; 2] =
    [const { core::sync::atomic::AtomicUsize::new(0) }; 2];
