//! Conversions from a [`TextStyle`] to Parley's styles.

use std::borrow::Cow;

use blitz_text::NodeKey;
use blitz_text::style::{self as css, Tag, TextStyle};
use parley::fontique::QueryFamily;
use parley::style::{
    AlignmentBaseline, BaselineShift, LineBreak, OverflowWrap, TextWrapMode, VerticalAlign,
    WhiteSpaceCollapse, WordBreak,
};
use parley::{
    Alignment, BaseDirection, FontFamily, FontFamilyName, FontFeature, FontFeatures, FontVariation,
    FontVariations, Language, LineHeight, StyleProperty,
};

use crate::TextBrush;

/// A named family as Blitz names it to Parley: the Apple system font's legacy names are the
/// `system-ui` generic, for web compatibility.
fn family_name(family: FontFamilyName<'_>) -> FontFamilyName<'_> {
    #[cfg(target_vendor = "apple")]
    if let FontFamilyName::Named(name) = &family {
        if name == "-apple-system" {
            return FontFamilyName::Generic(parley::GenericFamily::SystemUi);
        }
        #[cfg(target_os = "macos")]
        if name == "BlinkMacSystemFont" {
            return FontFamilyName::Generic(parley::GenericFamily::SystemUi);
        }
    }
    family
}

/// A family of `font-family`, as fontique queries it.
pub(crate) fn query_font_family<'a>(family: &'a FontFamilyName<'_>) -> QueryFamily<'a> {
    match family_name(family.clone()) {
        FontFamilyName::Named(_) => match family {
            FontFamilyName::Named(name) => QueryFamily::Named(name),
            FontFamilyName::Generic(generic) => QueryFamily::Generic(*generic),
        },
        FontFamilyName::Generic(generic) => QueryFamily::Generic(generic),
    }
}

#[inline]
fn feature(tag: &[u8; 4], value: u16) -> FontFeature {
    FontFeature {
        tag: Tag::from_bytes(*tag),
        value,
    }
}

/// Appends the `font-feature-settings` (low-level OpenType feature control) of `style`.
pub(crate) fn font_feature_settings(style: &impl TextStyle, out: &mut Vec<FontFeature>) {
    out.extend(style.font_feature_settings());
}

/// Appends the features `font-variant-ligatures` asks for.
pub(crate) fn font_variant_ligatures(input: css::FontVariantLigatures, out: &mut Vec<FontFeature>) {
    use css::FontVariantLigatures as L;

    // `none` disables all optional ligature and contextual features.
    if input.contains(L::NONE) {
        out.push(feature(b"liga", 0));
        out.push(feature(b"clig", 0));
        out.push(feature(b"dlig", 0));
        out.push(feature(b"hlig", 0));
        out.push(feature(b"calt", 0));
        return;
    }

    if input.contains(L::COMMON_LIGATURES) {
        out.push(feature(b"liga", 1));
        out.push(feature(b"clig", 1));
    } else if input.contains(L::NO_COMMON_LIGATURES) {
        out.push(feature(b"liga", 0));
        out.push(feature(b"clig", 0));
    }
    if input.contains(L::DISCRETIONARY_LIGATURES) {
        out.push(feature(b"dlig", 1));
    } else if input.contains(L::NO_DISCRETIONARY_LIGATURES) {
        out.push(feature(b"dlig", 0));
    }
    if input.contains(L::HISTORICAL_LIGATURES) {
        out.push(feature(b"hlig", 1));
    } else if input.contains(L::NO_HISTORICAL_LIGATURES) {
        out.push(feature(b"hlig", 0));
    }
    if input.contains(L::CONTEXTUAL) {
        out.push(feature(b"calt", 1));
    } else if input.contains(L::NO_CONTEXTUAL) {
        out.push(feature(b"calt", 0));
    }
}

/// Appends the features `font-variant-caps` asks for. Parley synthesizes none of them: each is
/// its font feature.
pub(crate) fn font_variant_caps(input: css::FontVariantCaps, out: &mut Vec<FontFeature>) {
    match input {
        css::FontVariantCaps::Normal => {}
        css::FontVariantCaps::SmallCaps => out.push(feature(b"smcp", 1)),
        css::FontVariantCaps::AllSmallCaps => {
            out.push(feature(b"c2sc", 1));
            out.push(feature(b"smcp", 1));
        }
        css::FontVariantCaps::PetiteCaps => out.push(feature(b"pcap", 1)),
        css::FontVariantCaps::AllPetiteCaps => {
            out.push(feature(b"c2pc", 1));
            out.push(feature(b"pcap", 1));
        }
        css::FontVariantCaps::Unicase => out.push(feature(b"unic", 1)),
        css::FontVariantCaps::TitlingCaps => out.push(feature(b"titl", 1)),
    }
}

/// Appends the features `font-variant-position` asks for.
pub(crate) fn font_variant_position(input: css::FontVariantPosition, out: &mut Vec<FontFeature>) {
    match input {
        css::FontVariantPosition::Normal => {}
        css::FontVariantPosition::Sub => out.push(feature(b"subs", 1)),
        css::FontVariantPosition::Super => out.push(feature(b"sups", 1)),
    }
}

/// Appends the features `font-variant-numeric` asks for.
pub(crate) fn font_variant_numeric(input: css::FontVariantNumeric, out: &mut Vec<FontFeature>) {
    use css::FontVariantNumeric as N;

    for (flag, tag) in [
        (N::LINING_NUMS, b"lnum"),
        (N::OLDSTYLE_NUMS, b"onum"),
        (N::PROPORTIONAL_NUMS, b"pnum"),
        (N::TABULAR_NUMS, b"tnum"),
        (N::DIAGONAL_FRACTIONS, b"frac"),
        (N::STACKED_FRACTIONS, b"afrc"),
        (N::ORDINAL, b"ordn"),
        (N::SLASHED_ZERO, b"zero"),
    ] {
        if input.contains(flag) {
            out.push(feature(tag, 1));
        }
    }
}

/// Appends the features `font-variant-east-asian` asks for.
pub(crate) fn font_variant_east_asian(
    input: css::FontVariantEastAsian,
    out: &mut Vec<FontFeature>,
) {
    use css::FontVariantEastAsian as E;

    for (flag, tag) in [
        (E::JIS78, b"jp78"),
        (E::JIS83, b"jp83"),
        (E::JIS90, b"jp90"),
        (E::JIS04, b"jp04"),
        (E::SIMPLIFIED, b"smpl"),
        (E::TRADITIONAL, b"trad"),
        (E::FULL_WIDTH, b"fwid"),
        (E::PROPORTIONAL_WIDTH, b"pwid"),
        (E::RUBY, b"ruby"),
    ] {
        if input.contains(flag) {
            out.push(feature(tag, 1));
        }
    }
}

/// The OpenType features a style asks for.
///
/// As CSS Fonts orders feature precedence
/// (<https://drafts.csswg.org/css-fonts/#feature-precedence>), the features implied by the
/// `font-variant-*` properties come first, and `font-feature-settings` last, so that it wins.
///
/// Parley sorts the features by tag, stably, and HarfBuzz applies the last of any duplicate
/// tags, so appending `font-feature-settings` after the `font-variant-*` features gives it the
/// precedence it needs.
pub(crate) fn font_features(style: &impl TextStyle) -> Vec<FontFeature> {
    let mut features = Vec::new();

    font_variant_ligatures(style.font_variant_ligatures(), &mut features);
    font_variant_caps(style.font_variant_caps(), &mut features);
    font_variant_numeric(style.font_variant_numeric(), &mut features);
    font_variant_east_asian(style.font_variant_east_asian(), &mut features);
    font_variant_position(style.font_variant_position(), &mut features);

    font_feature_settings(style, &mut features);

    features.sort_by_key(|feature| feature.tag);
    features.dedup_by_key(|feature| feature.tag);

    features
}

/// The paragraph direction of a block set in `style`.
pub(crate) fn base_direction(style: &impl TextStyle) -> BaseDirection {
    if style.unicode_bidi() == css::UnicodeBidi::Plaintext {
        return BaseDirection::Auto;
    }
    match style.direction() {
        css::Direction::Ltr => BaseDirection::Ltr,
        css::Direction::Rtl => BaseDirection::Rtl,
    }
}

/// `text-align`.
pub(crate) fn text_align(input: css::TextAlign) -> Alignment {
    match input {
        css::TextAlign::Start => Alignment::Start,
        css::TextAlign::Left => Alignment::Left,
        css::TextAlign::Right => Alignment::Right,
        css::TextAlign::Center => Alignment::Center,
        css::TextAlign::Justify => Alignment::Justify,
        css::TextAlign::End => Alignment::End,
    }
}

/// `text-align-last`, `None` for `auto`.
pub(crate) fn text_align_last(input: css::TextAlignLast) -> Option<Alignment> {
    match input {
        css::TextAlignLast::Auto => None,
        css::TextAlignLast::Start => Some(Alignment::Start),
        css::TextAlignLast::End => Some(Alignment::End),
        css::TextAlignLast::Left => Some(Alignment::Left),
        css::TextAlignLast::Right => Some(Alignment::Right),
        css::TextAlignLast::Center => Some(Alignment::Center),
        css::TextAlignLast::Justify => Some(Alignment::Justify),
    }
}

/// `text-wrap-mode`.
pub(crate) fn text_wrap_mode(input: css::TextWrapMode) -> TextWrapMode {
    match input {
        css::TextWrapMode::Wrap => TextWrapMode::Wrap,
        css::TextWrapMode::NoWrap => TextWrapMode::NoWrap,
    }
}

/// `white-space-collapse`.
pub(crate) fn white_space_collapse(input: css::WhiteSpaceCollapse) -> WhiteSpaceCollapse {
    match input {
        css::WhiteSpaceCollapse::Collapse => WhiteSpaceCollapse::Collapse,
        css::WhiteSpaceCollapse::Preserve => WhiteSpaceCollapse::Preserve,
        css::WhiteSpaceCollapse::PreserveBreaks => WhiteSpaceCollapse::PreserveBreaks,
        css::WhiteSpaceCollapse::BreakSpaces => WhiteSpaceCollapse::BreakSpaces,
        // Parley has no mode that keeps spaces but turns segment breaks into spaces.
        css::WhiteSpaceCollapse::PreserveSpaces => WhiteSpaceCollapse::Preserve,
    }
}

/// The modifications a `display: contents` element makes to the style of what it holds: its
/// white space handling.
pub(crate) fn contents_modifications(
    style: &impl TextStyle,
) -> [StyleProperty<'static, TextBrush>; 2] {
    [
        StyleProperty::WhiteSpaceCollapse(white_space_collapse(style.white_space_collapse())),
        StyleProperty::TextWrapMode(text_wrap_mode(style.text_wrap_mode())),
    ]
}

/// `vertical-align`. `alignment-baseline` and `baseline-shift` apply in SVG only, as in Chrome,
/// so HTML boxes never read them.
///
/// Percentages are of the element's own `line-height`.
pub(crate) fn vertical_align(style: &impl TextStyle) -> VerticalAlign {
    let align = VerticalAlign::new;
    let baseline = AlignmentBaseline::Baseline;
    let none = BaselineShift::Length(0.0);
    match style.vertical_align() {
        css::VerticalAlign::Baseline => align(baseline, none),
        css::VerticalAlign::Sub => align(baseline, BaselineShift::Sub),
        css::VerticalAlign::Super => align(baseline, BaselineShift::Super),
        css::VerticalAlign::Top => align(baseline, BaselineShift::Top),
        css::VerticalAlign::Bottom => align(baseline, BaselineShift::Bottom),
        css::VerticalAlign::TextTop => align(AlignmentBaseline::TextTop, none),
        css::VerticalAlign::TextBottom => align(AlignmentBaseline::TextBottom, none),
        css::VerticalAlign::Middle => align(AlignmentBaseline::Middle, none),
        css::VerticalAlign::Length(length) => {
            let shift = if length.has_percentage() {
                let font_size = style.font_size();
                let line_height = match style.line_height() {
                    // `normal` resolves against the first available font's metrics, which are
                    // not known here; 1.2 is the usual approximation.
                    css::LineHeight::Normal => font_size * 1.2,
                    css::LineHeight::Number(number) => font_size * number,
                    css::LineHeight::Length(length) => length,
                };
                length.resolve(line_height)
            } else {
                length.resolve(0.0)
            };
            align(baseline, BaselineShift::Length(shift))
        }
    }
}

/// The Parley style of the element `key`, set in `style`.
pub(crate) fn style(
    key: NodeKey,
    style: &impl TextStyle,
) -> parley::TextStyle<'static, 'static, TextBrush> {
    // Convert font size and line height
    let font_size = style.font_size();
    let line_height = match style.line_height() {
        css::LineHeight::Normal => LineHeight::NORMAL,
        css::LineHeight::Number(number) => LineHeight::FontSizeRelative(number),
        css::LineHeight::Length(length) => LineHeight::Absolute(length),
    };

    let letter_spacing = style.letter_spacing().resolve(font_size);
    let word_spacing = style.word_spacing().resolve(font_size);

    let font_variations: Vec<FontVariation> = style.font_variation_settings().collect();
    let font_features = font_features(style);

    let families: Vec<FontFamilyName<'static>> = style
        .font_family()
        .map(|family| family_name(family).into_owned())
        .collect();

    // Wrapping and breaking
    let word_break = match style.word_break() {
        css::WordBreak::Normal | css::WordBreak::BreakWord => WordBreak::Normal,
        css::WordBreak::BreakAll => WordBreak::BreakAll,
        css::WordBreak::KeepAll => WordBreak::KeepAll,
    };
    let line_break = match style.line_break() {
        css::LineBreak::Loose => LineBreak::Loose,
        css::LineBreak::Normal => LineBreak::Normal,
        css::LineBreak::Auto | css::LineBreak::Strict => LineBreak::Strict,
        css::LineBreak::Anywhere => LineBreak::Anywhere,
    };
    // `word-break: break-word` is `word-break: normal` with `overflow-wrap: anywhere`.
    let overflow_wrap = match (style.word_break(), style.overflow_wrap()) {
        (css::WordBreak::BreakWord, _) => OverflowWrap::Anywhere,
        (_, css::OverflowWrap::Normal) => OverflowWrap::Normal,
        (_, css::OverflowWrap::BreakWord) => OverflowWrap::BreakWord,
        (_, css::OverflowWrap::Anywhere) => OverflowWrap::Anywhere,
    };

    parley::TextStyle {
        font_family: FontFamily::List(Cow::Owned(families)),
        font_size,
        font_width: style.font_width(),
        font_style: style.font_style(),
        font_weight: style.font_weight(),
        font_variations: FontVariations::List(Cow::Owned(font_variations)),
        font_features: FontFeatures::List(Cow::Owned(font_features)),
        locale: Language::parse(style.language()).ok(),
        line_height,
        vertical_align: vertical_align(style),
        word_spacing,
        letter_spacing,
        text_wrap_mode: text_wrap_mode(style.text_wrap_mode()),
        white_space_collapse: white_space_collapse(style.white_space_collapse()),
        overflow_wrap,
        word_break,
        line_break,

        // The node the run's text is in.
        brush: TextBrush::from_key(key),

        // Decorations are left out of Parley's styles: they do not change the layout, and are
        // read from the node when painting, so that a change to them needs no new layout.
        has_underline: Default::default(),
        underline_offset: Default::default(),
        underline_size: Default::default(),
        underline_brush: Default::default(),
        has_strikethrough: Default::default(),
        strikethrough_offset: Default::default(),
        strikethrough_size: Default::default(),
        strikethrough_brush: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blitz_text::OwnedTextStyle;
    use std::sync::Arc;

    /// The (tag, value) pairs of a feature list.
    fn pairs(features: &[FontFeature]) -> Vec<([u8; 4], u16)> {
        features
            .iter()
            .map(|f| (f.tag.to_bytes(), f.value))
            .collect()
    }

    fn feature_settings(tags: &[(&[u8; 4], u16)]) -> OwnedTextStyle {
        OwnedTextStyle {
            font_feature_settings: tags
                .iter()
                .map(|(tag, value)| FontFeature::new(Tag::from_bytes(**tag), *value))
                .collect::<Arc<[_]>>(),
            ..OwnedTextStyle::default()
        }
    }

    #[test]
    fn ligatures_none_disables_all() {
        let mut out = Vec::new();
        font_variant_ligatures(css::FontVariantLigatures::NONE, &mut out);
        assert_eq!(
            pairs(&out),
            vec![
                (*b"liga", 0),
                (*b"clig", 0),
                (*b"dlig", 0),
                (*b"hlig", 0),
                (*b"calt", 0),
            ]
        );
    }

    #[test]
    fn ligatures_normal_emits_nothing() {
        let mut out = Vec::new();
        font_variant_ligatures(css::FontVariantLigatures::empty(), &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn ligatures_mixed_toggles() {
        let mut out = Vec::new();
        let input = css::FontVariantLigatures::COMMON_LIGATURES
            | css::FontVariantLigatures::NO_DISCRETIONARY_LIGATURES
            | css::FontVariantLigatures::HISTORICAL_LIGATURES;
        font_variant_ligatures(input, &mut out);
        assert_eq!(
            pairs(&out),
            vec![(*b"liga", 1), (*b"clig", 1), (*b"dlig", 0), (*b"hlig", 1)]
        );
    }

    #[test]
    fn caps_small_caps() {
        let mut out = Vec::new();
        font_variant_caps(css::FontVariantCaps::SmallCaps, &mut out);
        assert_eq!(pairs(&out), vec![(*b"smcp", 1)]);

        let mut out = Vec::new();
        font_variant_caps(css::FontVariantCaps::Normal, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn position_sub_and_super() {
        let mut out = Vec::new();
        font_variant_position(css::FontVariantPosition::Sub, &mut out);
        assert_eq!(pairs(&out), vec![(*b"subs", 1)]);

        let mut out = Vec::new();
        font_variant_position(css::FontVariantPosition::Super, &mut out);
        assert_eq!(pairs(&out), vec![(*b"sups", 1)]);
    }

    #[test]
    fn numeric_features() {
        let mut out = Vec::new();
        let input = css::FontVariantNumeric::OLDSTYLE_NUMS
            | css::FontVariantNumeric::TABULAR_NUMS
            | css::FontVariantNumeric::SLASHED_ZERO;
        font_variant_numeric(input, &mut out);
        assert_eq!(
            pairs(&out),
            vec![(*b"onum", 1), (*b"tnum", 1), (*b"zero", 1)]
        );
    }

    #[test]
    fn east_asian_features() {
        let mut out = Vec::new();
        let input =
            css::FontVariantEastAsian::JIS04 | css::FontVariantEastAsian::PROPORTIONAL_WIDTH;
        font_variant_east_asian(input, &mut out);
        assert_eq!(pairs(&out), vec![(*b"jp04", 1), (*b"pwid", 1)]);
    }

    #[test]
    fn feature_settings_map_directly() {
        let mut out = Vec::new();
        font_feature_settings(&feature_settings(&[(b"smcp", 1), (b"tnum", 0)]), &mut out);
        assert_eq!(pairs(&out), vec![(*b"smcp", 1), (*b"tnum", 0)]);
    }

    /// `font-feature-settings` outweighs the `font-variant-*` properties, so its entries come
    /// after the variant ones: Parley sorts by tag, stably, and HarfBuzz applies the last of any
    /// duplicate tags.
    #[test]
    fn feature_settings_override_variant_features() {
        let mut out = Vec::new();
        // font-variant-numeric: tabular-nums => tnum=1
        font_variant_numeric(css::FontVariantNumeric::TABULAR_NUMS, &mut out);
        // font-feature-settings: "tnum" 0 (should override the variant)
        font_feature_settings(&feature_settings(&[(b"tnum", 0)]), &mut out);

        let flat = pairs(&out);
        assert_eq!(flat, vec![(*b"tnum", 1), (*b"tnum", 0)]);
        let last_tnum = flat.iter().rev().find(|(tag, _)| tag == b"tnum").unwrap();
        assert_eq!(last_tnum.1, 0);
    }

    /// An owned style converts as Stylo's does: each property to its Parley value.
    #[test]
    fn style_reads_each_property() {
        let style = OwnedTextStyle {
            font_size: 20.0,
            letter_spacing: css::LengthPercentage::Percentage(0.1),
            word_break: css::WordBreak::BreakWord,
            white_space_collapse: css::WhiteSpaceCollapse::PreserveSpaces,
            ..OwnedTextStyle::with_families([FontFamilyName::named("Ahem")])
        };
        let parley = super::style(NodeKey(7), &style);
        assert_eq!(parley.letter_spacing, 2.0);
        assert_eq!(parley.overflow_wrap, OverflowWrap::Anywhere);
        assert_eq!(parley.word_break, WordBreak::Normal);
        assert_eq!(parley.white_space_collapse, WhiteSpaceCollapse::Preserve);
        assert_eq!(parley.brush.key, NodeKey(7));
        assert_eq!(
            parley.font_family,
            FontFamily::List(Cow::Owned(vec![FontFamilyName::named("Ahem")]))
        );
    }
}
