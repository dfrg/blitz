//! Parley's fonts: the collection documents lay text out with, `@font-face` registration, and the
//! font metrics a style engine resolves font-relative units with.

use std::borrow::Cow;
#[cfg(feature = "parallel")]
use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use blitz_text::{
    BULLET_FONT, Bytes, DocumentText, FaceDescriptors, FontMetrics, FontMetricsSource, FontQuery,
    TextFonts, decode_font_bytes,
};
use parley::fontique::{Blob, Collection, CollectionOptions, GenericFamily, SourceCache};
use parley::{FontFamilyName, FontVariation, LayoutContext};
use skrifa::MetadataProvider as _;
use skrifa::charmap::Charmap;

use crate::{TextContext, style as css_to_parley};

/// The fonts documents lay text out with: a fontique collection and its source cache. Clones
/// share the fonts they have loaded.
#[derive(Clone)]
pub struct FontContext {
    /// The fonts.
    pub collection: Collection,
    /// The cache of the fonts' data.
    pub source_cache: SourceCache,
}

impl FontContext {
    /// The platform's fonts, where the `system` feature reads them, and none otherwise.
    pub fn new() -> Self {
        Self::from(parley::FontContext::new())
    }
}

impl Default for FontContext {
    fn default() -> Self {
        Self::from(parley::FontContext::default())
    }
}

impl From<parley::FontContext> for FontContext {
    fn from(fonts: parley::FontContext) -> Self {
        Self {
            collection: fonts.collection,
            source_cache: fonts.source_cache,
        }
    }
}

impl From<FontContext> for parley::FontContext {
    fn from(fonts: FontContext) -> Self {
        Self {
            collection: fonts.collection,
            source_cache: fonts.source_cache,
        }
    }
}

impl TextFonts for FontContext {
    fn with_single_font(font_data: &[u8]) -> Self {
        let mut ctx = FontContext {
            source_cache: SourceCache::new_shared(),
            collection: Collection::new(CollectionOptions {
                shared: false,
                system_fonts: false,
            }),
        };
        let decoded = decode_font_bytes(font_data).into_owned();
        let registered = ctx
            .collection
            .register_fonts(Blob::new(Arc::new(decoded) as _), None);
        let family_ids: Vec<_> = registered.iter().map(|(id, _)| *id).collect();
        for generic in [
            GenericFamily::SansSerif,
            GenericFamily::Serif,
            GenericFamily::Monospace,
            GenericFamily::SystemUi,
        ] {
            ctx.collection
                .append_generic_families(generic, family_ids.iter().copied());
        }
        ctx
    }

    fn add_fonts(&mut self, font_data: impl Into<Bytes>) {
        let font_data = font_data.into();
        let blob = match decode_font_bytes(&font_data) {
            Cow::Owned(decoded) => Blob::new(Arc::new(decoded) as _),
            Cow::Borrowed(_) => Blob::new(Arc::new(font_data) as _),
        };
        self.collection.register_fonts(blob, None);
    }
}

impl DocumentText for TextContext {
    type Fonts = FontContext;
    type Metrics = ParleyFontMetrics;

    fn new(fonts: Option<FontContext>) -> Self {
        let font_ctx = fonts
            .map(|fonts| {
                let mut font_ctx = parley::FontContext::from(fonts);
                font_ctx.source_cache.make_shared();
                font_ctx
            })
            .unwrap_or_else(|| {
                let mut font_ctx = parley::FontContext {
                    source_cache: SourceCache::new_shared(),
                    collection: Collection::new(CollectionOptions {
                        shared: false,
                        system_fonts: cfg!(all(feature = "system", not(target_arch = "wasm32"))),
                    }),
                };
                font_ctx
                    .collection
                    .register_fonts(Blob::new(Arc::new(BULLET_FONT) as _), None);
                font_ctx
            });
        Self {
            font_ctx: Arc::new(Mutex::new(font_ctx)),
            #[cfg(feature = "parallel")]
            thread_font_contexts: thread_local::ThreadLocal::new(),
            layout_ctx: LayoutContext::new(),
        }
    }

    fn fonts(&self) -> FontContext {
        FontContext::from(self.font_ctx.lock().unwrap().clone())
    }

    fn add_web_font(&mut self, bytes: Bytes, family: Option<&str>, descriptors: &FaceDescriptors) {
        let font = Blob::new(Arc::new(bytes));

        // Registered under the `@font-face` rule's descriptors: without them, Parley reads the
        // family name from the font's own metadata, so that `font-family: 'Avenir Book'` would
        // not match a font file that calls itself `Avenir 45 Book`. fontique takes one weight
        // and one style, so a range registers as its lowest.
        let info_override = parley::fontique::FontInfoOverride {
            family_name: family,
            weight: descriptors.weight.map(|(min, _)| min),
            style: descriptors.style.map(|(min, _)| min),
            ..Default::default()
        };

        let mut global_font_ctx = self.font_ctx.lock().unwrap();
        global_font_ctx
            .collection
            .register_fonts(font.clone(), Some(info_override));

        #[cfg(feature = "parallel")]
        {
            rayon::broadcast(|_ctx| {
                let mut font_ctx = self
                    .thread_font_contexts
                    .get_or(|| RefCell::new(Box::new(global_font_ctx.clone())))
                    .borrow_mut();
                font_ctx
                    .collection
                    .register_fonts(font.clone(), Some(info_override));
            });
        }
        drop(global_font_ctx);
    }

    fn font_metrics(&self) -> ParleyFontMetrics {
        ParleyFontMetrics {
            font_ctx: self.font_ctx.clone(),
        }
    }
}

/// Font metrics from the fonts a document lays its text out with: the first font of the family
/// list that maps the character measured.
#[derive(Clone)]
pub struct ParleyFontMetrics {
    font_ctx: Arc<Mutex<parley::FontContext>>,
}

impl core::fmt::Debug for ParleyFontMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ParleyFontMetrics")
    }
}

impl FontMetricsSource for ParleyFontMetrics {
    fn query<'a, I>(&self, query: FontQuery<'_, I>) -> FontMetrics
    where
        I: Iterator<Item = FontFamilyName<'a>> + Clone,
    {
        use parley::fontique::{Attributes, Query, QueryFont, QueryStatus};
        use skrifa::instance::{LocationRef, Size};
        use skrifa::metrics::{GlyphMetrics, Metrics};

        let mut font_ctx = self.font_ctx.lock().unwrap();
        let font_ctx = &mut *font_ctx;

        // The fonts that match the family list and attributes
        let families: Vec<FontFamilyName<'a>> = query.families.collect();
        let mut fonts = font_ctx.collection.query(&mut font_ctx.source_cache);
        fonts.set_families(families.iter().map(css_to_parley::query_font_family));
        fonts.set_attributes(Attributes {
            width: query.width,
            weight: query.weight,
            style: query.style,
        });
        let variations = query.variations;
        let font_size = query.size;

        fn find_font_for(query: &mut Query, ch: char) -> Option<QueryFont> {
            let mut font = None;
            query.matches_with(|q_font: &QueryFont| {
                let Ok(font_ref) = skrifa::FontRef::from_index(q_font.blob.as_ref(), q_font.index)
                else {
                    return QueryStatus::Continue;
                };

                let charmap = font_ref.charmap();
                if charmap.map(ch).is_some() {
                    font = Some(q_font.clone());
                    QueryStatus::Stop
                } else {
                    QueryStatus::Continue
                }
            });
            font
        }

        /// Scales the advance as Parley scales shaped advances (font units × size / upem) rather
        /// than with skrifa's FreeType-compatible fixed-point scale, so that `ch` and `ic`
        /// lengths match the width of the laid-out text exactly.
        fn advance_of(
            query: &mut Query,
            ch: char,
            font_size: f32,
            variations: &[FontVariation],
        ) -> Option<f32> {
            let font = find_font_for(query, ch)?;
            let font_ref = skrifa::FontRef::from_index(font.blob.as_ref(), font.index).ok()?;
            let location = font_ref.axes().location(
                variations
                    .iter()
                    .map(|v| (skrifa::Tag::from_be_bytes(v.tag.to_bytes()), v.value)),
            );
            let location_ref = LocationRef::from(&location);
            let upem = Metrics::new(&font_ref, Size::unscaled(), location_ref).units_per_em;
            if upem == 0 {
                return None;
            }
            let glyph_metrics = GlyphMetrics::new(&font_ref, Size::unscaled(), location_ref);
            let char_map = Charmap::new(&font_ref);
            let glyph_id = char_map.map(ch)?;
            let advance = glyph_metrics.advance_width(glyph_id)?;
            Some(advance * (font_size / upem as f32))
        }

        fn metrics_of(
            query: &mut Query,
            ch: char,
            font_size: Size,
            variations: &[FontVariation],
        ) -> Option<(f32, Option<f32>, Option<f32>)> {
            let font = find_font_for(query, ch)?;
            let font_ref = skrifa::FontRef::from_index(font.blob.as_ref(), font.index).ok()?;
            let location = font_ref.axes().location(
                variations
                    .iter()
                    .map(|v| (skrifa::Tag::from_be_bytes(v.tag.to_bytes()), v.value)),
            );
            let location_ref = LocationRef::from(&location);
            let metrics = Metrics::new(&font_ref, font_size, location_ref);
            Some((metrics.ascent, metrics.x_height, metrics.cap_height))
        }

        let zero_advance = advance_of(&mut fonts, '0', font_size, variations);
        let ic_advance = advance_of(&mut fonts, '\u{6C34}', font_size, variations);
        let (ascent, x_height, cap_height) =
            metrics_of(&mut fonts, ' ', Size::new(font_size), variations)
                .unwrap_or((0.0, None, None));

        FontMetrics {
            ascent,
            x_height: x_height.filter(|xh| *xh != 0.0),
            cap_height,
            zero_advance,
            ic_width: ic_advance,
        }
    }
}
