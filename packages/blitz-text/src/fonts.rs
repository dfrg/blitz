//! The fonts documents lay text out with, and the font metrics a style engine resolves
//! font-relative units with.

use std::borrow::Cow;
use std::ops::RangeInclusive;

use bytes::Bytes;
use parlance::{FontFamilyName, FontFeature, FontStyle, FontVariation, FontWeight, FontWidth};

use crate::style::FontSizeAdjust;

/// The font Blitz sets list markers' bullets in, which every backend registers as `Bullet`.
pub const BULLET_FONT: &[u8] = include_bytes!("../assets/moz-bullet-font.otf");

/// Returns `bytes` as a font file: decoded from WOFF or WOFF2 where it is one and Blitz is built
/// to decode it (the `woff` feature), and as it is otherwise.
pub fn decode_font_bytes(bytes: &[u8]) -> Cow<'_, [u8]> {
    if bytes.len() < 4 {
        return Cow::Borrowed(bytes);
    }
    match &bytes[0..4] {
        #[cfg(feature = "woff")]
        b"wOFF" => wuff::decompress_woff1(bytes)
            .map(Cow::Owned)
            .unwrap_or_else(|_| {
                #[cfg(feature = "tracing")]
                tracing::warn!("Failed to decompress woff1 font");
                Cow::Borrowed(bytes)
            }),
        #[cfg(feature = "woff")]
        b"wOF2" => wuff::decompress_woff2(bytes)
            .map(Cow::Owned)
            .unwrap_or_else(|_| {
                #[cfg(feature = "tracing")]
                tracing::warn!("Failed to decompress woff2 font");
                Cow::Borrowed(bytes)
            }),
        _ => Cow::Borrowed(bytes),
    }
}

/// The fonts documents lay text out with.
pub trait TextFonts: Clone + Default {
    /// Returns fonts with only the fonts in `font_data`, decoded from WOFF or WOFF2 where it is,
    /// which every generic family resolves to, and no platform fonts: the setup for WASM, where
    /// browsers do not expose them.
    fn with_single_font(font_data: &[u8]) -> Self;

    /// Adds the fonts in `font_data`, decoded from WOFF or WOFF2 where it is, under the family
    /// names they declare. Fonts that need no decoding are kept without a copy.
    fn add_fonts(&mut self, font_data: impl Into<Bytes>);
}

/// What a document keeps for its text: its fonts, and the contexts the backend builds and breaks
/// text in.
pub trait DocumentText: Sized {
    /// The fonts a document is handed.
    type Fonts: TextFonts;

    /// What font metrics are read from.
    type Metrics: FontMetricsSource + Send + Sync + 'static;

    /// Returns the context for a document handed `fonts`, or the default fonts.
    fn new(fonts: Option<Self::Fonts>) -> Self;

    /// Returns the fonts the document was handed, as it hands them to its iframes.
    fn fonts(&self) -> Self::Fonts;

    /// Adds a web font that has loaded, under the family name of its `@font-face` rule, where it
    /// has one, and the rule's descriptors.
    fn add_web_font(&mut self, bytes: Bytes, family: Option<&str>, descriptors: &FaceDescriptors);

    /// Returns what font metrics are read from, which follows the fonts the document adds.
    fn font_metrics(&self) -> Self::Metrics;
}

/// The descriptors of an `@font-face` rule that a backend registers a face by, beyond its family
/// name.
///
/// A descriptor left out, or one whose `calc()` cannot be resolved without an element, is `None`
/// or empty: the font's own value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FaceDescriptors {
    /// The `font-weight` descriptor, as its lowest and highest weight; a single weight is both.
    pub weight: Option<(FontWeight, FontWeight)>,
    /// The `font-width` (`font-stretch`) descriptor, as its narrowest and widest width; a single
    /// width is both.
    pub width: Option<(FontWidth, FontWidth)>,
    /// The `font-style` descriptor, as its lowest and highest style; only an oblique's angle can
    /// differ between them.
    pub style: Option<(FontStyle, FontStyle)>,
    /// The code point ranges of the `unicode-range` descriptor; empty where it allows every one.
    pub unicode_range: Vec<RangeInclusive<u32>>,
    /// The `font-feature-settings` descriptor.
    pub feature_settings: Vec<FontFeature>,
    /// The `font-variation-settings` descriptor.
    pub variation_settings: Vec<FontVariation>,
    /// The `size-adjust` descriptor, as a ratio: `1.0` is `100%`.
    pub size_adjust: Option<f32>,
    /// The `ascent-override` descriptor, as a fraction of the em.
    pub ascent_override: Option<f32>,
    /// The `descent-override` descriptor, as a fraction of the em.
    pub descent_override: Option<f32>,
    /// The `line-gap-override` descriptor, as a fraction of the em.
    pub line_gap_override: Option<f32>,
}

/// The font a font metric is asked of.
#[derive(Clone, Debug)]
pub struct FontQuery<'a, I> {
    /// The families of `font-family`, in order.
    pub families: I,
    pub weight: FontWeight,
    pub width: FontWidth,
    pub style: FontStyle,
    /// The settings of `font-variation-settings`.
    pub variations: &'a [FontVariation],
    /// `font-size-adjust`.
    pub size_adjust: FontSizeAdjust,
    /// The font size, in CSS pixels.
    pub size: f32,
    /// Whether the text is set in a vertical line.
    pub vertical: bool,
}

/// The metrics of the first available font, in CSS pixels, which font-relative units measure.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FontMetrics {
    /// The ascent (`cap` falls back on it).
    pub ascent: f32,
    /// The x-height (`ex`), where the font has one.
    pub x_height: Option<f32>,
    /// The cap height (`cap`), where the font has one.
    pub cap_height: Option<f32>,
    /// The advance of `0` (`ch`), where the font has the glyph.
    pub zero_advance: Option<f32>,
    /// The advance of `水` (`ic`), where the font has the glyph.
    pub ic_width: Option<f32>,
}

/// Where a style engine reads font metrics from.
pub trait FontMetricsSource {
    /// Returns the metrics of the first font `query` finds.
    fn query<'a, I>(&self, query: FontQuery<'_, I>) -> FontMetrics
    where
        I: Iterator<Item = FontFamilyName<'a>> + Clone;
}
