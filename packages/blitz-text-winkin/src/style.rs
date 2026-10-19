//! Conversion from a [`TextStyle`] to winkin's styles.
//!
//! Every length is in device pixels, which is what Blitz lays text out in. A percentage only
//! layout can resolve is passed on as a fraction: a margin's or padding's of the containing
//! block, a `word-spacing` percentage of the space it widens, a `text-indent` percentage of the
//! line.
//!
//! What the style does not compute stays at winkin's initial value: `white-space-trim`,
//! `text-emphasis-skip`, `line-padding`, `line-fit-edge`, `initial-letter-align` and
//! `text-group-align`.

use blitz_text::style::{self as css, TextStyle};
use blitz_text::{FeatureBlock, FontFeatureValues};
use winkin::style::{
    AdjustMetric, BaseDirection, BidiGroup, BoxDecorationBreak, Direction, DominantBaseline,
    EdgesGroup, EmphasisPosition, EmphasisSide, EmphasisSkip, EmphasisVerticalSide, FontFamilyName,
    FontFeature, FontGroup, FontKerning, FontLanguageOverride, FontOpticalSizing, FontSizeAdjust,
    FontSynthesis, FontVariantCaps, FontVariantEmoji, FontVariantPosition, FontVariants,
    FontVariation, HangEnd, HangingPunctuation, Hyphens, InitialLetter, InitialLetterAlign,
    Language, LengthPercentage, LengthPercentageOrCalc, LineBreak, LineClamp, LineGroup,
    LineHeight, OrientationGroup, OverflowWrap, RubyAlign, RubyGroup, RubyOverhang, RubyPosition,
    Sides, TabSize, Tag, TextAlign, TextAlignLast, TextAutospace, TextBoxEdge, TextBoxTrim,
    TextCase, TextCombineUpright, TextEdgeOver, TextEdgeUnder, TextEmphasis, TextGroup, TextIndent,
    TextJustify, TextOrientation, TextOverflow, TextSpacingTrim, TextTransform, TextWrapMode,
    TextWrapStyle, UnicodeBidi, VerticalAlign, WhiteSpaceCollapse, WordBreak, WritingMode,
};
use winkin::style::{TextDecorationSkipSpaces, TextFit, TextFitKind, TextFitTarget};
use winkin::{ComputedBlockStyle, ComputedStyle};

use crate::calc::{self, CalcValues};

/// Adds the features `style`'s `font-variant-alternates` resolves to through the document's
/// `@font-feature-values` rules, `values`.
///
/// Chrome resolves a name against the rules for the family a font is taken from. Here it is the
/// first family in `font-family` that any rule names, and a name no rule for it defines adds
/// nothing.
fn resolve_alternates(
    style: &impl TextStyle,
    values: &impl FontFeatureValues,
    out: &mut Vec<FontFeature>,
) {
    if values.is_empty() {
        return;
    }
    if style
        .font_variant_alternates()
        .all(|alternate| matches!(alternate, css::VariantAlternate::HistoricalForms))
    {
        return;
    }
    let Some(family) = style.font_family().find_map(|family| match family {
        css::FontFamilyName::Named(name) if values.names_family(&name) => Some(name),
        _ => None,
    }) else {
        return;
    };
    let find = |block: FeatureBlock, name: &str| values.find(&family, block, name);
    let feature = |tag: [u8; 4], value: u32| FontFeature {
        tag: Tag::from_bytes(tag),
        value: value.min(u32::from(u16::MAX)) as u16,
    };
    // `ssNN` and `cvNN`, for a number from 1 to 99.
    let numbered = |prefix: [u8; 2], number: u32| {
        (1..=99).contains(&number).then(|| {
            [
                prefix[0],
                prefix[1],
                b'0' + (number / 10) as u8,
                b'0' + (number % 10) as u8,
            ]
        })
    };
    for alternate in style.font_variant_alternates() {
        match alternate {
            css::VariantAlternate::Stylistic(name) => {
                if let Some(value) = find(FeatureBlock::Stylistic, name) {
                    out.push(feature(*b"salt", value[0]));
                }
            }
            css::VariantAlternate::Swash(name) => {
                if let Some(value) = find(FeatureBlock::Swash, name) {
                    out.push(feature(*b"swsh", value[0]));
                    out.push(feature(*b"cswh", value[0]));
                }
            }
            css::VariantAlternate::Ornaments(name) => {
                if let Some(value) = find(FeatureBlock::Ornaments, name) {
                    out.push(feature(*b"ornm", value[0]));
                }
            }
            css::VariantAlternate::Annotation(name) => {
                if let Some(value) = find(FeatureBlock::Annotation, name) {
                    out.push(feature(*b"nalt", value[0]));
                }
            }
            css::VariantAlternate::Styleset(name) => {
                for set in find(FeatureBlock::Styleset, name).unwrap_or_default() {
                    if let Some(tag) = numbered(*b"ss", set) {
                        out.push(feature(tag, 1));
                    }
                }
            }
            css::VariantAlternate::CharacterVariant(name) => {
                let Some(variant) = find(FeatureBlock::CharacterVariant, name) else {
                    continue;
                };
                if let Some(tag) = numbered(*b"cv", variant[0]) {
                    out.push(feature(tag, variant.get(1).copied().unwrap_or(1)));
                }
            }
            css::VariantAlternate::HistoricalForms => {}
        }
    }
}

/// The lists an element's style borrows: its families, and the features and variations it asks
/// for.
///
/// The features are what `font-variant-alternates` names through `@font-feature-values`, then
/// `font-feature-settings`, as Chrome orders them: the other `font-variant-*` properties are
/// winkin's to turn into features, and to synthesize where the font has none.
pub(crate) struct FontLists<'a> {
    families: Vec<FontFamilyName<'a>>,
    features: Vec<FontFeature>,
    variations: Vec<FontVariation>,
}

impl<'a> FontLists<'a> {
    /// The lists `style` asks for, borrowing its family names.
    pub(crate) fn of<S: TextStyle>(style: &'a S, values: &impl FontFeatureValues) -> Self {
        // The initial `font-family` names no family: winkin then sets the text in the language's
        // standard font, as Chrome sets it, where `serif` would take the serif setting, which
        // differs for Chinese, Japanese and Korean.
        let families = if style.font_family_is_initial() {
            Vec::new()
        } else {
            style.font_family().map(family_name).collect()
        };
        let mut features = Vec::new();
        resolve_alternates(style, values, &mut features);
        features.extend(style.font_feature_settings());
        Self {
            families,
            features,
            variations: style.font_variation_settings().collect(),
        }
    }

    /// These lists with `families` tried first, as a list marker's bullet font is.
    pub(crate) fn with_families_first(mut self, first: &[FontFamilyName<'static>]) -> Self {
        let rest = std::mem::take(&mut self.families);
        self.families = first.iter().cloned().chain(rest).collect();
        self
    }
}

/// A family of `font-family`, as Blitz names it: the Apple system font's legacy names are the
/// `system-ui` generic.
pub(crate) fn family_name(family: FontFamilyName<'_>) -> FontFamilyName<'_> {
    #[cfg(target_vendor = "apple")]
    if let FontFamilyName::Named(name) = &family
        && (name == "-apple-system" || name == "BlinkMacSystemFont")
    {
        return FontFamilyName::Generic(winkin::style::GenericFamily::SystemUi);
    }
    family
}

/// A `letter-spacing` or `word-spacing` as a length in device pixels and a fraction of the font
/// size, `font_size` CSS pixels.
///
/// A nonlinear `calc()` is resolved against the font size here, since winkin takes spacing only
/// as a length and a fraction.
fn spacing(value: &css::LengthPercentage, font_size: f32, scale: f32) -> LengthPercentage {
    match value {
        css::LengthPercentage::Nonlinear(calc) => {
            LengthPercentage::from_px(calc.resolve(font_size) * scale)
        }
        _ => calc::linear(value, scale),
    }
}

/// Converts an element's computed style, its lists borrowed from `lists`.
///
/// Lengths are scaled by `scale` into device pixels. Percentages of the containing block stay
/// percentages, which winkin resolves against the basis each measure gives it; a margin's or
/// padding's nonlinear `calc()` is kept in `calcs`. `language` is the content language from the
/// nearest `lang`.
pub(crate) fn computed_style<'a, S: TextStyle>(
    lists: &'a FontLists<'_>,
    style: &'a S,
    calcs: &CalcValues,
    scale: f32,
    language: Option<Language>,
) -> ComputedStyle<'a> {
    ComputedStyle {
        font: font_group(style, lists, scale),
        text: text_group(style, scale, language),
        line: line_group(style, scale),
        bidi: BidiGroup {
            direction: direction(style),
            unicode_bidi: unicode_bidi(style),
        },
        orientation: OrientationGroup {
            // Which way each character stands in a vertical line. It inherits, so a span may set
            // it against the block around it.
            text_orientation: match style.text_orientation() {
                css::TextOrientation::Mixed => TextOrientation::Mixed,
                css::TextOrientation::Upright => TextOrientation::Upright,
                css::TextOrientation::Sideways => TextOrientation::Sideways,
            },
            text_combine_upright: match style.text_combine_upright() {
                css::TextCombineUpright::None => TextCombineUpright::None,
                css::TextCombineUpright::All => TextCombineUpright::All,
                css::TextCombineUpright::Digits(count) => TextCombineUpright::Digits(count),
            },
        },
        edges: edges_group(style, calcs, scale),
        ruby: RubyGroup {
            position: ruby_position(style),
            align: match style.ruby_align() {
                css::RubyAlign::SpaceAround => RubyAlign::SpaceAround,
                css::RubyAlign::SpaceBetween => RubyAlign::SpaceBetween,
                css::RubyAlign::Center => RubyAlign::Center,
                css::RubyAlign::Start => RubyAlign::Start,
            },
            overhang: match style.ruby_overhang() {
                css::RubyOverhang::Auto => RubyOverhang::Auto,
                css::RubyOverhang::Spaces => RubyOverhang::Spaces,
            },
        },
        paints: paints(style),
        outlines: outlines(style),
        decorates: decorates(style),
    }
}

/// `direction`.
pub(crate) fn direction(style: &impl TextStyle) -> Direction {
    match style.direction() {
        css::Direction::Ltr => Direction::Ltr,
        css::Direction::Rtl => Direction::Rtl,
    }
}

/// `unicode-bidi`.
pub(crate) fn unicode_bidi(style: &impl TextStyle) -> UnicodeBidi {
    match style.unicode_bidi() {
        css::UnicodeBidi::Normal => UnicodeBidi::Normal,
        css::UnicodeBidi::Embed => UnicodeBidi::Embed,
        css::UnicodeBidi::Isolate => UnicodeBidi::Isolate,
        css::UnicodeBidi::BidiOverride => UnicodeBidi::BidiOverride,
        css::UnicodeBidi::IsolateOverride => UnicodeBidi::IsolateOverride,
        css::UnicodeBidi::Plaintext => UnicodeBidi::Plaintext,
    }
}

/// Whether the box paints a background, a border or a box shadow, which keeps its fragment in the
/// layout and makes a line ending in its text end exactly. An outline and a decoration line don't
/// count. What it draws is read from the node when painting.
fn paints(style: &impl TextStyle) -> bool {
    let fills = style.background_color().components[3] > 0.0;
    let widths = style.border_widths();
    let styles = style.border_styles();
    let bordered = [
        (widths.top, styles.top),
        (widths.right, styles.right),
        (widths.bottom, styles.bottom),
        (widths.left, styles.left),
    ]
    .into_iter()
    .any(|(width, style)| !style.none_or_hidden() && width > 0.0);
    fills || style.has_background_image() || bordered || style.has_box_shadow()
}

/// Whether the box draws an outline, which keeps its fragment in the layout.
fn outlines(style: &impl TextStyle) -> bool {
    let drawn = match style.outline_style() {
        css::OutlineStyle::Auto => true,
        css::OutlineStyle::Border(border) => !border.none_or_hidden(),
    };
    drawn && style.outline_width() > 0.0
}

/// Whether the element sets a decoration line that is drawn.
pub(crate) fn decorates(style: &impl TextStyle) -> bool {
    style
        .text_decoration_line()
        .intersects(css::TextDecorationLine::DRAWN)
}

fn font_group<'a>(style: &impl TextStyle, lists: &'a FontLists<'_>, scale: f32) -> FontGroup<'a> {
    let synthesis = |value: css::FontSynthesis| value == css::FontSynthesis::Auto;
    let font_style = style.font_style();
    FontGroup {
        families: &lists.families,
        features: &lists.features,
        variations: &lists.variations,
        size: style.font_size() * scale,
        weight: style.font_weight(),
        width: style.font_width(),
        style: font_style,
        synthesis: FontSynthesis {
            weight: synthesis(style.font_synthesis_weight()),
            // `oblique-only` allows a slant only where one was asked for.
            style: match style.font_synthesis_style() {
                css::FontSynthesisStyle::Auto => true,
                css::FontSynthesisStyle::None => false,
                css::FontSynthesisStyle::ObliqueOnly => {
                    matches!(font_style, css::FontStyle::Oblique(_))
                }
            },
            small_caps: synthesis(style.font_synthesis_small_caps()),
            position: synthesis(style.font_synthesis_position()),
        },
        variant_caps: match style.font_variant_caps() {
            css::FontVariantCaps::Normal => FontVariantCaps::Normal,
            css::FontVariantCaps::SmallCaps => FontVariantCaps::SmallCaps,
            css::FontVariantCaps::AllSmallCaps => FontVariantCaps::AllSmallCaps,
            css::FontVariantCaps::PetiteCaps => FontVariantCaps::PetiteCaps,
            css::FontVariantCaps::AllPetiteCaps => FontVariantCaps::AllPetiteCaps,
            css::FontVariantCaps::Unicase => FontVariantCaps::Unicase,
            css::FontVariantCaps::TitlingCaps => FontVariantCaps::TitlingCaps,
        },
        variant_position: match style.font_variant_position() {
            css::FontVariantPosition::Normal => FontVariantPosition::Normal,
            css::FontVariantPosition::Sub => FontVariantPosition::Sub,
            css::FontVariantPosition::Super => FontVariantPosition::Super,
        },
        variants: variants(style),
        variant_emoji: match style.font_variant_emoji() {
            css::FontVariantEmoji::Normal => FontVariantEmoji::Normal,
            css::FontVariantEmoji::Text => FontVariantEmoji::Text,
            css::FontVariantEmoji::Emoji => FontVariantEmoji::Emoji,
            css::FontVariantEmoji::Unicode => FontVariantEmoji::Unicode,
        },
        optical_sizing: match style.font_optical_sizing() {
            css::FontOpticalSizing::Auto => FontOpticalSizing::Auto,
            css::FontOpticalSizing::None => FontOpticalSizing::None,
        },
        kerning: match style.font_kerning() {
            css::FontKerning::Auto => FontKerning::Auto,
            css::FontKerning::Normal => FontKerning::Normal,
            css::FontKerning::None => FontKerning::None,
        },
        language_override: match style.font_language_override() {
            None => FontLanguageOverride::Normal,
            Some(tag) => FontLanguageOverride::System(tag),
        },
        size_adjust: match style.font_size_adjust() {
            css::FontSizeAdjust::None => FontSizeAdjust::None,
            css::FontSizeAdjust::ExHeight(value) => adjust(AdjustMetric::ExHeight, value),
            css::FontSizeAdjust::CapHeight(value) => adjust(AdjustMetric::CapHeight, value),
            css::FontSizeAdjust::ChWidth(value) => adjust(AdjustMetric::ChWidth, value),
            css::FontSizeAdjust::IcWidth(value) => adjust(AdjustMetric::IcWidth, value),
            css::FontSizeAdjust::IcHeight(value) => adjust(AdjustMetric::IcHeight, value),
        },
    }
}

fn adjust(metric: AdjustMetric, value: f32) -> FontSizeAdjust {
    FontSizeAdjust::Hold { metric, value }
}

/// The keywords of `font-variant-ligatures`, `-numeric`, `-east-asian` and `-alternates` as one
/// set.
fn variants(style: &impl TextStyle) -> FontVariants {
    use css::{FontVariantEastAsian as E, FontVariantLigatures as L, FontVariantNumeric as N};
    let mut set = FontVariants::NORMAL;
    let mut add = |on: bool, keyword: FontVariants| {
        if on {
            set = set.union(keyword);
        }
    };
    let ligatures = style.font_variant_ligatures();
    if ligatures.contains(L::NONE) {
        // `none` is every `no-` keyword.
        add(true, FontVariants::NO_COMMON_LIGATURES);
        add(true, FontVariants::NO_DISCRETIONARY_LIGATURES);
        add(true, FontVariants::NO_HISTORICAL_LIGATURES);
        add(true, FontVariants::NO_CONTEXTUAL);
    }
    for (bit, keyword) in [
        (L::COMMON_LIGATURES, FontVariants::COMMON_LIGATURES),
        (L::NO_COMMON_LIGATURES, FontVariants::NO_COMMON_LIGATURES),
        (
            L::DISCRETIONARY_LIGATURES,
            FontVariants::DISCRETIONARY_LIGATURES,
        ),
        (
            L::NO_DISCRETIONARY_LIGATURES,
            FontVariants::NO_DISCRETIONARY_LIGATURES,
        ),
        (L::HISTORICAL_LIGATURES, FontVariants::HISTORICAL_LIGATURES),
        (
            L::NO_HISTORICAL_LIGATURES,
            FontVariants::NO_HISTORICAL_LIGATURES,
        ),
        (L::CONTEXTUAL, FontVariants::CONTEXTUAL),
        (L::NO_CONTEXTUAL, FontVariants::NO_CONTEXTUAL),
    ] {
        add(ligatures.contains(bit), keyword);
    }
    let numeric = style.font_variant_numeric();
    for (bit, keyword) in [
        (N::LINING_NUMS, FontVariants::LINING_NUMS),
        (N::OLDSTYLE_NUMS, FontVariants::OLDSTYLE_NUMS),
        (N::PROPORTIONAL_NUMS, FontVariants::PROPORTIONAL_NUMS),
        (N::TABULAR_NUMS, FontVariants::TABULAR_NUMS),
        (N::DIAGONAL_FRACTIONS, FontVariants::DIAGONAL_FRACTIONS),
        (N::STACKED_FRACTIONS, FontVariants::STACKED_FRACTIONS),
        (N::ORDINAL, FontVariants::ORDINAL),
        (N::SLASHED_ZERO, FontVariants::SLASHED_ZERO),
    ] {
        add(numeric.contains(bit), keyword);
    }
    let east_asian = style.font_variant_east_asian();
    for (bit, keyword) in [
        (E::JIS78, FontVariants::JIS78),
        (E::JIS83, FontVariants::JIS83),
        (E::JIS90, FontVariants::JIS90),
        (E::JIS04, FontVariants::JIS04),
        (E::SIMPLIFIED, FontVariants::SIMPLIFIED),
        (E::TRADITIONAL, FontVariants::TRADITIONAL),
        (E::FULL_WIDTH, FontVariants::FULL_WIDTH),
        (E::PROPORTIONAL_WIDTH, FontVariants::PROPORTIONAL_WIDTH),
        (E::RUBY, FontVariants::RUBY),
    ] {
        add(east_asian.contains(bit), keyword);
    }
    // The other alternates name `@font-feature-values`, and are features of the font lists.
    let historical = style
        .font_variant_alternates()
        .any(|alternate| matches!(alternate, css::VariantAlternate::HistoricalForms));
    add(historical, FontVariants::HISTORICAL_FORMS);
    set
}

fn text_group<S: TextStyle>(style: &S, scale: f32, language: Option<Language>) -> TextGroup<'_> {
    let transform = style.text_transform();
    let emphasis = emphasis(style, language.as_ref());
    TextGroup {
        language,
        hyphenate_character: style.hyphenate_character(),
        white_space_collapse: match style.white_space_collapse() {
            css::WhiteSpaceCollapse::Collapse => WhiteSpaceCollapse::Collapse,
            css::WhiteSpaceCollapse::Preserve => WhiteSpaceCollapse::Preserve,
            css::WhiteSpaceCollapse::PreserveBreaks => WhiteSpaceCollapse::PreserveBreaks,
            css::WhiteSpaceCollapse::BreakSpaces => WhiteSpaceCollapse::BreakSpaces,
            css::WhiteSpaceCollapse::PreserveSpaces => WhiteSpaceCollapse::PreserveSpaces,
        },
        wrap_mode: match style.text_wrap_mode() {
            css::TextWrapMode::Wrap => TextWrapMode::Wrap,
            css::TextWrapMode::NoWrap => TextWrapMode::NoWrap,
        },
        // math-auto excludes the other transforms.
        transform: if transform.contains(css::TextTransform::MATH_AUTO) {
            TextTransform::MATH_AUTO
        } else {
            TextTransform {
                case: if transform.contains(css::TextTransform::UPPERCASE) {
                    TextCase::Uppercase
                } else if transform.contains(css::TextTransform::LOWERCASE) {
                    TextCase::Lowercase
                } else if transform.contains(css::TextTransform::CAPITALIZE) {
                    TextCase::Capitalize
                } else {
                    TextCase::None
                },
                full_width: transform.contains(css::TextTransform::FULL_WIDTH),
                full_size_kana: transform.contains(css::TextTransform::FULL_SIZE_KANA),
            }
        },
        security: match style.text_security() {
            css::TextSecurity::None => winkin::style::TextSecurity::None,
            css::TextSecurity::Disc => winkin::style::TextSecurity::Disc,
            css::TextSecurity::Circle => winkin::style::TextSecurity::Circle,
            css::TextSecurity::Square => winkin::style::TextSecurity::Square,
        },
        word_break: match style.word_break() {
            css::WordBreak::Normal | css::WordBreak::BreakWord => WordBreak::Normal,
            css::WordBreak::BreakAll => WordBreak::BreakAll,
            css::WordBreak::KeepAll => WordBreak::KeepAll,
        },
        line_break: match style.line_break() {
            css::LineBreak::Auto => LineBreak::Auto,
            css::LineBreak::Normal => LineBreak::Normal,
            css::LineBreak::Loose => LineBreak::Loose,
            css::LineBreak::Strict => LineBreak::Strict,
            css::LineBreak::Anywhere => LineBreak::Anywhere,
        },
        // `word-break: break-word` is `word-break: normal` with `overflow-wrap: anywhere`.
        overflow_wrap: match (style.word_break(), style.overflow_wrap()) {
            (css::WordBreak::BreakWord, _) => OverflowWrap::Anywhere,
            (_, css::OverflowWrap::Normal) => OverflowWrap::Normal,
            (_, css::OverflowWrap::BreakWord) => OverflowWrap::BreakWord,
            (_, css::OverflowWrap::Anywhere) => OverflowWrap::Anywhere,
        },
        hyphens: match style.hyphens() {
            css::Hyphens::None => Hyphens::None,
            css::Hyphens::Manual => Hyphens::Manual,
            css::Hyphens::Auto => Hyphens::Auto,
        },
        tab_size: match style.tab_size() {
            css::TabSize::Spaces(spaces) => TabSize::Spaces(spaces),
            css::TabSize::Length(length) => TabSize::Px(length * scale),
        },
        // A percentage is of the font size, which layout takes it of.
        letter_spacing: spacing(&style.letter_spacing(), style.font_size(), scale),
        // A percentage is of the font size, which layout takes it of.
        word_spacing: spacing(&style.word_spacing(), style.font_size(), scale),
        autospace: {
            let autospace = style.text_autospace();
            if autospace.intersects(css::TextAutospace::NORMAL | css::TextAutospace::AUTO) {
                TextAutospace::NORMAL
            } else {
                TextAutospace {
                    ideograph_alpha: autospace.contains(css::TextAutospace::IDEOGRAPH_ALPHA),
                    ideograph_numeric: autospace.contains(css::TextAutospace::IDEOGRAPH_NUMERIC),
                }
            }
        },
        justify: match style.text_justify() {
            css::TextJustify::Auto => TextJustify::Auto,
            css::TextJustify::None => TextJustify::None,
            css::TextJustify::InterWord => TextJustify::InterWord,
            css::TextJustify::InterCharacter => TextJustify::InterCharacter,
        },
        spacing_trim: match style.text_spacing_trim() {
            css::TextSpacingTrim::Normal => TextSpacingTrim::Normal,
            css::TextSpacingTrim::SpaceAll => TextSpacingTrim::SpaceAll,
            css::TextSpacingTrim::SpaceFirst => TextSpacingTrim::SpaceFirst,
            css::TextSpacingTrim::TrimStart => TextSpacingTrim::TrimStart,
        },
        hanging_punctuation: {
            let hanging = style.hanging_punctuation();
            HangingPunctuation {
                first: hanging.contains(css::HangingPunctuation::FIRST),
                last: hanging.contains(css::HangingPunctuation::LAST),
                end: if hanging.contains(css::HangingPunctuation::FORCE_END) {
                    HangEnd::Force
                } else if hanging.contains(css::HangingPunctuation::ALLOW_END) {
                    HangEnd::Allow
                } else {
                    HangEnd::None
                },
            }
        },
        emphasis,
        decoration_skip_spaces: match style.text_decoration_skip_spaces() {
            css::TextDecorationSkipSpaces::None => TextDecorationSkipSpaces::None,
            css::TextDecorationSkipSpaces::All => TextDecorationSkipSpaces::All,
            css::TextDecorationSkipSpaces::Start => TextDecorationSkipSpaces::Start,
            css::TextDecorationSkipSpaces::End => TextDecorationSkipSpaces::End,
            css::TextDecorationSkipSpaces::StartEnd => TextDecorationSkipSpaces::StartEnd,
        },
        ..TextGroup::INITIAL
    }
}

/// The string `text-emphasis-style` marks text with, where it marks it: a string's first
/// character, or a keyword's character.
pub(crate) fn emphasis_mark_string(style: &impl TextStyle) -> Option<String> {
    match style.text_emphasis_style() {
        css::TextEmphasisStyle::None => None,
        css::TextEmphasisStyle::String(string) => string.chars().next().map(String::from),
        css::TextEmphasisStyle::Keyword { fill, shape } => {
            let (filled, open) = match shape {
                css::EmphasisShape::Dot => ('\u{2022}', '\u{25E6}'),
                css::EmphasisShape::Circle => ('\u{25CF}', '\u{25CB}'),
                css::EmphasisShape::DoubleCircle => ('\u{25C9}', '\u{25CE}'),
                css::EmphasisShape::Triangle => ('\u{25B2}', '\u{25B3}'),
                css::EmphasisShape::Sesame => ('\u{FE45}', '\u{FE46}'),
            };
            Some(String::from(if fill == css::EmphasisFill::Open {
                open
            } else {
                filled
            }))
        }
    }
}

/// `text-emphasis` as far as layout reads it: whether marks are set, and on which side. `auto` is
/// `under right` where the content `language` is Chinese and `over right` otherwise, as CSS Text
/// Decoration 4 and Chrome have it.
fn emphasis(style: &impl TextStyle, language: Option<&Language>) -> TextEmphasis {
    let marks = !matches!(style.text_emphasis_style(), css::TextEmphasisStyle::None);
    let position = style.text_emphasis_position();
    TextEmphasis {
        marks,
        position: EmphasisPosition {
            side: if position.contains(css::TextEmphasisPosition::UNDER)
                || (position.contains(css::TextEmphasisPosition::AUTO)
                    && language
                        .is_some_and(|language| language.language().eq_ignore_ascii_case("zh")))
            {
                EmphasisSide::Under
            } else {
                EmphasisSide::Over
            },
            vertical_side: if position.contains(css::TextEmphasisPosition::LEFT) {
                EmphasisVerticalSide::Left
            } else {
                EmphasisVerticalSide::Right
            },
        },
        skip: EmphasisSkip::INITIAL,
    }
}

fn line_group(style: &impl TextStyle, scale: f32) -> LineGroup {
    let initial_letter = style.initial_letter();
    LineGroup {
        height: match style.line_height() {
            css::LineHeight::Normal => LineHeight::Normal,
            css::LineHeight::Number(number) => LineHeight::Factor(number),
            css::LineHeight::Length(length) => LineHeight::Px(length * scale),
        },
        vertical_align: vertical_align(style, scale),
        dominant_baseline: match style.dominant_baseline() {
            css::DominantBaseline::Alphabetic => DominantBaseline::Alphabetic,
            css::DominantBaseline::Ideographic => DominantBaseline::Ideographic,
            css::DominantBaseline::Central | css::DominantBaseline::Middle => {
                DominantBaseline::Central
            }
            css::DominantBaseline::Mathematical => DominantBaseline::Mathematical,
            css::DominantBaseline::Auto
            | css::DominantBaseline::Hanging
            | css::DominantBaseline::TextTop
            | css::DominantBaseline::TextBottom => DominantBaseline::Auto,
        },
        // A sink left out, or `drop`, is the size, floored; `raise` is 1.
        initial_letter: if initial_letter.size > 0.0 {
            InitialLetter {
                size: initial_letter.size,
                sink: match initial_letter.sink {
                    css::InitialLetterSink::Lines(sink) if sink > 0 => sink as u32,
                    css::InitialLetterSink::Raise => 1,
                    _ => (initial_letter.size.floor() as u32).max(1),
                },
                align: InitialLetterAlign::Alphabetic,
            }
        } else {
            InitialLetter::NONE
        },
        text_box_trim: text_box_trim(style.text_box_trim()),
        text_box_edge: text_box_edge(style.text_box_edge()),
        ..LineGroup::INITIAL
    }
}

fn text_box_trim(trim: css::TextBoxTrim) -> TextBoxTrim {
    match trim {
        css::TextBoxTrim::TrimBoth => TextBoxTrim::TrimBoth,
        css::TextBoxTrim::TrimStart => TextBoxTrim::TrimStart,
        css::TextBoxTrim::TrimEnd => TextBoxTrim::TrimEnd,
        css::TextBoxTrim::None => TextBoxTrim::None,
    }
}

fn text_box_edge(edge: css::TextBoxEdge) -> TextBoxEdge {
    match edge {
        css::TextBoxEdge::Auto => TextBoxEdge::AUTO,
        css::TextBoxEdge::Edges { over, under } => TextBoxEdge {
            over: match over {
                css::TextEdge::Cap => TextEdgeOver::Cap,
                css::TextEdge::Ex => TextEdgeOver::Ex,
                css::TextEdge::Ideographic => TextEdgeOver::Ideographic,
                css::TextEdge::IdeographicInk => TextEdgeOver::IdeographicInk,
                css::TextEdge::Text | css::TextEdge::Alphabetic => TextEdgeOver::Text,
            },
            under: match under {
                css::TextEdge::Alphabetic => TextEdgeUnder::Alphabetic,
                css::TextEdge::Ideographic => TextEdgeUnder::Ideographic,
                css::TextEdge::IdeographicInk => TextEdgeUnder::IdeographicInk,
                css::TextEdge::Text | css::TextEdge::Cap | css::TextEdge::Ex => TextEdgeUnder::Text,
            },
        },
    }
}

/// Margin, border and padding, and `box-decoration-break`.
///
/// A border width is snapped as CSS snaps it, in device pixels: one under a pixel to a pixel,
/// and any other down to a whole one. A nonlinear `calc()` is kept in `calcs`.
fn edges_group(style: &impl TextStyle, calcs: &CalcValues, scale: f32) -> EdgesGroup {
    let margin = style.margin();
    let padding = style.padding();
    let widths = style.border_widths();
    let styles = style.border_styles();
    let length_percentage = |value: &css::LengthPercentage| calcs.length_percentage(value, scale);
    let margin_side = |value: &css::LengthPercentageOrAuto| match value {
        css::LengthPercentageOrAuto::LengthPercentage(length) => length_percentage(length),
        css::LengthPercentageOrAuto::Auto => LengthPercentageOrCalc::ZERO,
    };
    let border_side = |width: f32, style: css::BorderStyle| {
        let width = width * scale;
        if style.none_or_hidden() || width <= 0.0 {
            0.0
        } else {
            width.floor().max(1.0)
        }
    };
    EdgesGroup {
        margin: Sides {
            top: margin_side(&margin.top),
            right: margin_side(&margin.right),
            bottom: margin_side(&margin.bottom),
            left: margin_side(&margin.left),
        },
        border: Sides {
            top: border_side(widths.top, styles.top),
            right: border_side(widths.right, styles.right),
            bottom: border_side(widths.bottom, styles.bottom),
            left: border_side(widths.left, styles.left),
        },
        padding: Sides {
            top: length_percentage(&padding.top),
            right: length_percentage(&padding.right),
            bottom: length_percentage(&padding.bottom),
            left: length_percentage(&padding.left),
        },
        decoration_break: match style.box_decoration_break() {
            css::BoxDecorationBreak::Slice => BoxDecorationBreak::Slice,
            css::BoxDecorationBreak::Clone => BoxDecorationBreak::Clone,
        },
    }
}

/// `vertical-align`. `alignment-baseline` and `baseline-shift` apply in SVG only, as in Chrome,
/// so HTML boxes never read them.
fn vertical_align(style: &impl TextStyle, scale: f32) -> VerticalAlign {
    match style.vertical_align() {
        css::VerticalAlign::Baseline => VerticalAlign::Baseline,
        css::VerticalAlign::Sub => VerticalAlign::Sub,
        css::VerticalAlign::Super => VerticalAlign::Super,
        css::VerticalAlign::Top => VerticalAlign::Top,
        css::VerticalAlign::TextTop => VerticalAlign::TextTop,
        css::VerticalAlign::Middle => VerticalAlign::Middle,
        css::VerticalAlign::Bottom => VerticalAlign::Bottom,
        css::VerticalAlign::TextBottom => VerticalAlign::TextBottom,
        css::VerticalAlign::Length(css::LengthPercentage::Length(length)) if length != 0.0 => {
            VerticalAlign::Px(length * scale)
        }
        css::VerticalAlign::Length(css::LengthPercentage::Percentage(percentage))
            if percentage != 0.0 =>
        {
            VerticalAlign::Fraction(percentage)
        }
        css::VerticalAlign::Length(_) => VerticalAlign::Baseline,
    }
}

/// Which way the block's lines run.
pub(crate) fn writing_mode(style: &impl TextStyle) -> WritingMode {
    match style.writing_mode() {
        css::WritingMode::HorizontalTb => WritingMode::HorizontalTb,
        css::WritingMode::VerticalRl => WritingMode::VerticalRl,
        css::WritingMode::VerticalLr => WritingMode::VerticalLr,
        css::WritingMode::SidewaysRl => WritingMode::SidewaysRl,
        css::WritingMode::SidewaysLr => WritingMode::SidewaysLr,
    }
}

/// The block's `text-align` and `text-align-last`.
pub(crate) fn text_align(style: &impl TextStyle) -> (TextAlign, TextAlignLast) {
    let align = match style.text_align() {
        css::TextAlign::Start => TextAlign::Start,
        css::TextAlign::End => TextAlign::End,
        css::TextAlign::Left => TextAlign::Left,
        css::TextAlign::Right => TextAlign::Right,
        css::TextAlign::Center => TextAlign::Center,
        css::TextAlign::Justify => TextAlign::Justify,
    };
    let last = match style.text_align_last() {
        css::TextAlignLast::Auto => TextAlignLast::Auto,
        css::TextAlignLast::Start => TextAlignLast::Start,
        css::TextAlignLast::End => TextAlignLast::End,
        css::TextAlignLast::Left => TextAlignLast::Left,
        css::TextAlignLast::Right => TextAlignLast::Right,
        css::TextAlignLast::Center => TextAlignLast::Center,
        css::TextAlignLast::Justify => TextAlignLast::Justify,
    };
    (align, last)
}

/// The block's styling: its own style and its `::first-line` style, and the properties with one
/// value per block container, read from `block`.
///
/// `container` is the box whose overflow decides whether `text-overflow` applies, which is the
/// block itself or, for an anonymous block, the box around it. A nonlinear `text-indent` is kept
/// in `calcs`.
pub(crate) fn block_style<'a>(
    style: &'a ComputedStyle<'a>,
    first_line: Option<&'a ComputedStyle<'a>>,
    block: &impl TextStyle,
    container: &impl TextStyle,
    calcs: &CalcValues,
    scale: f32,
) -> ComputedBlockStyle<'a> {
    let (text_align, text_align_last) = text_align(block);
    let indent = block.text_indent();
    // `plaintext` on the block takes each paragraph's direction from its own first strong
    // character.
    let direction = if style.bidi.unicode_bidi == UnicodeBidi::Plaintext {
        BaseDirection::Auto
    } else if style.bidi.direction == Direction::Rtl {
        BaseDirection::Rtl
    } else {
        BaseDirection::Ltr
    };
    // An ellipsis where the block clips what overflows it, as Chrome's
    // `ShouldTruncateOverflowingText` asks: the end side's value, which is the only side a line
    // overflows at.
    let clips = container.overflow_x() != css::Overflow::Visible;
    let text_overflow = match block.text_overflow() {
        css::TextOverflow::Ellipsis | css::TextOverflow::String if clips => TextOverflow::Ellipsis,
        _ => TextOverflow::Clip,
    };
    ComputedBlockStyle {
        first_line,
        direction,
        writing_mode: writing_mode(block),
        text_align,
        text_align_last,
        text_indent: TextIndent {
            amount: calcs.length_percentage(&indent.length, scale),
            hanging: indent.hanging,
            each_line: indent.each_line,
        },
        text_wrap_style: match block.text_wrap_style() {
            css::TextWrapStyle::Auto => TextWrapStyle::Auto,
            css::TextWrapStyle::Stable => TextWrapStyle::Stable,
            css::TextWrapStyle::Balance => TextWrapStyle::Balance,
            css::TextWrapStyle::Pretty => TextWrapStyle::Pretty,
        },
        text_overflow,
        // `line-clamp`, or where it is `none`, `-webkit-line-clamp`, which Chrome applies to a
        // `-webkit-box`; the box is not required here.
        line_clamp: match block.line_clamp() {
            css::LineClamp::Lines(lines) => LineClamp::Lines(lines.max(0) as u32),
            css::LineClamp::Auto => LineClamp::Auto,
            css::LineClamp::None => LineClamp::None,
        },
        text_box_trim: text_box_trim(block.text_box_trim()),
        text_box_edge: text_box_edge(block.text_box_edge()),
        text_fit: text_fit(block.text_fit()),
        ..ComputedBlockStyle::new(style)
    }
}

/// `text-fit`, its limit as a fraction.
fn text_fit(fit: css::TextFit) -> TextFit {
    TextFit {
        kind: match fit.kind {
            css::TextFitKind::None => TextFitKind::None,
            css::TextFitKind::Grow => TextFitKind::Grow,
            css::TextFitKind::Shrink => TextFitKind::Shrink,
        },
        target: match fit.target {
            css::TextFitTarget::Consistent => TextFitTarget::Consistent,
            css::TextFitTarget::PerLine => TextFitTarget::PerLine,
            css::TextFitTarget::PerLineAll => TextFitTarget::PerLineAll,
        },
        limit: fit.limit,
    }
}

/// `ruby-position`.
///
/// `alternate` alone is `alternate over`; the initial value is Chrome's `over`, so the two are
/// told apart.
pub(super) fn ruby_position(style: &impl TextStyle) -> RubyPosition {
    ruby_position_value(style.ruby_position())
}

/// `position`, a computed `ruby-position`, as winkin sets it.
pub(super) fn ruby_position_value(position: css::RubyPosition) -> RubyPosition {
    match position {
        css::RubyPosition::AlternateOver => RubyPosition::Alternate,
        css::RubyPosition::AlternateUnder => RubyPosition::AlternateUnder,
        css::RubyPosition::Over => RubyPosition::Over,
        css::RubyPosition::Under => RubyPosition::Under,
    }
}

#[cfg(test)]
mod tests {
    use blitz_text::OwnedTextStyle;
    use blitz_text::style::{self as css, FontFamilyName};

    use super::{CalcValues, FontLists, computed_style};

    /// The initial `font-family` names no family, which winkin sets in the language's standard
    /// font; a family a style sheet names is kept.
    #[test]
    fn the_initial_family_names_no_family() {
        let initial = OwnedTextStyle::default();
        let lists = FontLists::of(&initial, &());
        assert!(
            computed_style(&lists, &initial, &CalcValues::default(), 1.0, None)
                .font
                .families
                .is_empty()
        );
        let named = OwnedTextStyle::with_families([FontFamilyName::named("Ahem")]);
        let lists = FontLists::of(&named, &());
        assert_eq!(
            computed_style(&lists, &named, &CalcValues::default(), 1.0, None)
                .font
                .families
                .len(),
            1
        );
    }

    /// Lengths are scaled into device pixels, and percentages kept as fractions.
    #[test]
    fn lengths_are_device_pixels() {
        let style = OwnedTextStyle {
            letter_spacing: css::LengthPercentage::Length(2.0),
            word_spacing: css::LengthPercentage::Percentage(0.5),
            ..OwnedTextStyle::default()
        };
        let lists = FontLists::of(&style, &());
        let computed = computed_style(&lists, &style, &CalcValues::default(), 2.0, None);
        assert_eq!(computed.text.letter_spacing.px, 4.0);
        assert_eq!(computed.text.word_spacing.fraction, 0.5);
        assert_eq!(computed.font.size, 32.0);
    }

    /// A nonlinear `letter-spacing` resolves against the font size as the style converts:
    /// `max(1px, 10%)` of 16px is 1.6px, 3.2 device pixels at a scale of 2.
    #[test]
    fn a_nonlinear_spacing_resolves_at_the_font_size() {
        let style = OwnedTextStyle {
            letter_spacing: css::LengthPercentage::Nonlinear(css::Calc::new(|basis| {
                f32::max(1.0, 0.1 * basis)
            })),
            ..OwnedTextStyle::default()
        };
        let lists = FontLists::of(&style, &());
        let computed = computed_style(&lists, &style, &CalcValues::default(), 2.0, None);
        assert_eq!(computed.text.letter_spacing.px, 3.2);
        assert_eq!(computed.text.letter_spacing.fraction, 0.0);
    }
}
