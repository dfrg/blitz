//! A computed style held as plain values.

use std::sync::Arc;

use super::*;

/// A function or keyword of `font-variant-alternates`, holding its name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum OwnedVariantAlternate {
    Stylistic(String),
    Swash(String),
    Ornaments(String),
    Annotation(String),
    Styleset(String),
    CharacterVariant(String),
    HistoricalForms,
}

impl OwnedVariantAlternate {
    /// Returns it borrowed, as [`TextStyle::font_variant_alternates`] answers.
    pub fn as_alternate(&self) -> VariantAlternate<'_> {
        match self {
            Self::Stylistic(name) => VariantAlternate::Stylistic(name),
            Self::Swash(name) => VariantAlternate::Swash(name),
            Self::Ornaments(name) => VariantAlternate::Ornaments(name),
            Self::Annotation(name) => VariantAlternate::Annotation(name),
            Self::Styleset(name) => VariantAlternate::Styleset(name),
            Self::CharacterVariant(name) => VariantAlternate::CharacterVariant(name),
            Self::HistoricalForms => VariantAlternate::HistoricalForms,
        }
    }
}

/// A computed `text-emphasis-style`, holding its string.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum OwnedEmphasisStyle {
    #[default]
    None,
    Keyword {
        fill: EmphasisFill,
        shape: EmphasisShape,
    },
    String(String),
}

/// A computed style held as plain values: every property [`TextStyle`] reads, at its initial
/// value by default.
///
/// For tests, and for hosts without a style engine of their own. Its lists are shared between
/// clones.
#[derive(Clone, Debug, PartialEq)]
pub struct OwnedTextStyle {
    // Font
    pub font_family: Arc<[FontFamilyName<'static>]>,
    /// Whether `font_family` is the initial value, which names no family a style sheet chose.
    pub font_family_is_initial: bool,
    pub font_size: f32,
    pub font_weight: FontWeight,
    pub font_width: FontWidth,
    pub font_style: FontStyle,
    pub font_synthesis_weight: FontSynthesis,
    pub font_synthesis_style: FontSynthesisStyle,
    pub font_synthesis_small_caps: FontSynthesis,
    pub font_synthesis_position: FontSynthesis,
    pub font_variant_caps: FontVariantCaps,
    pub font_variant_position: FontVariantPosition,
    pub font_variant_ligatures: FontVariantLigatures,
    pub font_variant_numeric: FontVariantNumeric,
    pub font_variant_east_asian: FontVariantEastAsian,
    pub font_variant_alternates: Arc<[OwnedVariantAlternate]>,
    pub font_variant_emoji: FontVariantEmoji,
    pub font_feature_settings: Arc<[FontFeature]>,
    pub font_variation_settings: Arc<[FontVariation]>,
    pub font_optical_sizing: FontOpticalSizing,
    pub font_kerning: FontKerning,
    pub font_language_override: Option<Tag>,
    pub font_size_adjust: FontSizeAdjust,
    pub line_height: LineHeight,
    pub language: Arc<str>,
    // Inherited text
    pub color: Color,
    pub text_align: TextAlign,
    pub text_align_last: TextAlignLast,
    pub text_indent: TextIndent,
    pub text_justify: TextJustify,
    pub text_transform: TextTransform,
    pub white_space_collapse: WhiteSpaceCollapse,
    pub text_wrap_mode: TextWrapMode,
    pub text_wrap_style: TextWrapStyle,
    pub word_break: WordBreak,
    pub line_break: LineBreak,
    pub overflow_wrap: OverflowWrap,
    pub hyphens: Hyphens,
    pub hyphenate_character: Option<Arc<str>>,
    pub tab_size: TabSize,
    pub letter_spacing: LengthPercentage,
    pub word_spacing: LengthPercentage,
    pub text_autospace: TextAutospace,
    pub text_spacing_trim: TextSpacingTrim,
    pub hanging_punctuation: HangingPunctuation,
    pub text_emphasis_style: OwnedEmphasisStyle,
    pub text_emphasis_position: TextEmphasisPosition,
    pub text_emphasis_color: Color,
    pub text_security: TextSecurity,
    pub text_fit: TextFit,
    pub text_underline_offset: Option<LengthPercentage>,
    pub text_underline_position: TextUnderlinePosition,
    pub text_decoration_skip_spaces: TextDecorationSkipSpaces,
    pub text_shadow: Arc<[TextShadow]>,
    pub ruby_position: RubyPosition,
    pub ruby_align: RubyAlign,
    pub ruby_overhang: RubyOverhang,
    pub text_combine_upright: TextCombineUpright,
    // Inherited box
    pub direction: Direction,
    pub writing_mode: WritingMode,
    pub text_orientation: TextOrientation,
    // Text
    pub text_decoration_line: TextDecorationLine,
    pub text_decoration_style: TextDecorationStyle,
    pub text_decoration_color: Color,
    pub text_decoration_thickness: TextDecorationThickness,
    pub text_decoration_inset: TextDecorationInset,
    pub text_overflow: TextOverflow,
    pub unicode_bidi: UnicodeBidi,
    pub text_box_trim: TextBoxTrim,
    pub text_box_edge: TextBoxEdge,
    pub initial_letter: InitialLetter,
    pub dominant_baseline: DominantBaseline,
    pub line_clamp: LineClamp,
    // Box
    pub display: Display,
    pub float: Float,
    pub position: Position,
    pub alignment_baseline: AlignmentBaseline,
    pub baseline_shift: BaselineShift,
    pub overflow_x: Overflow,
    // Margin, padding and border
    pub margin: Sides<LengthPercentageOrAuto>,
    pub padding: Sides<LengthPercentage>,
    pub border_widths: Sides<f32>,
    pub border_styles: Sides<BorderStyle>,
    pub border_colors: Sides<Color>,
    pub border_radii: Corners<CornerRadius>,
    pub box_decoration_break: BoxDecorationBreak,
    // Outline
    pub outline_style: OutlineStyle,
    pub outline_width: f32,
    pub outline_color: Color,
    // Background
    pub background_color: Color,
    pub has_background_image: bool,
    // Box shadow
    pub has_box_shadow: bool,
}

impl Default for OwnedTextStyle {
    fn default() -> Self {
        let black = Color::new([0.0, 0.0, 0.0, 1.0]);
        Self {
            font_family: Arc::new([FontFamilyName::Generic(GenericFamily::Serif)]),
            font_family_is_initial: true,
            font_size: 16.0,
            font_weight: FontWeight::NORMAL,
            font_width: FontWidth::NORMAL,
            font_style: FontStyle::Normal,
            font_synthesis_weight: FontSynthesis::Auto,
            font_synthesis_style: FontSynthesisStyle::Auto,
            font_synthesis_small_caps: FontSynthesis::Auto,
            font_synthesis_position: FontSynthesis::Auto,
            font_variant_caps: FontVariantCaps::Normal,
            font_variant_position: FontVariantPosition::Normal,
            font_variant_ligatures: FontVariantLigatures::empty(),
            font_variant_numeric: FontVariantNumeric::empty(),
            font_variant_east_asian: FontVariantEastAsian::empty(),
            font_variant_alternates: Arc::new([]),
            font_variant_emoji: FontVariantEmoji::Normal,
            font_feature_settings: Arc::new([]),
            font_variation_settings: Arc::new([]),
            font_optical_sizing: FontOpticalSizing::Auto,
            font_kerning: FontKerning::Auto,
            font_language_override: None,
            font_size_adjust: FontSizeAdjust::None,
            line_height: LineHeight::Normal,
            language: Arc::from(""),
            color: black,
            text_align: TextAlign::Start,
            text_align_last: TextAlignLast::Auto,
            text_indent: TextIndent::default(),
            text_justify: TextJustify::Auto,
            text_transform: TextTransform::empty(),
            white_space_collapse: WhiteSpaceCollapse::Collapse,
            text_wrap_mode: TextWrapMode::Wrap,
            text_wrap_style: TextWrapStyle::Auto,
            word_break: WordBreak::Normal,
            line_break: LineBreak::Auto,
            overflow_wrap: OverflowWrap::Normal,
            hyphens: Hyphens::Manual,
            hyphenate_character: None,
            tab_size: TabSize::Spaces(8.0),
            letter_spacing: LengthPercentage::Length(0.0),
            word_spacing: LengthPercentage::Length(0.0),
            text_autospace: TextAutospace::NORMAL,
            text_spacing_trim: TextSpacingTrim::Normal,
            hanging_punctuation: HangingPunctuation::empty(),
            text_emphasis_style: OwnedEmphasisStyle::None,
            text_emphasis_position: TextEmphasisPosition::AUTO,
            text_emphasis_color: black,
            text_security: TextSecurity::None,
            text_fit: TextFit::default(),
            text_underline_offset: None,
            text_underline_position: TextUnderlinePosition::empty(),
            text_decoration_skip_spaces: TextDecorationSkipSpaces::StartEnd,
            text_shadow: Arc::new([]),
            ruby_position: RubyPosition::Over,
            ruby_align: RubyAlign::SpaceAround,
            ruby_overhang: RubyOverhang::Auto,
            text_combine_upright: TextCombineUpright::None,
            direction: Direction::Ltr,
            writing_mode: WritingMode::HorizontalTb,
            text_orientation: TextOrientation::Mixed,
            text_decoration_line: TextDecorationLine::empty(),
            text_decoration_style: TextDecorationStyle::Solid,
            text_decoration_color: black,
            text_decoration_thickness: TextDecorationThickness::Auto,
            text_decoration_inset: TextDecorationInset::Auto,
            text_overflow: TextOverflow::Clip,
            unicode_bidi: UnicodeBidi::Normal,
            text_box_trim: TextBoxTrim::None,
            text_box_edge: TextBoxEdge::Auto,
            initial_letter: InitialLetter::default(),
            dominant_baseline: DominantBaseline::Auto,
            line_clamp: LineClamp::None,
            display: Display {
                outside: DisplayOutside::Inline,
                inside: DisplayInside::Flow,
            },
            float: Float::None,
            position: Position::Static,
            alignment_baseline: AlignmentBaseline::Baseline,
            baseline_shift: BaselineShift::default(),
            overflow_x: Overflow::Visible,
            margin: Sides::all(LengthPercentageOrAuto::LengthPercentage(
                LengthPercentage::Length(0.0),
            )),
            padding: Sides::all(LengthPercentage::Length(0.0)),
            // `medium`, which a style of `none` draws none of.
            border_widths: Sides::all(3.0),
            border_styles: Sides::all(BorderStyle::None),
            border_colors: Sides::all(black),
            border_radii: Corners::default(),
            box_decoration_break: BoxDecorationBreak::Slice,
            outline_style: OutlineStyle::default(),
            outline_width: 3.0,
            outline_color: black,
            background_color: Color::new([0.0, 0.0, 0.0, 0.0]),
            has_background_image: false,
            has_box_shadow: false,
        }
    }
}

impl OwnedTextStyle {
    /// Returns the initial style with `font-family` set to `families`.
    pub fn with_families(families: impl IntoIterator<Item = FontFamilyName<'static>>) -> Self {
        Self {
            font_family: families.into_iter().collect(),
            font_family_is_initial: false,
            ..Self::default()
        }
    }
}

impl TextStyle for OwnedTextStyle {
    fn font_family(&self) -> impl Iterator<Item = FontFamilyName<'_>> + Clone {
        self.font_family.iter().map(|family| match family {
            FontFamilyName::Named(name) => FontFamilyName::Named(std::borrow::Cow::Borrowed(name)),
            FontFamilyName::Generic(generic) => FontFamilyName::Generic(*generic),
        })
    }
    fn font_family_is_initial(&self) -> bool {
        self.font_family_is_initial
    }
    fn font_size(&self) -> f32 {
        self.font_size
    }
    fn font_weight(&self) -> FontWeight {
        self.font_weight
    }
    fn font_width(&self) -> FontWidth {
        self.font_width
    }
    fn font_style(&self) -> FontStyle {
        self.font_style
    }
    fn font_synthesis_weight(&self) -> FontSynthesis {
        self.font_synthesis_weight
    }
    fn font_synthesis_style(&self) -> FontSynthesisStyle {
        self.font_synthesis_style
    }
    fn font_synthesis_small_caps(&self) -> FontSynthesis {
        self.font_synthesis_small_caps
    }
    fn font_synthesis_position(&self) -> FontSynthesis {
        self.font_synthesis_position
    }
    fn font_variant_caps(&self) -> FontVariantCaps {
        self.font_variant_caps
    }
    fn font_variant_position(&self) -> FontVariantPosition {
        self.font_variant_position
    }
    fn font_variant_ligatures(&self) -> FontVariantLigatures {
        self.font_variant_ligatures
    }
    fn font_variant_numeric(&self) -> FontVariantNumeric {
        self.font_variant_numeric
    }
    fn font_variant_east_asian(&self) -> FontVariantEastAsian {
        self.font_variant_east_asian
    }
    fn font_variant_alternates(&self) -> impl Iterator<Item = VariantAlternate<'_>> {
        self.font_variant_alternates
            .iter()
            .map(OwnedVariantAlternate::as_alternate)
    }
    fn font_variant_emoji(&self) -> FontVariantEmoji {
        self.font_variant_emoji
    }
    fn font_feature_settings(&self) -> impl Iterator<Item = FontFeature> + Clone {
        self.font_feature_settings.iter().copied()
    }
    fn font_variation_settings(&self) -> impl Iterator<Item = FontVariation> + Clone {
        self.font_variation_settings.iter().copied()
    }
    fn font_optical_sizing(&self) -> FontOpticalSizing {
        self.font_optical_sizing
    }
    fn font_kerning(&self) -> FontKerning {
        self.font_kerning
    }
    fn font_language_override(&self) -> Option<Tag> {
        self.font_language_override
    }
    fn font_size_adjust(&self) -> FontSizeAdjust {
        self.font_size_adjust
    }
    fn line_height(&self) -> LineHeight {
        self.line_height
    }
    fn language(&self) -> &str {
        &self.language
    }
    fn color(&self) -> Color {
        self.color
    }
    fn text_align(&self) -> TextAlign {
        self.text_align
    }
    fn text_align_last(&self) -> TextAlignLast {
        self.text_align_last
    }
    fn text_indent(&self) -> TextIndent {
        self.text_indent.clone()
    }
    fn text_justify(&self) -> TextJustify {
        self.text_justify
    }
    fn text_transform(&self) -> TextTransform {
        self.text_transform
    }
    fn white_space_collapse(&self) -> WhiteSpaceCollapse {
        self.white_space_collapse
    }
    fn text_wrap_mode(&self) -> TextWrapMode {
        self.text_wrap_mode
    }
    fn text_wrap_style(&self) -> TextWrapStyle {
        self.text_wrap_style
    }
    fn word_break(&self) -> WordBreak {
        self.word_break
    }
    fn line_break(&self) -> LineBreak {
        self.line_break
    }
    fn overflow_wrap(&self) -> OverflowWrap {
        self.overflow_wrap
    }
    fn hyphens(&self) -> Hyphens {
        self.hyphens
    }
    fn hyphenate_character(&self) -> Option<&str> {
        self.hyphenate_character.as_deref()
    }
    fn tab_size(&self) -> TabSize {
        self.tab_size
    }
    fn letter_spacing(&self) -> LengthPercentage {
        self.letter_spacing.clone()
    }
    fn word_spacing(&self) -> LengthPercentage {
        self.word_spacing.clone()
    }
    fn text_autospace(&self) -> TextAutospace {
        self.text_autospace
    }
    fn text_spacing_trim(&self) -> TextSpacingTrim {
        self.text_spacing_trim
    }
    fn hanging_punctuation(&self) -> HangingPunctuation {
        self.hanging_punctuation
    }
    fn text_emphasis_style(&self) -> TextEmphasisStyle<'_> {
        match &self.text_emphasis_style {
            OwnedEmphasisStyle::None => TextEmphasisStyle::None,
            OwnedEmphasisStyle::Keyword { fill, shape } => TextEmphasisStyle::Keyword {
                fill: *fill,
                shape: *shape,
            },
            OwnedEmphasisStyle::String(string) => TextEmphasisStyle::String(string),
        }
    }
    fn text_emphasis_position(&self) -> TextEmphasisPosition {
        self.text_emphasis_position
    }
    fn text_emphasis_color(&self) -> Color {
        self.text_emphasis_color
    }
    fn text_security(&self) -> TextSecurity {
        self.text_security
    }
    fn text_fit(&self) -> TextFit {
        self.text_fit
    }
    fn text_underline_offset(&self) -> Option<LengthPercentage> {
        self.text_underline_offset.clone()
    }
    fn text_underline_position(&self) -> TextUnderlinePosition {
        self.text_underline_position
    }
    fn text_decoration_skip_spaces(&self) -> TextDecorationSkipSpaces {
        self.text_decoration_skip_spaces
    }
    fn text_shadow(&self) -> impl Iterator<Item = TextShadow> + Clone {
        self.text_shadow.iter().copied()
    }
    fn text_shadow_count(&self) -> usize {
        self.text_shadow.len()
    }
    fn ruby_position(&self) -> RubyPosition {
        self.ruby_position
    }
    fn ruby_align(&self) -> RubyAlign {
        self.ruby_align
    }
    fn ruby_overhang(&self) -> RubyOverhang {
        self.ruby_overhang
    }
    fn text_combine_upright(&self) -> TextCombineUpright {
        self.text_combine_upright
    }
    fn direction(&self) -> Direction {
        self.direction
    }
    fn writing_mode(&self) -> WritingMode {
        self.writing_mode
    }
    fn text_orientation(&self) -> TextOrientation {
        self.text_orientation
    }
    fn text_decoration_line(&self) -> TextDecorationLine {
        self.text_decoration_line
    }
    fn text_decoration_style(&self) -> TextDecorationStyle {
        self.text_decoration_style
    }
    fn text_decoration_color(&self) -> Color {
        self.text_decoration_color
    }
    fn text_decoration_thickness(&self) -> TextDecorationThickness {
        self.text_decoration_thickness.clone()
    }
    fn text_decoration_inset(&self) -> TextDecorationInset {
        self.text_decoration_inset.clone()
    }
    fn text_overflow(&self) -> TextOverflow {
        self.text_overflow
    }
    fn unicode_bidi(&self) -> UnicodeBidi {
        self.unicode_bidi
    }
    fn text_box_trim(&self) -> TextBoxTrim {
        self.text_box_trim
    }
    fn text_box_edge(&self) -> TextBoxEdge {
        self.text_box_edge
    }
    fn initial_letter(&self) -> InitialLetter {
        self.initial_letter
    }
    fn dominant_baseline(&self) -> DominantBaseline {
        self.dominant_baseline
    }
    fn line_clamp(&self) -> LineClamp {
        self.line_clamp
    }
    fn display(&self) -> Display {
        self.display
    }
    fn float(&self) -> Float {
        self.float
    }
    fn position(&self) -> Position {
        self.position
    }
    fn alignment_baseline(&self) -> AlignmentBaseline {
        self.alignment_baseline
    }
    fn baseline_shift(&self) -> BaselineShift {
        self.baseline_shift.clone()
    }
    fn overflow_x(&self) -> Overflow {
        self.overflow_x
    }
    fn margin(&self) -> Sides<LengthPercentageOrAuto> {
        self.margin.clone()
    }
    fn padding(&self) -> Sides<LengthPercentage> {
        self.padding.clone()
    }
    fn border_widths(&self) -> Sides<f32> {
        self.border_widths
    }
    fn border_styles(&self) -> Sides<BorderStyle> {
        self.border_styles
    }
    fn border_colors(&self) -> Sides<Color> {
        self.border_colors
    }
    fn border_radii(&self) -> Corners<CornerRadius> {
        self.border_radii.clone()
    }
    fn box_decoration_break(&self) -> BoxDecorationBreak {
        self.box_decoration_break
    }
    fn outline_style(&self) -> OutlineStyle {
        self.outline_style
    }
    fn outline_width(&self) -> f32 {
        self.outline_width
    }
    fn outline_color(&self) -> Color {
        self.outline_color
    }
    fn background_color(&self) -> Color {
        self.background_color
    }
    fn has_background_image(&self) -> bool {
        self.has_background_image
    }

    fn has_box_shadow(&self) -> bool {
        self.has_box_shadow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_comparisons_compare_values() {
        let style = OwnedTextStyle::default();
        let mut other = style.clone();
        assert!(style.same_font(&other) && style.same_inline_text(&other));
        other.text_align = TextAlign::Center;
        assert!(!style.same_block_text(&other));
        assert!(style.same_block_text_but_alignment(&other));
        assert!(style.same_inline_text(&other));
        other.letter_spacing = LengthPercentage::Length(2.0);
        assert!(!style.same_inline_text(&other));
        let named = OwnedTextStyle::with_families([FontFamilyName::named("Ahem")]);
        assert!(!style.same_font(&named));
    }
}
