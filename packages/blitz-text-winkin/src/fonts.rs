//! winkin's fonts: the collection documents lay text out with, `@font-face` registration, and
//! the font metrics Stylo resolves font-relative units with.

use std::sync::{Arc, RwLock};

use blitz_text::{
    BULLET_FONT, Bytes, DocumentText, FaceDescriptors, FontMetrics, FontMetricsSource, FontQuery,
    TextFonts, decode_font_bytes,
};
use fontwich::{
    Attributes, Collection, FallbackKey, FallbackOverride, FallbackRequest, Family, FontBytes,
    LayerBuilder, Role,
};
use skrifa::MetadataProvider as _;
use skrifa::instance::{LocationRef, Size};
use skrifa::metrics::{GlyphMetrics, Metrics};
use winkin::style::FontFamilyName;
use winkin::{Context, SharedFonts};

use super::TextContext;

/// The fonts documents lay text out with: a fontwich collection and the parsed fonts every
/// context shares. Clones share loaded fonts, fallback answers and parsed fonts, so hand a clone
/// of one to every document rather than listing the platform's fonts for each.
#[derive(Clone)]
pub struct FontContext {
    collection: Collection,
    shared: SharedFonts,
}

impl FontContext {
    /// The platform's fonts, where the backend is built to read them (`system`), and no fonts
    /// otherwise.
    pub fn new() -> Self {
        #[cfg(all(feature = "system", not(target_arch = "wasm32")))]
        let collection = Collection::system();
        #[cfg(not(all(feature = "system", not(target_arch = "wasm32"))))]
        let collection = Collection::new();
        Self::from_collection(collection)
    }

    /// The fonts in `collection`.
    pub fn from_collection(collection: Collection) -> Self {
        Self {
            collection,
            shared: SharedFonts::default(),
        }
    }

    /// The fontwich collection.
    pub fn collection(&self) -> &Collection {
        &self.collection
    }

    /// The parsed fonts every context made from this one shares.
    #[cfg_attr(not(feature = "parallel"), allow(dead_code))]
    pub(crate) fn shared_fonts(&self) -> &SharedFonts {
        &self.shared
    }
}

impl Default for FontContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Fallback to the families of one layer, for every request: every generic family, and every
/// character, resolves to them.
struct OnlyFamilies(Vec<String>);

impl FallbackOverride for OnlyFamilies {
    fn families(&self, _key: &FallbackKey, collection: &Collection, out: &mut Vec<Family>) {
        out.extend(self.0.iter().filter_map(|name| collection.family(name)));
    }
}

impl TextFonts for FontContext {
    fn with_single_font(font_data: &[u8]) -> Self {
        let mut layer = LayerBuilder::new(Role::Application);
        let decoded = decode_font_bytes(font_data).into_owned();
        let families = layer.add_data(FontBytes::new(decoded)).unwrap_or_default();
        let names = families
            .iter()
            .map(|family| family.name().to_string())
            .collect();
        layer.set_fallback_override(OnlyFamilies(names));
        Self::from_collection(Collection::new().with_layer(layer.snapshot()))
    }

    fn add_fonts(&mut self, font_data: impl Into<Bytes>) {
        let mut layer = LayerBuilder::new(Role::Application);
        let font_data = font_data.into();
        let bytes = match decode_font_bytes(&font_data) {
            std::borrow::Cow::Owned(decoded) => FontBytes::new(decoded),
            std::borrow::Cow::Borrowed(_) => FontBytes::new(font_data),
        };
        if layer.add_data(bytes).is_ok() {
            self.collection.push(layer.snapshot());
        }
    }
}

impl DocumentText for TextContext {
    type Fonts = FontContext;
    type Metrics = WinkinFontMetrics;

    fn new(fonts: Option<FontContext>) -> Self {
        let given = fonts.unwrap_or_default();
        let mut shipped = LayerBuilder::new(Role::Application);
        let _ = shipped.add_data(FontBytes::from(BULLET_FONT));
        let base = given.collection.clone().with_layer(shipped.snapshot());
        let document = LayerBuilder::new(Role::Document);
        let collection = base.clone().with_layer(document.snapshot());
        Self {
            cx: {
                let mut cx = Context::with_shared_fonts(collection.clone(), given.shared.clone());
                cx.set_calc_resolver(Some(Box::new(crate::calc::HostCalc)));
                cx
            },
            generation: 0,
            #[cfg(feature = "parallel")]
            threads: thread_local::ThreadLocal::new(),
            metrics: Arc::new(RwLock::new(collection)),
            given,
            base,
            document,
        }
    }

    fn fonts(&self) -> FontContext {
        self.given.clone()
    }

    fn add_web_font(&mut self, bytes: Bytes, family: Option<&str>, descriptors: &FaceDescriptors) {
        // A rule with no family, or bytes that are not a font, adds nothing, as CSS falls back
        // past a face whose download is no font.
        let bytes = FontBytes::new(bytes);
        let added = match family {
            Some(family) => self
                .document
                .add_face(family, fontwich_descriptors(descriptors), Some((bytes, 0)))
                .is_ok(),
            None => self.document.add_data(bytes).is_ok(),
        };
        if added {
            let collection = self.base.clone().with_layer(self.document.snapshot());
            self.cx.set_collection(collection.clone());
            self.generation += 1;
            *self.metrics.write().unwrap() = collection;
        }
    }

    fn font_metrics(&self) -> WinkinFontMetrics {
        WinkinFontMetrics {
            fonts: self.metrics.clone(),
        }
    }
}

/// Font metrics from the fonts a document's text is laid out with: the first font in the family
/// list that maps the character measured, then the platform's standard font.
pub struct WinkinFontMetrics {
    fonts: Arc<RwLock<Collection>>,
}

impl core::fmt::Debug for WinkinFontMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WinkinFontMetrics")
    }
}

/// The bytes of a font, and its index in them.
type LoadedFont = (FontBytes, u32);

impl WinkinFontMetrics {
    /// The first font of `families`, matched to `attributes`, that maps `ch`; or else the
    /// standard font's.
    fn font_for(
        fonts: &Collection,
        families: &[FontFamilyName<'_>],
        attributes: Attributes,
        ch: char,
    ) -> Option<LoadedFont> {
        let request = FallbackRequest::Standard(None);
        let standard = fonts.fallback(&fonts.key(&request)).first().cloned();
        families
            .iter()
            .filter_map(|name| fonts.resolve(name, &request))
            .chain(standard)
            .find_map(|family| {
                let font = family.match_font(attributes, true)?;
                if font.is_pending() || !font.serves(ch) || !font.charset().contains(ch) {
                    return None;
                }
                Some((font.load()?, font.index()))
            })
    }
}

impl FontMetricsSource for WinkinFontMetrics {
    fn query<'a, I>(&self, query: FontQuery<'_, I>) -> FontMetrics
    where
        I: Iterator<Item = blitz_text::style::FontFamilyName<'a>> + Clone,
    {
        use blitz_text::style::FontSizeAdjust;
        let fonts = self.fonts.read().unwrap();
        let families: Vec<FontFamilyName<'_>> =
            query.families.map(super::style::family_name).collect();
        let attributes = Attributes {
            width: query.width,
            weight: query.weight,
            style: query.style,
        };
        let variations: Vec<(skrifa::Tag, f32)> = query
            .variations
            .iter()
            .map(|setting| {
                (
                    skrifa::Tag::from_be_bytes(setting.tag.to_bytes()),
                    setting.value,
                )
            })
            .collect();
        let vertical = query.vertical;
        let font_size = query.size;

        // The advance of `ch`, scaled as winkin scales a shaped advance: font units times size
        // over units per em. Text set upright in a vertical line advances down it: by the
        // glyph's `vmtx` height, or else by the font's ascent and descent, as HarfBuzz
        // advances a glyph vertically without one.
        let advance_of = |ch: char| -> Option<f32> {
            let (bytes, index) = Self::font_for(&fonts, &families, attributes, ch)?;
            let font = skrifa::FontRef::from_index(bytes.data(), index).ok()?;
            let location = font.axes().location(variations.iter().copied());
            let location = LocationRef::from(&location);
            let upem = Metrics::new(&font, Size::unscaled(), location).units_per_em;
            if upem == 0 {
                return None;
            }
            let glyph = font.charmap().map(ch)?;
            let advance = if vertical {
                use skrifa::raw::TableProvider as _;
                match font.vmtx().ok().and_then(|vmtx| vmtx.advance(glyph)) {
                    Some(advance) => f32::from(advance),
                    None => {
                        let metrics = Metrics::new(&font, Size::unscaled(), location);
                        metrics.ascent - metrics.descent
                    }
                }
            } else {
                GlyphMetrics::new(&font, Size::unscaled(), location).advance_width(glyph)?
            };
            Some(advance * (font_size / f32::from(upem)))
        };
        let metrics_of = |ch: char| -> Option<(f32, Option<f32>, Option<f32>)> {
            let (bytes, index) = Self::font_for(&fonts, &families, attributes, ch)?;
            let font = skrifa::FontRef::from_index(bytes.data(), index).ok()?;
            let location = font.axes().location(variations.iter().copied());
            let metrics = Metrics::new(&font, Size::new(font_size), LocationRef::from(&location));
            Some((metrics.ascent, metrics.x_height, metrics.cap_height))
        };

        let zero_advance = advance_of('0');
        let ic_advance = advance_of('\u{6C34}');
        let (ascent, x_height, cap_height) = metrics_of(' ').unwrap_or((0.0, None, None));

        let x_height = x_height.filter(|xh| *xh != 0.0);
        // `font-size-adjust` scales the font to its used size, which the font-relative units
        // measure: the metric it names, over the size, is set to its value.
        let metric = match query.size_adjust {
            FontSizeAdjust::None => None,
            FontSizeAdjust::ExHeight(value) => x_height.map(|metric| (value, metric)),
            FontSizeAdjust::CapHeight(value) => cap_height.map(|metric| (value, metric)),
            FontSizeAdjust::ChWidth(value) => zero_advance.map(|metric| (value, metric)),
            FontSizeAdjust::IcWidth(value) | FontSizeAdjust::IcHeight(value) => {
                ic_advance.map(|metric| (value, metric))
            }
        };
        let scale = metric
            .map(|(value, metric)| value * font_size / metric)
            .filter(|scale| scale.is_finite() && *scale >= 0.0)
            .unwrap_or(1.0);
        let scaled = |metric: f32| metric * scale;

        FontMetrics {
            ascent: scaled(ascent),
            x_height: x_height.map(scaled),
            cap_height: cap_height.map(scaled),
            zero_advance: zero_advance.map(scaled),
            ic_width: ic_advance.map(scaled),
        }
    }
}

/// An `@font-face` rule's descriptors as fontwich declares a face with them.
fn fontwich_descriptors(descriptors: &FaceDescriptors) -> fontwich::FaceDescriptors {
    use blitz_text::parlance::FontStyle;
    use fontwich::{FaceStyle, FontWeight, FontWidth};
    use winkin::style::{FontFeature, FontVariation, Tag};

    /// The angle CSS gives an `oblique` that names none.
    const OBLIQUE: f32 = 14.0;
    fontwich::FaceDescriptors {
        weight: descriptors
            .weight
            .map(|(min, max)| (FontWeight::new(min.value()), FontWeight::new(max.value()))),
        width: descriptors.width.map(|(min, max)| {
            (
                FontWidth::from_ratio(min.ratio()),
                FontWidth::from_ratio(max.ratio()),
            )
        }),
        style: descriptors.style.map(|style| match style {
            (FontStyle::Normal, _) => FaceStyle::Normal,
            (FontStyle::Italic, _) => FaceStyle::Italic,
            (FontStyle::Oblique(min), max) => {
                let max = match max {
                    FontStyle::Oblique(max) => max.or(min),
                    _ => min,
                };
                FaceStyle::Oblique(min.unwrap_or(OBLIQUE), max.unwrap_or(OBLIQUE))
            }
        }),
        unicode_range: descriptors.unicode_range.clone(),
        feature_settings: descriptors
            .feature_settings
            .iter()
            .map(|feature| FontFeature::new(Tag::from_bytes(feature.tag.to_bytes()), feature.value))
            .collect(),
        variation_settings: descriptors
            .variation_settings
            .iter()
            .map(|variation| FontVariation {
                tag: Tag::from_bytes(variation.tag.to_bytes()),
                value: variation.value,
            })
            .collect(),
        size_adjust: descriptors.size_adjust,
        ascent_override: descriptors.ascent_override,
        descent_override: descriptors.descent_override,
        line_gap_override: descriptors.line_gap_override,
    }
}
