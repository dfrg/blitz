//! Stylo's computed values as the text backends read them: [`StyloStyle`].
//!
//! Each accessor matches Stylo's value exhaustively onto blitz-text's, so that a change to Stylo's
//! values is a compile error here rather than a value the backends misread.

use std::borrow::Cow;
use std::ops::Deref;

use blitz_text::style::{self as text, Color, FontFamilyName, TextStyle};
use style::computed_values::box_decoration_break::T as StyloBoxDecorationBreak;
use style::computed_values::direction::T as StyloDirection;
use style::computed_values::font_kerning::T as StyloFontKerning;
use style::computed_values::font_optical_sizing::T as StyloFontOpticalSizing;
use style::computed_values::font_synthesis_style::T as StyloFontSynthesisStyle;
use style::computed_values::font_variant_caps::T as StyloFontVariantCaps;
use style::computed_values::font_variant_emoji::T as StyloFontVariantEmoji;
use style::computed_values::font_variant_position::T as StyloFontVariantPosition;
use style::computed_values::hyphens::T as StyloHyphens;
use style::computed_values::position::T as StyloPosition;
use style::computed_values::ruby_align::T as StyloRubyAlign;
use style::computed_values::ruby_overhang::T as StyloRubyOverhang;
use style::computed_values::text_decoration_style::T as StyloTextDecorationStyle;
use style::computed_values::text_orientation::T as StyloTextOrientation;
use style::computed_values::text_spacing_trim::T as StyloTextSpacingTrim;
use style::computed_values::text_wrap_mode::T as StyloTextWrapMode;
use style::computed_values::text_wrap_style::T as StyloTextWrapStyle;
use style::computed_values::unicode_bidi::T as StyloUnicodeBidi;
use style::computed_values::white_space_collapse::T as StyloWhiteSpaceCollapse;
use style::logical_geometry::WritingMode as StyloWritingMode;
use style::properties::ComputedValues;
use style::servo_arc::Arc as ServoArc;
use style::values::computed::ToComputedValue;
use style::values::computed::font::{
    FontFamily as StyloFontFamily, FontStyle as StyloFontStyle,
    FontSynthesis as StyloFontSynthesis, FontVariantEastAsian, FontVariantLigatures,
    FontVariantNumeric, GenericFontFamily as StyloGenericFontFamily, LineHeight as StyloLineHeight,
    SingleFontFamily,
};
use style::values::computed::length_percentage::Unpacked;
use style::values::computed::text::{
    TextAlignLast as StyloTextAlignLast, TextEmphasisStyle as StyloTextEmphasisStyle,
    TextFitKind as StyloTextFitKind, TextFitTarget as StyloTextFitTarget,
};
use style::values::computed::{
    Display as StyloDisplay, Float as StyloFloat, HangingPunctuation as StyloHangingPunctuation,
    Length, LengthPercentage as StyloLengthPercentage, LineBreak as StyloLineBreak,
    OverflowWrap as StyloOverflowWrap, TextCombineUpright as StyloTextCombineUpright,
    TextSecurity as StyloTextSecurity, TextTransform as StyloTextTransform,
    WordBreak as StyloWordBreak,
};
use style::values::generics::box_::{GenericVerticalAlign, VerticalAlignKeyword};
use style::values::generics::calc::GenericCalcNode;
use style::values::generics::font::GenericFontSizeAdjust;
use style::values::generics::length::{GenericLengthOrNumber, GenericMargin};
use style::values::generics::text::{
    GenericInitialLetterSink, GenericTextDecorationInset, GenericTextDecorationLength,
};
use style::values::specified::LengthPercentage as SpecifiedLengthPercentage;
use style::values::specified::OutlineStyle as StyloOutlineStyle;
use style::values::specified::TextAlignKeyword;
use style::values::specified::border::BorderStyle as StyloBorderStyle;
use style::values::specified::box_::{
    DisplayInside as StyloDisplayInside, DisplayOutside as StyloDisplayOutside,
    DominantBaseline as StyloDominantBaseline, Overflow as StyloOverflow,
};
use style::values::specified::calc::Leaf as CalcLeaf;
use style::values::specified::font::VariantAlternates;
use style::values::specified::text::{
    HyphenateCharacter, RubyPosition as StyloRubyPosition, TextAutospace as StyloTextAutospace,
    TextBoxEdge as StyloTextBoxEdge, TextBoxTrim as StyloTextBoxTrim,
    TextDecorationLine as StyloTextDecorationLine,
    TextDecorationSkipSpaces as StyloTextDecorationSkipSpaces, TextEdgeKeyword,
    TextEmphasisFillMode, TextEmphasisPosition as StyloTextEmphasisPosition,
    TextEmphasisShapeKeyword, TextJustify as StyloTextJustify,
    TextOverflowSide as StyloTextOverflowSide, TextUnderlinePosition as StyloTextUnderlinePosition,
};
use style_traits::values::specified::AllowedNumericType;

use crate::util::ToColorColor as _;

/// A Stylo computed style, as the text backends read it.
///
/// `StyloStyle` holds the style's `Arc`, so that a backend can keep it; [`StyloStyleRef`]
/// borrows it, for comparing styles that are not kept.
#[derive(Clone, Debug)]
#[repr(transparent)]
pub struct StyloStyle<P = ServoArc<ComputedValues>>(pub P);

/// A borrowed Stylo computed style.
pub type StyloStyleRef<'a> = StyloStyle<&'a ComputedValues>;

impl StyloStyle {
    /// Returns `style` as a [`StyloStyle`], without taking a reference to it.
    #[inline]
    pub fn from_arc(style: &ServoArc<ComputedValues>) -> &Self {
        // SAFETY: `StyloStyle` is a transparent wrapper of the `Arc`.
        unsafe { &*(style as *const ServoArc<ComputedValues> as *const Self) }
    }
}

impl<'a> StyloStyleRef<'a> {
    /// Returns a borrowed style.
    #[inline]
    pub fn of(style: &'a ComputedValues) -> Self {
        Self(style)
    }
}

impl<P: Deref<Target = ComputedValues>> StyloStyle<P> {
    /// Returns the computed values.
    #[inline]
    pub fn computed(&self) -> &ComputedValues {
        &self.0
    }
}

/// Whether `family` is the initial `font-family`.
///
/// Stylo's initial value is `serif`, held in one shared list that inheriting and `initial` keep;
/// a `serif` a style sheet names is parsed into a list of its own.
pub(crate) fn is_initial_family(family: &StyloFontFamily) -> bool {
    std::ptr::eq(
        family.families.list.as_ptr(),
        StyloFontFamily::generic(StyloGenericFontFamily::Serif)
            .families
            .list
            .as_ptr(),
    )
}

/// A family of `font-family`.
#[inline]
pub(crate) fn font_family_name(family: &SingleFontFamily) -> FontFamilyName<'_> {
    match family {
        SingleFontFamily::FamilyName(name) => {
            FontFamilyName::Named(Cow::Borrowed(name.name.as_str()))
        }
        SingleFontFamily::Generic(generic) => FontFamilyName::Generic(generic_family(*generic)),
    }
}

/// A generic family. Stylo's `None`, which no style sheet names, is `sans-serif`.
#[inline]
fn generic_family(generic: StyloGenericFontFamily) -> text::GenericFamily {
    match generic {
        StyloGenericFontFamily::None | StyloGenericFontFamily::SansSerif => {
            text::GenericFamily::SansSerif
        }
        StyloGenericFontFamily::Serif => text::GenericFamily::Serif,
        StyloGenericFontFamily::Monospace => text::GenericFamily::Monospace,
        StyloGenericFontFamily::Cursive => text::GenericFamily::Cursive,
        StyloGenericFontFamily::Fantasy => text::GenericFamily::Fantasy,
        StyloGenericFontFamily::SystemUi => text::GenericFamily::SystemUi,
    }
}

/// `font-style`.
#[inline]
pub(crate) fn font_style(style: StyloFontStyle) -> text::FontStyle {
    match style {
        StyloFontStyle::NORMAL => text::FontStyle::Normal,
        StyloFontStyle::ITALIC => text::FontStyle::Italic,
        oblique => text::FontStyle::Oblique(Some(oblique.oblique_degrees())),
    }
}

/// A `<length-percentage>`.
///
/// A `calc()` linear in its basis is split into its length and its fraction by resolving it
/// against two bases. Any other, such as `max(20px, 10%)`, is kept whole as a function of the
/// basis, which resolves it as Stylo does.
#[inline]
fn length_percentage(value: &StyloLengthPercentage) -> text::LengthPercentage {
    match value.unpack() {
        Unpacked::Length(length) => text::LengthPercentage::Length(length.px()),
        Unpacked::Percentage(percentage) => text::LengthPercentage::Percentage(percentage.0),
        Unpacked::Calc(_) if is_linear(value) => {
            let at_zero = value.resolve(Length::new(0.0)).px();
            let at_hundred = value.resolve(Length::new(100.0)).px();
            text::LengthPercentage::Calc {
                px: at_zero,
                fraction: (at_hundred - at_zero) / 100.0,
            }
        }
        Unpacked::Calc(_) => {
            let value = value.clone();
            text::LengthPercentage::Nonlinear(text::Calc::new(move |basis| {
                value.resolve(Length::new(basis)).px()
            }))
        }
    }
}

/// A `<length-percentage>` whose percentages are of the font size, `font_size` CSS pixels.
///
/// A `calc()` that is not linear in the font size is resolved against it here, since the font
/// size is known.
#[inline]
fn of_font_size(value: &StyloLengthPercentage, font_size: f32) -> text::LengthPercentage {
    match length_percentage(value) {
        text::LengthPercentage::Nonlinear(calc) => {
            text::LengthPercentage::Length(calc.resolve(font_size))
        }
        linear => linear,
    }
}

/// Returns whether `value` is linear in its basis: a length, a percentage, or a `calc()` that
/// only adds, negates and scales them by constants.
///
/// A `calc()` its property clamps, such as padding's at zero, is not: the clamp bends it, so
/// splitting it by two bases would carry the clamp at one of them to every basis.
fn is_linear(value: &StyloLengthPercentage) -> bool {
    if !matches!(value.unpack(), Unpacked::Calc(_)) {
        return true;
    }
    match SpecifiedLengthPercentage::from_computed_value(value) {
        SpecifiedLengthPercentage::Calc(calc) => {
            calc.0.clamping_mode == AllowedNumericType::All && node_is_linear(&calc.0.node)
        }
        _ => true,
    }
}

type CalcNode = GenericCalcNode<CalcLeaf>;

/// Returns whether `node` is linear in the basis.
fn node_is_linear(node: &CalcNode) -> bool {
    if is_constant(node) {
        return true;
    }
    match node {
        CalcNode::Leaf(_) => true,
        CalcNode::Negate(child) => node_is_linear(child),
        CalcNode::Sum(children) => children.iter().all(node_is_linear),
        CalcNode::Product(factors) => {
            let mut varying = factors.iter().filter(|factor| !is_constant(factor));
            varying.next().is_some_and(node_is_linear) && varying.next().is_none()
        }
        _ => false,
    }
}

/// Returns whether `node` holds no percentage, so it is the same at every basis.
fn is_constant(node: &CalcNode) -> bool {
    let mut percentage = false;
    let mut node = node.clone();
    node.visit_depth_first(|node| {
        if let CalcNode::Leaf(CalcLeaf::Percentage(_)) = node {
            percentage = true;
        }
    });
    !percentage
}

#[inline]
fn border_style(style: StyloBorderStyle) -> text::BorderStyle {
    match style {
        StyloBorderStyle::Hidden => text::BorderStyle::Hidden,
        StyloBorderStyle::None => text::BorderStyle::None,
        StyloBorderStyle::Inset => text::BorderStyle::Inset,
        StyloBorderStyle::Groove => text::BorderStyle::Groove,
        StyloBorderStyle::Outset => text::BorderStyle::Outset,
        StyloBorderStyle::Ridge => text::BorderStyle::Ridge,
        StyloBorderStyle::Dotted => text::BorderStyle::Dotted,
        StyloBorderStyle::Dashed => text::BorderStyle::Dashed,
        StyloBorderStyle::Solid => text::BorderStyle::Solid,
        StyloBorderStyle::Double => text::BorderStyle::Double,
    }
}

/// Sets `to` in `out` where `from` is set in `flags`.
macro_rules! map_flags {
    ($flags:expr, $out:ty, [$(($from:expr, $to:expr)),* $(,)?]) => {{
        let flags = $flags;
        let mut out = <$out>::empty();
        $(if flags.contains($from) {
            out |= $to;
        })*
        out
    }};
}

/// Which list of names a variant alternate holds.
#[derive(Clone, Copy)]
enum AlternateList {
    Styleset,
    CharacterVariant,
}

/// A variant alternate's single item, or the names of its list.
fn alternate_names(
    alternate: &VariantAlternates,
) -> (
    Option<text::VariantAlternate<'_>>,
    &[style::values::CustomIdent],
    AlternateList,
) {
    match alternate {
        VariantAlternates::Stylistic(name) => (
            Some(text::VariantAlternate::Stylistic(name.0.as_ref())),
            &[],
            AlternateList::Styleset,
        ),
        VariantAlternates::Swash(name) => (
            Some(text::VariantAlternate::Swash(name.0.as_ref())),
            &[],
            AlternateList::Styleset,
        ),
        VariantAlternates::Ornaments(name) => (
            Some(text::VariantAlternate::Ornaments(name.0.as_ref())),
            &[],
            AlternateList::Styleset,
        ),
        VariantAlternates::Annotation(name) => (
            Some(text::VariantAlternate::Annotation(name.0.as_ref())),
            &[],
            AlternateList::Styleset,
        ),
        VariantAlternates::Styleset(names) => (None, names, AlternateList::Styleset),
        VariantAlternates::CharacterVariant(names) => {
            (None, names, AlternateList::CharacterVariant)
        }
        VariantAlternates::HistoricalForms => (
            Some(text::VariantAlternate::HistoricalForms),
            &[],
            AlternateList::Styleset,
        ),
    }
}

impl<P: Deref<Target = ComputedValues> + Clone + Send + Sync> TextStyle for StyloStyle<P> {
    // Font

    #[inline]
    fn font_family(&self) -> impl Iterator<Item = FontFamilyName<'_>> + Clone {
        self.0
            .get_font()
            .font_family
            .families
            .list
            .iter()
            .map(font_family_name)
    }

    #[inline]
    fn font_family_is_initial(&self) -> bool {
        is_initial_family(&self.0.get_font().font_family)
    }

    #[inline]
    fn font_size(&self) -> f32 {
        self.0.get_font().font_size.used_size.0.px()
    }

    #[inline]
    fn font_weight(&self) -> text::FontWeight {
        text::FontWeight::new(self.0.get_font().font_weight.value())
    }

    #[inline]
    fn font_width(&self) -> text::FontWidth {
        text::FontWidth::from_percentage(self.0.get_font().font_width.0.to_float())
    }

    #[inline]
    fn font_style(&self) -> text::FontStyle {
        font_style(self.0.get_font().font_style)
    }

    #[inline]
    fn font_synthesis_weight(&self) -> text::FontSynthesis {
        synthesis(self.0.slow_clone_font_synthesis_weight())
    }

    #[inline]
    fn font_synthesis_style(&self) -> text::FontSynthesisStyle {
        match self.0.slow_clone_font_synthesis_style() {
            StyloFontSynthesisStyle::Auto => text::FontSynthesisStyle::Auto,
            StyloFontSynthesisStyle::None => text::FontSynthesisStyle::None,
            StyloFontSynthesisStyle::ObliqueOnly => text::FontSynthesisStyle::ObliqueOnly,
        }
    }

    #[inline]
    fn font_synthesis_small_caps(&self) -> text::FontSynthesis {
        synthesis(self.0.slow_clone_font_synthesis_small_caps())
    }

    #[inline]
    fn font_synthesis_position(&self) -> text::FontSynthesis {
        synthesis(self.0.slow_clone_font_synthesis_position())
    }

    #[inline]
    fn font_variant_caps(&self) -> text::FontVariantCaps {
        match self.0.get_font().font_variant_caps {
            StyloFontVariantCaps::Normal => text::FontVariantCaps::Normal,
            StyloFontVariantCaps::SmallCaps => text::FontVariantCaps::SmallCaps,
            StyloFontVariantCaps::AllSmallCaps => text::FontVariantCaps::AllSmallCaps,
            StyloFontVariantCaps::PetiteCaps => text::FontVariantCaps::PetiteCaps,
            StyloFontVariantCaps::AllPetiteCaps => text::FontVariantCaps::AllPetiteCaps,
            StyloFontVariantCaps::Unicase => text::FontVariantCaps::Unicase,
            StyloFontVariantCaps::TitlingCaps => text::FontVariantCaps::TitlingCaps,
        }
    }

    #[inline]
    fn font_variant_position(&self) -> text::FontVariantPosition {
        match self.0.get_font().font_variant_position {
            StyloFontVariantPosition::Normal => text::FontVariantPosition::Normal,
            StyloFontVariantPosition::Sub => text::FontVariantPosition::Sub,
            StyloFontVariantPosition::Super => text::FontVariantPosition::Super,
        }
    }

    #[inline]
    fn font_variant_ligatures(&self) -> text::FontVariantLigatures {
        use text::FontVariantLigatures as L;
        map_flags!(
            self.0.get_font().font_variant_ligatures,
            L,
            [
                (FontVariantLigatures::NONE, L::NONE),
                (FontVariantLigatures::COMMON_LIGATURES, L::COMMON_LIGATURES),
                (
                    FontVariantLigatures::NO_COMMON_LIGATURES,
                    L::NO_COMMON_LIGATURES
                ),
                (
                    FontVariantLigatures::DISCRETIONARY_LIGATURES,
                    L::DISCRETIONARY_LIGATURES
                ),
                (
                    FontVariantLigatures::NO_DISCRETIONARY_LIGATURES,
                    L::NO_DISCRETIONARY_LIGATURES
                ),
                (
                    FontVariantLigatures::HISTORICAL_LIGATURES,
                    L::HISTORICAL_LIGATURES
                ),
                (
                    FontVariantLigatures::NO_HISTORICAL_LIGATURES,
                    L::NO_HISTORICAL_LIGATURES
                ),
                (FontVariantLigatures::CONTEXTUAL, L::CONTEXTUAL),
                (FontVariantLigatures::NO_CONTEXTUAL, L::NO_CONTEXTUAL),
            ]
        )
    }

    #[inline]
    fn font_variant_numeric(&self) -> text::FontVariantNumeric {
        use text::FontVariantNumeric as N;
        map_flags!(
            self.0.get_font().font_variant_numeric,
            N,
            [
                (FontVariantNumeric::LINING_NUMS, N::LINING_NUMS),
                (FontVariantNumeric::OLDSTYLE_NUMS, N::OLDSTYLE_NUMS),
                (FontVariantNumeric::PROPORTIONAL_NUMS, N::PROPORTIONAL_NUMS),
                (FontVariantNumeric::TABULAR_NUMS, N::TABULAR_NUMS),
                (
                    FontVariantNumeric::DIAGONAL_FRACTIONS,
                    N::DIAGONAL_FRACTIONS
                ),
                (FontVariantNumeric::STACKED_FRACTIONS, N::STACKED_FRACTIONS),
                (FontVariantNumeric::ORDINAL, N::ORDINAL),
                (FontVariantNumeric::SLASHED_ZERO, N::SLASHED_ZERO),
            ]
        )
    }

    #[inline]
    fn font_variant_east_asian(&self) -> text::FontVariantEastAsian {
        use text::FontVariantEastAsian as E;
        map_flags!(
            self.0.get_font().font_variant_east_asian,
            E,
            [
                (FontVariantEastAsian::JIS78, E::JIS78),
                (FontVariantEastAsian::JIS83, E::JIS83),
                (FontVariantEastAsian::JIS90, E::JIS90),
                (FontVariantEastAsian::JIS04, E::JIS04),
                (FontVariantEastAsian::SIMPLIFIED, E::SIMPLIFIED),
                (FontVariantEastAsian::TRADITIONAL, E::TRADITIONAL),
                (FontVariantEastAsian::FULL_WIDTH, E::FULL_WIDTH),
                (
                    FontVariantEastAsian::PROPORTIONAL_WIDTH,
                    E::PROPORTIONAL_WIDTH
                ),
                (FontVariantEastAsian::RUBY, E::RUBY),
            ]
        )
    }

    #[inline]
    fn font_variant_alternates(&self) -> impl Iterator<Item = text::VariantAlternate<'_>> {
        self.0
            .get_font()
            .font_variant_alternates
            .iter()
            .flat_map(|alternate| {
                let (single, names, list) = alternate_names(alternate);
                single.into_iter().chain(names.iter().map(move |name| {
                    let name: &str = name.0.as_ref();
                    match list {
                        AlternateList::Styleset => text::VariantAlternate::Styleset(name),
                        AlternateList::CharacterVariant => {
                            text::VariantAlternate::CharacterVariant(name)
                        }
                    }
                }))
            })
    }

    #[inline]
    fn font_variant_emoji(&self) -> text::FontVariantEmoji {
        match self.0.slow_clone_font_variant_emoji() {
            StyloFontVariantEmoji::Normal => text::FontVariantEmoji::Normal,
            StyloFontVariantEmoji::Text => text::FontVariantEmoji::Text,
            StyloFontVariantEmoji::Emoji => text::FontVariantEmoji::Emoji,
            StyloFontVariantEmoji::Unicode => text::FontVariantEmoji::Unicode,
        }
    }

    #[inline]
    fn font_feature_settings(&self) -> impl Iterator<Item = text::FontFeature> + Clone {
        self.0
            .get_font()
            .font_feature_settings
            .0
            .iter()
            .map(|setting| {
                text::FontFeature::new(
                    text::Tag::from_bytes(setting.tag.0.to_be_bytes()),
                    setting.value as u16,
                )
            })
    }

    #[inline]
    fn font_variation_settings(&self) -> impl Iterator<Item = text::FontVariation> + Clone {
        self.0
            .get_font()
            .font_variation_settings
            .0
            .iter()
            .map(|setting| text::FontVariation {
                tag: text::Tag::from_bytes(setting.tag.0.to_be_bytes()),
                value: setting.value,
            })
    }

    #[inline]
    fn font_optical_sizing(&self) -> text::FontOpticalSizing {
        match self.0.slow_clone_font_optical_sizing() {
            StyloFontOpticalSizing::Auto => text::FontOpticalSizing::Auto,
            StyloFontOpticalSizing::None => text::FontOpticalSizing::None,
        }
    }

    #[inline]
    fn font_kerning(&self) -> text::FontKerning {
        match self.0.slow_clone_font_kerning() {
            StyloFontKerning::Auto => text::FontKerning::Auto,
            StyloFontKerning::Normal => text::FontKerning::Normal,
            StyloFontKerning::None => text::FontKerning::None,
        }
    }

    #[inline]
    fn font_language_override(&self) -> Option<text::Tag> {
        match self.0.slow_clone_font_language_override().0 {
            0 => None,
            tag => Some(text::Tag::from_bytes(tag.to_be_bytes())),
        }
    }

    #[inline]
    fn font_size_adjust(&self) -> text::FontSizeAdjust {
        match self.0.slow_clone_font_size_adjust() {
            GenericFontSizeAdjust::None => text::FontSizeAdjust::None,
            GenericFontSizeAdjust::ExHeight(value) => text::FontSizeAdjust::ExHeight(value.0),
            GenericFontSizeAdjust::CapHeight(value) => text::FontSizeAdjust::CapHeight(value.0),
            GenericFontSizeAdjust::ChWidth(value) => text::FontSizeAdjust::ChWidth(value.0),
            GenericFontSizeAdjust::IcWidth(value) => text::FontSizeAdjust::IcWidth(value.0),
            GenericFontSizeAdjust::IcHeight(value) => text::FontSizeAdjust::IcHeight(value.0),
        }
    }

    #[inline]
    fn line_height(&self) -> text::LineHeight {
        match self.0.get_font().line_height {
            StyloLineHeight::Normal => text::LineHeight::Normal,
            StyloLineHeight::Number(number) => text::LineHeight::Number(number.0),
            StyloLineHeight::Length(length) => text::LineHeight::Length(length.0.px()),
        }
    }

    #[inline]
    fn language(&self) -> &str {
        &self.0.get_font()._x_lang.0
    }

    // Inherited text

    #[inline]
    fn color(&self) -> Color {
        self.0.get_inherited_text().color.as_color_color()
    }

    #[inline]
    fn text_align(&self) -> text::TextAlign {
        match self.0.slow_clone_text_align() {
            TextAlignKeyword::Start => text::TextAlign::Start,
            TextAlignKeyword::End => text::TextAlign::End,
            TextAlignKeyword::Left | TextAlignKeyword::MozLeft => text::TextAlign::Left,
            TextAlignKeyword::Right | TextAlignKeyword::MozRight => text::TextAlign::Right,
            TextAlignKeyword::Center | TextAlignKeyword::MozCenter => text::TextAlign::Center,
            TextAlignKeyword::Justify => text::TextAlign::Justify,
        }
    }

    #[inline]
    fn text_align_last(&self) -> text::TextAlignLast {
        match self.0.get_inherited_text().text_align_last {
            StyloTextAlignLast::Auto => text::TextAlignLast::Auto,
            StyloTextAlignLast::Start => text::TextAlignLast::Start,
            StyloTextAlignLast::End => text::TextAlignLast::End,
            StyloTextAlignLast::Left => text::TextAlignLast::Left,
            StyloTextAlignLast::Right => text::TextAlignLast::Right,
            StyloTextAlignLast::Center => text::TextAlignLast::Center,
            StyloTextAlignLast::Justify => text::TextAlignLast::Justify,
        }
    }

    #[inline]
    fn text_indent(&self) -> text::TextIndent {
        let indent = &self.0.get_inherited_text().text_indent;
        text::TextIndent {
            length: length_percentage(&indent.length),
            hanging: indent.hanging,
            each_line: indent.each_line,
        }
    }

    #[inline]
    fn text_justify(&self) -> text::TextJustify {
        match self.0.get_inherited_text().text_justify {
            StyloTextJustify::Auto => text::TextJustify::Auto,
            StyloTextJustify::None => text::TextJustify::None,
            StyloTextJustify::InterWord => text::TextJustify::InterWord,
            StyloTextJustify::InterCharacter => text::TextJustify::InterCharacter,
        }
    }

    #[inline]
    fn text_transform(&self) -> text::TextTransform {
        use text::TextTransform as T;
        map_flags!(
            self.0.slow_clone_text_transform(),
            T,
            [
                (StyloTextTransform::UPPERCASE, T::UPPERCASE),
                (StyloTextTransform::LOWERCASE, T::LOWERCASE),
                (StyloTextTransform::CAPITALIZE, T::CAPITALIZE),
                (StyloTextTransform::MATH_AUTO, T::MATH_AUTO),
                (StyloTextTransform::FULL_WIDTH, T::FULL_WIDTH),
                (StyloTextTransform::FULL_SIZE_KANA, T::FULL_SIZE_KANA),
            ]
        )
    }

    #[inline]
    fn white_space_collapse(&self) -> text::WhiteSpaceCollapse {
        match self.0.get_inherited_text().white_space_collapse {
            StyloWhiteSpaceCollapse::Collapse => text::WhiteSpaceCollapse::Collapse,
            StyloWhiteSpaceCollapse::Preserve => text::WhiteSpaceCollapse::Preserve,
            StyloWhiteSpaceCollapse::PreserveBreaks => text::WhiteSpaceCollapse::PreserveBreaks,
            StyloWhiteSpaceCollapse::PreserveSpaces => text::WhiteSpaceCollapse::PreserveSpaces,
            StyloWhiteSpaceCollapse::BreakSpaces => text::WhiteSpaceCollapse::BreakSpaces,
        }
    }

    #[inline]
    fn text_wrap_mode(&self) -> text::TextWrapMode {
        match self.0.get_inherited_text().text_wrap_mode {
            StyloTextWrapMode::Wrap => text::TextWrapMode::Wrap,
            StyloTextWrapMode::Nowrap => text::TextWrapMode::NoWrap,
        }
    }

    #[inline]
    fn text_wrap_style(&self) -> text::TextWrapStyle {
        match self.0.slow_clone_text_wrap_style() {
            StyloTextWrapStyle::Auto => text::TextWrapStyle::Auto,
            StyloTextWrapStyle::Stable => text::TextWrapStyle::Stable,
            StyloTextWrapStyle::Balance => text::TextWrapStyle::Balance,
            StyloTextWrapStyle::Pretty => text::TextWrapStyle::Pretty,
        }
    }

    #[inline]
    fn word_break(&self) -> text::WordBreak {
        match self.0.get_inherited_text().word_break {
            StyloWordBreak::Normal => text::WordBreak::Normal,
            StyloWordBreak::BreakAll => text::WordBreak::BreakAll,
            StyloWordBreak::KeepAll => text::WordBreak::KeepAll,
            StyloWordBreak::BreakWord => text::WordBreak::BreakWord,
        }
    }

    #[inline]
    fn line_break(&self) -> text::LineBreak {
        match self.0.get_inherited_text().line_break {
            StyloLineBreak::Auto => text::LineBreak::Auto,
            StyloLineBreak::Loose => text::LineBreak::Loose,
            StyloLineBreak::Normal => text::LineBreak::Normal,
            StyloLineBreak::Strict => text::LineBreak::Strict,
            StyloLineBreak::Anywhere => text::LineBreak::Anywhere,
        }
    }

    #[inline]
    fn overflow_wrap(&self) -> text::OverflowWrap {
        match self.0.get_inherited_text().overflow_wrap {
            StyloOverflowWrap::Normal => text::OverflowWrap::Normal,
            StyloOverflowWrap::BreakWord => text::OverflowWrap::BreakWord,
            StyloOverflowWrap::Anywhere => text::OverflowWrap::Anywhere,
        }
    }

    #[inline]
    fn hyphens(&self) -> text::Hyphens {
        match self.0.slow_clone_hyphens() {
            StyloHyphens::None => text::Hyphens::None,
            StyloHyphens::Manual => text::Hyphens::Manual,
            StyloHyphens::Auto => text::Hyphens::Auto,
        }
    }

    #[inline]
    fn hyphenate_character(&self) -> Option<&str> {
        match &self.0.get_inherited_text().hyphenate_character {
            HyphenateCharacter::Auto => None,
            HyphenateCharacter::String(string) => Some(string),
        }
    }

    #[inline]
    fn tab_size(&self) -> text::TabSize {
        match self.0.get_inherited_text().tab_size {
            GenericLengthOrNumber::Number(spaces) => text::TabSize::Spaces(spaces.0),
            GenericLengthOrNumber::Length(length) => text::TabSize::Length(length.0.px()),
        }
    }

    #[inline]
    fn letter_spacing(&self) -> text::LengthPercentage {
        of_font_size(
            &self.0.get_inherited_text().letter_spacing.0,
            self.font_size(),
        )
    }

    #[inline]
    fn word_spacing(&self) -> text::LengthPercentage {
        of_font_size(&self.0.get_inherited_text().word_spacing, self.font_size())
    }

    #[inline]
    fn text_autospace(&self) -> text::TextAutospace {
        use text::TextAutospace as A;
        map_flags!(
            self.0.slow_clone_text_autospace(),
            A,
            [
                (StyloTextAutospace::NORMAL, A::NORMAL),
                (StyloTextAutospace::AUTO, A::AUTO),
                (StyloTextAutospace::IDEOGRAPH_ALPHA, A::IDEOGRAPH_ALPHA),
                (StyloTextAutospace::IDEOGRAPH_NUMERIC, A::IDEOGRAPH_NUMERIC),
                (StyloTextAutospace::INSERT, A::INSERT),
            ]
        )
    }

    #[inline]
    fn text_spacing_trim(&self) -> text::TextSpacingTrim {
        match self.0.slow_clone_text_spacing_trim() {
            StyloTextSpacingTrim::Normal => text::TextSpacingTrim::Normal,
            StyloTextSpacingTrim::SpaceAll => text::TextSpacingTrim::SpaceAll,
            StyloTextSpacingTrim::SpaceFirst => text::TextSpacingTrim::SpaceFirst,
            StyloTextSpacingTrim::TrimStart => text::TextSpacingTrim::TrimStart,
        }
    }

    #[inline]
    fn hanging_punctuation(&self) -> text::HangingPunctuation {
        use text::HangingPunctuation as H;
        map_flags!(
            self.0.slow_clone_hanging_punctuation(),
            H,
            [
                (StyloHangingPunctuation::FIRST, H::FIRST),
                (StyloHangingPunctuation::LAST, H::LAST),
                (StyloHangingPunctuation::FORCE_END, H::FORCE_END),
                (StyloHangingPunctuation::ALLOW_END, H::ALLOW_END),
            ]
        )
    }

    #[inline]
    fn text_emphasis_style(&self) -> text::TextEmphasisStyle<'_> {
        match &self.0.get_inherited_text().text_emphasis_style {
            StyloTextEmphasisStyle::None => text::TextEmphasisStyle::None,
            StyloTextEmphasisStyle::Keyword { fill, shape } => text::TextEmphasisStyle::Keyword {
                fill: match fill {
                    TextEmphasisFillMode::Filled => text::EmphasisFill::Filled,
                    TextEmphasisFillMode::Open => text::EmphasisFill::Open,
                },
                shape: match shape {
                    TextEmphasisShapeKeyword::Dot => text::EmphasisShape::Dot,
                    TextEmphasisShapeKeyword::Circle => text::EmphasisShape::Circle,
                    TextEmphasisShapeKeyword::DoubleCircle => text::EmphasisShape::DoubleCircle,
                    TextEmphasisShapeKeyword::Triangle => text::EmphasisShape::Triangle,
                    TextEmphasisShapeKeyword::Sesame => text::EmphasisShape::Sesame,
                },
            },
            StyloTextEmphasisStyle::String(string) => text::TextEmphasisStyle::String(string),
        }
    }

    #[inline]
    fn text_emphasis_position(&self) -> text::TextEmphasisPosition {
        use text::TextEmphasisPosition as E;
        map_flags!(
            self.0.slow_clone_text_emphasis_position(),
            E,
            [
                (StyloTextEmphasisPosition::AUTO, E::AUTO),
                (StyloTextEmphasisPosition::OVER, E::OVER),
                (StyloTextEmphasisPosition::UNDER, E::UNDER),
                (StyloTextEmphasisPosition::LEFT, E::LEFT),
                (StyloTextEmphasisPosition::RIGHT, E::RIGHT),
            ]
        )
    }

    #[inline]
    fn text_emphasis_color(&self) -> Color {
        let current = self.0.slow_clone_color();
        self.0
            .get_inherited_text()
            .text_emphasis_color
            .resolve_to_absolute(&current)
            .as_color_color()
    }

    #[inline]
    fn text_security(&self) -> text::TextSecurity {
        match self.0.get_inherited_text()._webkit_text_security {
            StyloTextSecurity::None => text::TextSecurity::None,
            StyloTextSecurity::Disc => text::TextSecurity::Disc,
            StyloTextSecurity::Circle => text::TextSecurity::Circle,
            StyloTextSecurity::Square => text::TextSecurity::Square,
        }
    }

    #[inline]
    fn text_fit(&self) -> text::TextFit {
        let fit = &self.0.get_inherited_text().text_fit;
        text::TextFit {
            kind: match fit.kind {
                StyloTextFitKind::None => text::TextFitKind::None,
                StyloTextFitKind::Grow => text::TextFitKind::Grow,
                StyloTextFitKind::Shrink => text::TextFitKind::Shrink,
            },
            target: match fit.target {
                StyloTextFitTarget::Consistent => text::TextFitTarget::Consistent,
                StyloTextFitTarget::PerLine => text::TextFitTarget::PerLine,
                StyloTextFitTarget::PerLineAll => text::TextFitTarget::PerLineAll,
            },
            limit: fit.limit.map(|limit| limit.0.0),
        }
    }

    #[inline]
    fn text_underline_offset(&self) -> Option<text::LengthPercentage> {
        self.0
            .get_inherited_text()
            .text_underline_offset
            .non_auto()
            .as_ref()
            .map(|offset| of_font_size(offset, self.font_size()))
    }

    #[inline]
    fn text_underline_position(&self) -> text::TextUnderlinePosition {
        use text::TextUnderlinePosition as U;
        map_flags!(
            self.0.get_inherited_text().text_underline_position,
            U,
            [
                (StyloTextUnderlinePosition::FROM_FONT, U::FROM_FONT),
                (StyloTextUnderlinePosition::UNDER, U::UNDER),
                (StyloTextUnderlinePosition::LEFT, U::LEFT),
                (StyloTextUnderlinePosition::RIGHT, U::RIGHT),
            ]
        )
    }

    #[inline]
    fn text_decoration_skip_spaces(&self) -> text::TextDecorationSkipSpaces {
        use text::TextDecorationSkipSpaces as S;
        let skip = self.0.get_inherited_text().text_decoration_skip_spaces;
        // `all` holds both edges.
        if skip.contains(StyloTextDecorationSkipSpaces::ALL) {
            return S::All;
        }
        match (
            skip.contains(StyloTextDecorationSkipSpaces::START),
            skip.contains(StyloTextDecorationSkipSpaces::END),
        ) {
            (true, true) => S::StartEnd,
            (true, false) => S::Start,
            (false, true) => S::End,
            (false, false) => S::None,
        }
    }

    #[inline]
    fn text_shadow(&self) -> impl Iterator<Item = text::TextShadow> + Clone {
        let current = self.0.slow_clone_color();
        self.0
            .get_inherited_text()
            .text_shadow
            .0
            .iter()
            .map(move |shadow| text::TextShadow {
                color: shadow.color.resolve_to_absolute(&current).as_color_color(),
                x: shadow.horizontal.px(),
                y: shadow.vertical.px(),
                blur: shadow.blur.0.px(),
            })
    }

    #[inline]
    fn text_shadow_count(&self) -> usize {
        self.0.get_inherited_text().text_shadow.0.len()
    }

    #[inline]
    fn ruby_position(&self) -> text::RubyPosition {
        match self.0.slow_clone_ruby_position() {
            StyloRubyPosition::AlternateOver => text::RubyPosition::AlternateOver,
            StyloRubyPosition::AlternateUnder => text::RubyPosition::AlternateUnder,
            StyloRubyPosition::Over => text::RubyPosition::Over,
            StyloRubyPosition::Under => text::RubyPosition::Under,
        }
    }

    #[inline]
    fn ruby_align(&self) -> text::RubyAlign {
        match self.0.slow_clone_ruby_align() {
            StyloRubyAlign::SpaceAround => text::RubyAlign::SpaceAround,
            StyloRubyAlign::SpaceBetween => text::RubyAlign::SpaceBetween,
            StyloRubyAlign::Center => text::RubyAlign::Center,
            StyloRubyAlign::Start => text::RubyAlign::Start,
        }
    }

    #[inline]
    fn ruby_overhang(&self) -> text::RubyOverhang {
        match self.0.slow_clone_ruby_overhang() {
            StyloRubyOverhang::Auto => text::RubyOverhang::Auto,
            StyloRubyOverhang::Spaces => text::RubyOverhang::Spaces,
        }
    }

    #[inline]
    fn text_combine_upright(&self) -> text::TextCombineUpright {
        match self.0.slow_clone_text_combine_upright() {
            StyloTextCombineUpright::None => text::TextCombineUpright::None,
            StyloTextCombineUpright::All => text::TextCombineUpright::All,
            StyloTextCombineUpright::Digits(count) => text::TextCombineUpright::Digits(count),
        }
    }

    // Inherited box

    #[inline]
    fn direction(&self) -> text::Direction {
        match self.0.get_inherited_box().direction {
            StyloDirection::Ltr => text::Direction::Ltr,
            StyloDirection::Rtl => text::Direction::Rtl,
        }
    }

    /// `is_sideways` is not the question here: it also answers yes to
    /// `text-orientation: sideways`, which turns the glyphs of a vertical block without changing
    /// which way its lines run. Only the writing mode's own sideways values reverse the lines,
    /// and that is `VERTICAL_SIDEWAYS`.
    #[inline]
    fn writing_mode(&self) -> text::WritingMode {
        let mode = self.0.writing_mode;
        if mode.intersects(StyloWritingMode::VERTICAL_SIDEWAYS) {
            if mode.is_vertical_lr() {
                text::WritingMode::SidewaysLr
            } else {
                text::WritingMode::SidewaysRl
            }
        } else if mode.is_vertical_lr() {
            text::WritingMode::VerticalLr
        } else if mode.is_vertical() {
            text::WritingMode::VerticalRl
        } else {
            text::WritingMode::HorizontalTb
        }
    }

    #[inline]
    fn text_orientation(&self) -> text::TextOrientation {
        match self.0.slow_clone_text_orientation() {
            StyloTextOrientation::Mixed => text::TextOrientation::Mixed,
            StyloTextOrientation::Upright => text::TextOrientation::Upright,
            StyloTextOrientation::Sideways => text::TextOrientation::Sideways,
        }
    }

    // Text

    #[inline]
    fn text_decoration_line(&self) -> text::TextDecorationLine {
        use text::TextDecorationLine as D;
        map_flags!(
            self.0.get_text().text_decoration_line,
            D,
            [
                (StyloTextDecorationLine::UNDERLINE, D::UNDERLINE),
                (StyloTextDecorationLine::OVERLINE, D::OVERLINE),
                (StyloTextDecorationLine::LINE_THROUGH, D::LINE_THROUGH),
                (StyloTextDecorationLine::BLINK, D::BLINK),
                (StyloTextDecorationLine::SPELLING_ERROR, D::SPELLING_ERROR),
                (StyloTextDecorationLine::GRAMMAR_ERROR, D::GRAMMAR_ERROR),
            ]
        )
    }

    #[inline]
    fn text_decoration_style(&self) -> text::TextDecorationStyle {
        match self.0.get_text().text_decoration_style {
            StyloTextDecorationStyle::Solid => text::TextDecorationStyle::Solid,
            StyloTextDecorationStyle::Double => text::TextDecorationStyle::Double,
            StyloTextDecorationStyle::Dotted => text::TextDecorationStyle::Dotted,
            StyloTextDecorationStyle::Dashed => text::TextDecorationStyle::Dashed,
            StyloTextDecorationStyle::Wavy => text::TextDecorationStyle::Wavy,
            StyloTextDecorationStyle::None => text::TextDecorationStyle::None,
        }
    }

    #[inline]
    fn text_decoration_color(&self) -> Color {
        self.0
            .get_text()
            .text_decoration_color
            .as_absolute()
            .map(|color| color.as_color_color())
            .unwrap_or_else(|| self.color())
    }

    #[inline]
    fn text_decoration_thickness(&self) -> text::TextDecorationThickness {
        match &self.0.get_text().text_decoration_thickness {
            GenericTextDecorationLength::LengthPercentage(length) => {
                text::TextDecorationThickness::Length(length_percentage(length))
            }
            GenericTextDecorationLength::FromFont => text::TextDecorationThickness::FromFont,
            GenericTextDecorationLength::Auto => text::TextDecorationThickness::Auto,
        }
    }

    #[inline]
    fn text_decoration_inset(&self) -> text::TextDecorationInset {
        match &self.0.get_text().text_decoration_inset {
            GenericTextDecorationInset::LengthPercentage { start, end } => {
                text::TextDecorationInset::Length {
                    start: length_percentage(start),
                    end: length_percentage(end),
                }
            }
            GenericTextDecorationInset::Auto => text::TextDecorationInset::Auto,
        }
    }

    #[inline]
    fn text_overflow(&self) -> text::TextOverflow {
        // Stylo keeps the end side in `second` either way, a single value being the end with
        // the start left at `clip`.
        match &self.0.get_text().text_overflow.second {
            StyloTextOverflowSide::Clip => text::TextOverflow::Clip,
            StyloTextOverflowSide::Ellipsis => text::TextOverflow::Ellipsis,
            StyloTextOverflowSide::String(_) => text::TextOverflow::String,
        }
    }

    #[inline]
    fn unicode_bidi(&self) -> text::UnicodeBidi {
        match self.0.slow_clone_unicode_bidi() {
            StyloUnicodeBidi::Normal => text::UnicodeBidi::Normal,
            StyloUnicodeBidi::Embed => text::UnicodeBidi::Embed,
            StyloUnicodeBidi::Isolate => text::UnicodeBidi::Isolate,
            StyloUnicodeBidi::BidiOverride => text::UnicodeBidi::BidiOverride,
            StyloUnicodeBidi::IsolateOverride => text::UnicodeBidi::IsolateOverride,
            StyloUnicodeBidi::Plaintext => text::UnicodeBidi::Plaintext,
        }
    }

    #[inline]
    fn text_box_trim(&self) -> text::TextBoxTrim {
        let trim = self.0.slow_clone_text_box_trim();
        match (
            trim.contains(StyloTextBoxTrim::TRIM_START),
            trim.contains(StyloTextBoxTrim::TRIM_END),
        ) {
            (true, true) => text::TextBoxTrim::TrimBoth,
            (true, false) => text::TextBoxTrim::TrimStart,
            (false, true) => text::TextBoxTrim::TrimEnd,
            (false, false) => text::TextBoxTrim::None,
        }
    }

    #[inline]
    fn text_box_edge(&self) -> text::TextBoxEdge {
        let edge = |keyword: TextEdgeKeyword| match keyword {
            TextEdgeKeyword::Text => text::TextEdge::Text,
            TextEdgeKeyword::Ideographic => text::TextEdge::Ideographic,
            TextEdgeKeyword::IdeographicInk => text::TextEdge::IdeographicInk,
            TextEdgeKeyword::Cap => text::TextEdge::Cap,
            TextEdgeKeyword::Ex => text::TextEdge::Ex,
            TextEdgeKeyword::Alphabetic => text::TextEdge::Alphabetic,
        };
        match self.0.slow_clone_text_box_edge() {
            StyloTextBoxEdge::Auto => text::TextBoxEdge::Auto,
            StyloTextBoxEdge::TextEdge(edges) => text::TextBoxEdge::Edges {
                over: edge(edges.over),
                under: edge(edges.under),
            },
        }
    }

    #[inline]
    fn initial_letter(&self) -> text::InitialLetter {
        let letter = self.0.slow_clone_initial_letter();
        text::InitialLetter {
            size: letter.size,
            sink: match letter.sink {
                GenericInitialLetterSink::Omitted | GenericInitialLetterSink::Drop => {
                    text::InitialLetterSink::Drop
                }
                GenericInitialLetterSink::Raise => text::InitialLetterSink::Raise,
                GenericInitialLetterSink::Integer(lines) => text::InitialLetterSink::Lines(lines),
            },
        }
    }

    #[inline]
    fn dominant_baseline(&self) -> text::DominantBaseline {
        match self.0.slow_clone_dominant_baseline() {
            StyloDominantBaseline::Auto => text::DominantBaseline::Auto,
            StyloDominantBaseline::TextBottom => text::DominantBaseline::TextBottom,
            StyloDominantBaseline::Alphabetic => text::DominantBaseline::Alphabetic,
            StyloDominantBaseline::Ideographic => text::DominantBaseline::Ideographic,
            StyloDominantBaseline::Middle => text::DominantBaseline::Middle,
            StyloDominantBaseline::Central => text::DominantBaseline::Central,
            StyloDominantBaseline::Mathematical => text::DominantBaseline::Mathematical,
            StyloDominantBaseline::Hanging => text::DominantBaseline::Hanging,
            StyloDominantBaseline::TextTop => text::DominantBaseline::TextTop,
        }
    }

    #[inline]
    fn line_clamp(&self) -> text::LineClamp {
        // `line-clamp`, or where it is `none`, `-webkit-line-clamp`: Stylo's servo build has no
        // `-webkit-box` to ask for.
        let clamp = self.0.slow_clone_line_clamp();
        match clamp.max_lines.lines_value() {
            Some(lines) => text::LineClamp::Lines(lines.0),
            None if clamp.max_lines.is_auto() => text::LineClamp::Auto,
            None => text::LineClamp::None,
        }
    }

    // Box

    #[inline]
    fn display(&self) -> text::Display {
        let display: StyloDisplay = self.0.get_box().display;
        text::Display {
            outside: match display.outside() {
                StyloDisplayOutside::None => text::DisplayOutside::None,
                StyloDisplayOutside::Inline => text::DisplayOutside::Inline,
                StyloDisplayOutside::Block => text::DisplayOutside::Block,
                StyloDisplayOutside::TableCaption | StyloDisplayOutside::InternalTable => {
                    text::DisplayOutside::Other
                }
            },
            inside: match display.inside() {
                StyloDisplayInside::None => text::DisplayInside::None,
                StyloDisplayInside::Contents => text::DisplayInside::Contents,
                StyloDisplayInside::Flow => text::DisplayInside::Flow,
                StyloDisplayInside::FlowRoot => text::DisplayInside::FlowRoot,
                StyloDisplayInside::Flex
                | StyloDisplayInside::Grid
                | StyloDisplayInside::GridLanes
                | StyloDisplayInside::Table
                | StyloDisplayInside::TableRowGroup
                | StyloDisplayInside::TableColumn
                | StyloDisplayInside::TableColumnGroup
                | StyloDisplayInside::TableHeaderGroup
                | StyloDisplayInside::TableFooterGroup
                | StyloDisplayInside::TableRow
                | StyloDisplayInside::TableCell => text::DisplayInside::Other,
            },
        }
    }

    #[inline]
    fn float(&self) -> text::Float {
        match self.0.slow_clone_float() {
            StyloFloat::Left => text::Float::Left,
            StyloFloat::Right => text::Float::Right,
            StyloFloat::None => text::Float::None,
            StyloFloat::InlineStart => text::Float::InlineStart,
            StyloFloat::InlineEnd => text::Float::InlineEnd,
        }
    }

    #[inline]
    fn position(&self) -> text::Position {
        match self.0.slow_clone_position() {
            StyloPosition::Static => text::Position::Static,
            StyloPosition::Relative => text::Position::Relative,
            StyloPosition::Absolute => text::Position::Absolute,
            StyloPosition::Fixed => text::Position::Fixed,
            StyloPosition::Sticky => text::Position::Sticky,
        }
    }

    #[inline]
    fn vertical_align(&self) -> text::VerticalAlign {
        match self.0.get_box().slow_clone__servo_vertical_align() {
            GenericVerticalAlign::Keyword(keyword) => match keyword {
                VerticalAlignKeyword::Baseline => text::VerticalAlign::Baseline,
                VerticalAlignKeyword::Sub => text::VerticalAlign::Sub,
                VerticalAlignKeyword::Super => text::VerticalAlign::Super,
                VerticalAlignKeyword::Top => text::VerticalAlign::Top,
                VerticalAlignKeyword::TextTop => text::VerticalAlign::TextTop,
                VerticalAlignKeyword::Middle => text::VerticalAlign::Middle,
                VerticalAlignKeyword::Bottom => text::VerticalAlign::Bottom,
                VerticalAlignKeyword::TextBottom => text::VerticalAlign::TextBottom,
            },
            GenericVerticalAlign::Length(length) => {
                text::VerticalAlign::Length(length_percentage(&length))
            }
        }
    }

    #[inline]
    fn overflow_x(&self) -> text::Overflow {
        match self.0.slow_clone_overflow_x() {
            StyloOverflow::Visible => text::Overflow::Visible,
            StyloOverflow::Hidden => text::Overflow::Hidden,
            StyloOverflow::Scroll => text::Overflow::Scroll,
            StyloOverflow::Auto => text::Overflow::Auto,
            StyloOverflow::Clip => text::Overflow::Clip,
        }
    }

    // Margin, padding and border

    #[inline]
    fn margin(&self) -> text::Sides<text::LengthPercentageOrAuto> {
        let margin = self.0.get_margin();
        let side = |value: &GenericMargin<StyloLengthPercentage>| match value {
            GenericMargin::LengthPercentage(length) => {
                text::LengthPercentageOrAuto::LengthPercentage(length_percentage(length))
            }
            // An anchor function resolves only against an anchor, which text has none of.
            GenericMargin::Auto
            | GenericMargin::AnchorSizeFunction(_)
            | GenericMargin::AnchorContainingCalcFunction(_) => text::LengthPercentageOrAuto::Auto,
        };
        text::Sides {
            top: side(&margin.margin_top),
            right: side(&margin.margin_right),
            bottom: side(&margin.margin_bottom),
            left: side(&margin.margin_left),
        }
    }

    #[inline]
    fn padding(&self) -> text::Sides<text::LengthPercentage> {
        let padding = self.0.get_padding();
        text::Sides {
            top: length_percentage(&padding.padding_top.0),
            right: length_percentage(&padding.padding_right.0),
            bottom: length_percentage(&padding.padding_bottom.0),
            left: length_percentage(&padding.padding_left.0),
        }
    }

    #[inline]
    fn border_widths(&self) -> text::Sides<f32> {
        let border = self.0.get_border();
        text::Sides {
            top: border.border_top_width.0.to_f32_px(),
            right: border.border_right_width.0.to_f32_px(),
            bottom: border.border_bottom_width.0.to_f32_px(),
            left: border.border_left_width.0.to_f32_px(),
        }
    }

    #[inline]
    fn border_styles(&self) -> text::Sides<text::BorderStyle> {
        let border = self.0.get_border();
        text::Sides {
            top: border_style(border.border_top_style),
            right: border_style(border.border_right_style),
            bottom: border_style(border.border_bottom_style),
            left: border_style(border.border_left_style),
        }
    }

    #[inline]
    fn border_colors(&self) -> text::Sides<Color> {
        let border = self.0.get_border();
        let current = self.0.slow_clone_color();
        text::Sides {
            top: border
                .border_top_color
                .resolve_to_absolute(&current)
                .as_color_color(),
            right: border
                .border_right_color
                .resolve_to_absolute(&current)
                .as_color_color(),
            bottom: border
                .border_bottom_color
                .resolve_to_absolute(&current)
                .as_color_color(),
            left: border
                .border_left_color
                .resolve_to_absolute(&current)
                .as_color_color(),
        }
    }

    #[inline]
    fn border_radii(&self) -> text::Corners<text::CornerRadius> {
        let border = self.0.get_border();
        let radius = |corner: &style::values::computed::BorderCornerRadius| text::CornerRadius {
            width: length_percentage(&corner.0.width.0),
            height: length_percentage(&corner.0.height.0),
        };
        text::Corners {
            top_left: radius(&border.border_top_left_radius),
            top_right: radius(&border.border_top_right_radius),
            bottom_right: radius(&border.border_bottom_right_radius),
            bottom_left: radius(&border.border_bottom_left_radius),
        }
    }

    #[inline]
    fn box_decoration_break(&self) -> text::BoxDecorationBreak {
        match self.0.slow_clone_box_decoration_break() {
            StyloBoxDecorationBreak::Slice => text::BoxDecorationBreak::Slice,
            StyloBoxDecorationBreak::Clone => text::BoxDecorationBreak::Clone,
        }
    }

    // Outline

    #[inline]
    fn outline_style(&self) -> text::OutlineStyle {
        match self.0.get_outline().outline_style {
            StyloOutlineStyle::Auto => text::OutlineStyle::Auto,
            StyloOutlineStyle::BorderStyle(style) => {
                text::OutlineStyle::Border(border_style(style))
            }
        }
    }

    #[inline]
    fn outline_width(&self) -> f32 {
        self.0.get_outline().outline_width.0.to_f32_px()
    }

    #[inline]
    fn outline_color(&self) -> Color {
        let current = self.0.slow_clone_color();
        self.0
            .get_outline()
            .outline_color
            .resolve_to_absolute(&current)
            .as_color_color()
    }

    // Background

    #[inline]
    fn background_color(&self) -> Color {
        let current = self.0.slow_clone_color();
        self.0
            .get_background()
            .background_color
            .resolve_to_absolute(&current)
            .as_color_color()
    }

    #[inline]
    fn has_background_image(&self) -> bool {
        self.0
            .get_background()
            .background_image
            .0
            .iter()
            .any(|image| !matches!(image, style::values::computed::Image::None))
    }

    #[inline]
    fn has_box_shadow(&self) -> bool {
        !self.0.get_effects().box_shadow.0.is_empty()
    }

    // Group comparisons: a style struct that two styles share is the same.

    #[inline]
    fn same_font(&self, other: &Self) -> bool {
        let (a, b) = (self.0.get_font(), other.0.get_font());
        std::ptr::eq(a, b) || a == b
    }

    #[inline]
    fn same_block_text(&self, other: &Self) -> bool {
        let (a, b) = (self.0.get_inherited_text(), other.0.get_inherited_text());
        std::ptr::eq(a, b)
            || (a.text_align == b.text_align
                && a.text_align_last == b.text_align_last
                && a.text_indent == b.text_indent
                && a.text_wrap_style == b.text_wrap_style
                && a.text_fit == b.text_fit)
    }

    #[inline]
    fn same_block_text_but_alignment(&self, other: &Self) -> bool {
        let (a, b) = (self.0.get_inherited_text(), other.0.get_inherited_text());
        std::ptr::eq(a, b)
            || (a.text_indent == b.text_indent
                && a.text_wrap_style == b.text_wrap_style
                && a.text_fit == b.text_fit)
    }

    #[inline]
    fn same_inline_text(&self, other: &Self) -> bool {
        let (old, new) = (self.0.get_inherited_text(), other.0.get_inherited_text());
        if std::ptr::eq(old, new) || old == new {
            return true;
        }
        let mut masked = old.clone();
        masked.text_align = new.text_align;
        masked.text_align_last = new.text_align_last;
        masked.text_indent = new.text_indent.clone();
        masked.text_wrap_style = new.text_wrap_style;
        masked.text_fit = new.text_fit;
        masked == *new
    }

    #[inline]
    fn same_inherited_box(&self, other: &Self) -> bool {
        let (a, b) = (self.0.get_inherited_box(), other.0.get_inherited_box());
        std::ptr::eq(a, b) || a == b
    }

    #[inline]
    fn same_text(&self, other: &Self) -> bool {
        let (a, b) = (self.0.get_text(), other.0.get_text());
        std::ptr::eq(a, b) || a == b
    }

    #[inline]
    fn same_box(&self, other: &Self) -> bool {
        let (a, b) = (self.0.get_box(), other.0.get_box());
        std::ptr::eq(a, b) || a == b
    }

    #[inline]
    fn same_margin(&self, other: &Self) -> bool {
        let (a, b) = (self.0.get_margin(), other.0.get_margin());
        std::ptr::eq(a, b) || a == b
    }

    #[inline]
    fn same_padding(&self, other: &Self) -> bool {
        let (a, b) = (self.0.get_padding(), other.0.get_padding());
        std::ptr::eq(a, b) || a == b
    }

    #[inline]
    fn same_border(&self, other: &Self) -> bool {
        let (a, b) = (self.0.get_border(), other.0.get_border());
        std::ptr::eq(a, b) || a == b
    }
}

#[inline]
fn synthesis(value: StyloFontSynthesis) -> text::FontSynthesis {
    match value {
        StyloFontSynthesis::Auto => text::FontSynthesis::Auto,
        StyloFontSynthesis::None => text::FontSynthesis::None,
    }
}

#[cfg(test)]
mod tests {
    use blitz_text::OwnedTextStyle;
    use blitz_text::style::{self as text, TextStyle};
    use style::ArcSlice;
    use style::properties::ComputedValues;
    use style::properties::style_structs::Font;
    use style::values::computed::font::{FontFamilyList, SingleFontFamily};

    use super::{StyloFontFamily, StyloGenericFontFamily, StyloStyle, is_initial_family};

    /// Stylo's initial style reads as blitz-text's initial values, but where the two differ on
    /// purpose: Stylo's servo build computes `ruby-position` as Chrome's `over`.
    #[test]
    fn the_initial_style_reads_as_initial_values() {
        let computed = ComputedValues::initial_values_with_font_override(Font::initial_values());
        let style = StyloStyle(computed.clone());
        let owned = OwnedTextStyle::default();
        assert!(style.font_family_is_initial());
        assert_eq!(style.font_weight(), owned.font_weight());
        assert_eq!(style.font_style(), owned.font_style());
        assert_eq!(style.line_height(), owned.line_height());
        assert_eq!(style.text_align(), owned.text_align());
        assert_eq!(style.text_align_last(), owned.text_align_last());
        assert_eq!(style.text_indent(), owned.text_indent());
        assert_eq!(style.white_space_collapse(), owned.white_space_collapse());
        assert_eq!(style.word_break(), owned.word_break());
        assert_eq!(style.letter_spacing(), owned.letter_spacing());
        assert_eq!(style.text_decoration_line(), owned.text_decoration_line());
        assert_eq!(style.text_decoration_style(), owned.text_decoration_style());
        assert_eq!(style.direction(), owned.direction());
        assert_eq!(style.writing_mode(), owned.writing_mode());
        assert_eq!(style.unicode_bidi(), owned.unicode_bidi());
        assert_eq!(style.display(), owned.display());
        assert_eq!(style.float(), owned.float());
        assert_eq!(style.position(), owned.position());
        assert_eq!(style.vertical_align(), owned.vertical_align());
        assert_eq!(style.padding(), owned.padding());
        assert_eq!(style.border_widths(), owned.border_widths());
        assert_eq!(style.border_styles(), owned.border_styles());
        assert_eq!(style.outline_width(), owned.outline_width());
        assert_eq!(style.outline_style(), owned.outline_style());
        assert_eq!(style.background_color(), owned.background_color());
        assert_eq!(style.has_box_shadow(), owned.has_box_shadow());
        assert_eq!(style.color(), owned.color());
        assert_eq!(style.text_shadow_count(), 0);
        assert_eq!(style.font_variant_alternates().count(), 0);
        assert_eq!(style.ruby_position(), text::RubyPosition::Over);
        // A style compares the same as itself, through the shared structs.
        let again = StyloStyle(computed);
        assert!(style.same_font(&again) && style.same_inline_text(&again));
        assert!(style.same_block_text(&again) && style.same_box(&again));
    }

    #[test]
    fn only_the_initial_family_is_the_initial_family() {
        assert!(is_initial_family(&StyloFontFamily::serif()));
        // A style sheet's `serif`, in a list of its own as Stylo parses it.
        let named = StyloFontFamily {
            families: FontFamilyList {
                list: ArcSlice::from_iter(std::iter::once(SingleFontFamily::Generic(
                    StyloGenericFontFamily::Serif,
                ))),
            },
            is_system_font: false,
            is_initial: false,
        };
        assert!(!is_initial_family(&named));
    }
}

#[cfg(test)]
mod calc_tests {
    use blitz_text::style as text;
    use style::typed_om::NumericBaseType;
    use style::values::computed::calc::CalcPercentageLeaf;
    use style::values::computed::length_percentage::{CalcNode, ComputedLeaf};
    use style::values::computed::{Length, LengthPercentage};
    use style::values::generics::Optional;
    use style::values::generics::calc::MinMaxOp;
    use style_traits::values::specified::AllowedNumericType;

    use super::{is_linear, length_percentage, of_font_size};

    fn px(px: f32) -> CalcNode {
        CalcNode::Leaf(ComputedLeaf::Length(Length::new(px)))
    }

    fn percent(fraction: f32) -> CalcNode {
        CalcNode::Leaf(ComputedLeaf::Percentage(CalcPercentageLeaf::new(
            fraction,
            Optional::Some(NumericBaseType::Length),
        )))
    }

    fn calc(node: CalcNode, clamping: AllowedNumericType) -> LengthPercentage {
        LengthPercentage::new_calc(node, clamping)
    }

    fn max() -> LengthPercentage {
        calc(
            CalcNode::MinMax(vec![px(20.0), percent(0.1)].into(), MinMaxOp::Max),
            AllowedNumericType::All,
        )
    }

    /// Sums, negations and constant multiples of lengths and percentages are linear in the
    /// basis; `max()`, `abs()` and a sum its property clamps at zero are not.
    #[test]
    fn only_sums_and_scaled_percentages_are_linear() {
        let all = AllowedNumericType::All;
        let sum = CalcNode::Sum(vec![px(10.0), CalcNode::Negate(Box::new(percent(0.5)))].into());
        assert!(is_linear(&calc(sum.clone(), all)));
        let scaled = CalcNode::Sum(
            vec![
                px(1.0),
                CalcNode::Product(
                    vec![percent(0.5), CalcNode::Leaf(ComputedLeaf::Number(2.0))].into(),
                ),
            ]
            .into(),
        );
        assert!(is_linear(&calc(scaled, all)));
        assert!(!is_linear(&max()));
        let abs = CalcNode::Abs(Box::new(sum.clone()));
        assert!(!is_linear(&calc(abs, all)));
        assert!(!is_linear(&calc(sum, AllowedNumericType::NonNegative)));
    }

    /// A nonlinear `calc()` stays a function of its basis, `max(20px, 10%)` resolving to 20 at
    /// 100 and 40 at 400; against the font size it is resolved at once.
    #[test]
    fn a_nonlinear_calc_resolves_against_each_basis() {
        let value = length_percentage(&max());
        assert!(matches!(value, text::LengthPercentage::Nonlinear(_)));
        assert_eq!(value.resolve(100.0), 20.0);
        assert_eq!(value.resolve(400.0), 40.0);
        assert_eq!(
            of_font_size(&max(), 300.0),
            text::LengthPercentage::Length(30.0)
        );
    }
}
