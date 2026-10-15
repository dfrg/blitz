//! The computed style text layout and painting read: [`TextStyle`].

mod owned;
mod values;

pub use owned::OwnedTextStyle;
pub use values::*;

pub use parlance::{
    FontFamilyName, FontFeature, FontStyle, FontVariation, FontWeight, FontWidth, GenericFamily,
    Tag,
};

/// An element's computed style, as text layout and painting read it.
///
/// Each accessor answers one property, grouped as Stylo groups them into style structs: font,
/// inherited text, inherited box, text, box, margin, padding, border, outline and background.
/// Lists borrow from the style. Lengths are in CSS pixels.
///
/// The `same_*` methods compare a group of properties between two styles. Each defaults to
/// comparing the group's values; an implementation that shares groups between styles answers
/// faster by comparing the shared groups first. They may answer `false` for styles whose values
/// are equal, never `true` for styles whose values differ.
///
/// Backends are generic over the style, and keep a clone where they read it after the call
/// they were handed it in, so a clone is meant to be cheap. Styles are shared between the threads
/// that build inline formatting contexts in parallel.
pub trait TextStyle: Clone + Send + Sync {
    // Font

    /// Returns the families of `font-family`, in order.
    fn font_family(&self) -> impl Iterator<Item = FontFamilyName<'_>> + Clone;

    /// Returns whether `font-family` is its initial value, which names no family a style sheet
    /// chose.
    fn font_family_is_initial(&self) -> bool;

    /// Returns the used font size, in CSS pixels.
    fn font_size(&self) -> f32;

    /// Returns `font-weight`.
    fn font_weight(&self) -> FontWeight;

    /// Returns `font-width` (`font-stretch`).
    fn font_width(&self) -> FontWidth;

    /// Returns `font-style`.
    fn font_style(&self) -> FontStyle;

    /// Returns `font-synthesis-weight`.
    fn font_synthesis_weight(&self) -> FontSynthesis;

    /// Returns `font-synthesis-style`.
    fn font_synthesis_style(&self) -> FontSynthesisStyle;

    /// Returns `font-synthesis-small-caps`.
    fn font_synthesis_small_caps(&self) -> FontSynthesis;

    /// Returns `font-synthesis-position`.
    fn font_synthesis_position(&self) -> FontSynthesis;

    /// Returns `font-variant-caps`.
    fn font_variant_caps(&self) -> FontVariantCaps;

    /// Returns `font-variant-position`.
    fn font_variant_position(&self) -> FontVariantPosition;

    /// Returns `font-variant-ligatures`.
    fn font_variant_ligatures(&self) -> FontVariantLigatures;

    /// Returns `font-variant-numeric`.
    fn font_variant_numeric(&self) -> FontVariantNumeric;

    /// Returns `font-variant-east-asian`.
    fn font_variant_east_asian(&self) -> FontVariantEastAsian;

    /// Returns the functions and keywords of `font-variant-alternates`, in order.
    fn font_variant_alternates(&self) -> impl Iterator<Item = VariantAlternate<'_>>;

    /// Returns `font-variant-emoji`.
    fn font_variant_emoji(&self) -> FontVariantEmoji;

    /// Returns the settings of `font-feature-settings`, in order.
    fn font_feature_settings(&self) -> impl Iterator<Item = FontFeature> + Clone;

    /// Returns the settings of `font-variation-settings`, in order.
    fn font_variation_settings(&self) -> impl Iterator<Item = FontVariation> + Clone;

    /// Returns `font-optical-sizing`.
    fn font_optical_sizing(&self) -> FontOpticalSizing;

    /// Returns `font-kerning`.
    fn font_kerning(&self) -> FontKerning;

    /// Returns `font-language-override`, or `None` for `normal`.
    fn font_language_override(&self) -> Option<Tag>;

    /// Returns `font-size-adjust`.
    fn font_size_adjust(&self) -> FontSizeAdjust;

    /// Returns `line-height`.
    fn line_height(&self) -> LineHeight;

    /// Returns the content language the style was computed for, as a BCP 47 tag, or an empty
    /// string where it has none.
    fn language(&self) -> &str;

    // Inherited text

    /// Returns `color`.
    fn color(&self) -> Color;

    /// Returns `text-align`.
    fn text_align(&self) -> TextAlign;

    /// Returns `text-align-last`.
    fn text_align_last(&self) -> TextAlignLast;

    /// Returns `text-indent`.
    fn text_indent(&self) -> TextIndent;

    /// Returns `text-justify`.
    fn text_justify(&self) -> TextJustify;

    /// Returns `text-transform`.
    fn text_transform(&self) -> TextTransform;

    /// Returns `white-space-collapse`.
    fn white_space_collapse(&self) -> WhiteSpaceCollapse;

    /// Returns `text-wrap-mode`.
    fn text_wrap_mode(&self) -> TextWrapMode;

    /// Returns `text-wrap-style`.
    fn text_wrap_style(&self) -> TextWrapStyle;

    /// Returns `word-break`.
    fn word_break(&self) -> WordBreak;

    /// Returns `line-break`.
    fn line_break(&self) -> LineBreak;

    /// Returns `overflow-wrap`.
    fn overflow_wrap(&self) -> OverflowWrap;

    /// Returns `hyphens`.
    fn hyphens(&self) -> Hyphens;

    /// Returns `hyphenate-character`, or `None` for `auto`.
    fn hyphenate_character(&self) -> Option<&str>;

    /// Returns `tab-size`.
    fn tab_size(&self) -> TabSize;

    /// Returns `letter-spacing`, whose percentages are of the font size.
    fn letter_spacing(&self) -> LengthPercentage;

    /// Returns `word-spacing`, whose percentages are of the font size.
    fn word_spacing(&self) -> LengthPercentage;

    /// Returns `text-autospace`.
    fn text_autospace(&self) -> TextAutospace;

    /// Returns `text-spacing-trim`.
    fn text_spacing_trim(&self) -> TextSpacingTrim;

    /// Returns `hanging-punctuation`.
    fn hanging_punctuation(&self) -> HangingPunctuation;

    /// Returns `text-emphasis-style`.
    fn text_emphasis_style(&self) -> TextEmphasisStyle<'_>;

    /// Returns `text-emphasis-position`.
    fn text_emphasis_position(&self) -> TextEmphasisPosition;

    /// Returns `text-emphasis-color`, `currentcolor` resolved to [`color`](Self::color).
    fn text_emphasis_color(&self) -> Color;

    /// Returns `-webkit-text-security`.
    fn text_security(&self) -> TextSecurity;

    /// Returns `text-fit`.
    fn text_fit(&self) -> TextFit;

    /// Returns `text-underline-offset`, or `None` for `auto`.
    fn text_underline_offset(&self) -> Option<LengthPercentage>;

    /// Returns `text-underline-position`.
    fn text_underline_position(&self) -> TextUnderlinePosition;

    /// Returns `text-decoration-skip-spaces`.
    fn text_decoration_skip_spaces(&self) -> TextDecorationSkipSpaces;

    /// Returns the shadows of `text-shadow`, the topmost first.
    fn text_shadow(&self) -> impl Iterator<Item = TextShadow> + Clone;

    /// Returns how many shadows `text-shadow` casts.
    fn text_shadow_count(&self) -> usize {
        self.text_shadow().count()
    }

    /// Returns `ruby-position`.
    fn ruby_position(&self) -> RubyPosition;

    /// Returns `ruby-align`.
    fn ruby_align(&self) -> RubyAlign;

    /// Returns `ruby-overhang`.
    fn ruby_overhang(&self) -> RubyOverhang;

    /// Returns `text-combine-upright`.
    fn text_combine_upright(&self) -> TextCombineUpright;

    // Inherited box

    /// Returns `direction`.
    fn direction(&self) -> Direction;

    /// Returns the way lines run and stack, from `writing-mode`.
    fn writing_mode(&self) -> WritingMode;

    /// Returns `text-orientation`.
    fn text_orientation(&self) -> TextOrientation;

    // Text

    /// Returns `text-decoration-line`.
    fn text_decoration_line(&self) -> TextDecorationLine;

    /// Returns `text-decoration-style`.
    fn text_decoration_style(&self) -> TextDecorationStyle;

    /// Returns `text-decoration-color`, where it is an absolute color; `currentcolor`, and any
    /// color that reads it, is [`color`](Self::color).
    fn text_decoration_color(&self) -> Color;

    /// Returns `text-decoration-thickness`.
    fn text_decoration_thickness(&self) -> TextDecorationThickness;

    /// Returns `text-decoration-inset`.
    fn text_decoration_inset(&self) -> TextDecorationInset;

    /// Returns the end side of `text-overflow`.
    fn text_overflow(&self) -> TextOverflow;

    /// Returns `unicode-bidi`.
    fn unicode_bidi(&self) -> UnicodeBidi;

    /// Returns `text-box-trim`.
    fn text_box_trim(&self) -> TextBoxTrim;

    /// Returns `text-box-edge`.
    fn text_box_edge(&self) -> TextBoxEdge;

    /// Returns `initial-letter`.
    fn initial_letter(&self) -> InitialLetter;

    /// Returns `dominant-baseline`.
    fn dominant_baseline(&self) -> DominantBaseline;

    /// Returns `line-clamp`, or where it is `none`, `-webkit-line-clamp`.
    fn line_clamp(&self) -> LineClamp;

    // Box

    /// Returns `display`.
    fn display(&self) -> Display;

    /// Returns `float`.
    fn float(&self) -> Float;

    /// Returns `position`.
    fn position(&self) -> Position;

    /// Returns `alignment-baseline`.
    fn alignment_baseline(&self) -> AlignmentBaseline;

    /// Returns `baseline-shift`.
    fn baseline_shift(&self) -> BaselineShift;

    /// Returns `overflow-x`.
    fn overflow_x(&self) -> Overflow;

    // Margin, padding and border

    /// Returns `margin`.
    fn margin(&self) -> Sides<LengthPercentageOrAuto>;

    /// Returns `padding`.
    fn padding(&self) -> Sides<LengthPercentage>;

    /// Returns the widths of `border`, in CSS pixels.
    fn border_widths(&self) -> Sides<f32>;

    /// Returns the styles of `border`.
    fn border_styles(&self) -> Sides<BorderStyle>;

    /// Returns the colors of `border`, `currentcolor` resolved to [`color`](Self::color).
    fn border_colors(&self) -> Sides<Color>;

    /// Returns `border-radius`.
    fn border_radii(&self) -> Corners<CornerRadius>;

    /// Returns `box-decoration-break`.
    fn box_decoration_break(&self) -> BoxDecorationBreak;

    // Outline

    /// Returns `outline-style`.
    fn outline_style(&self) -> OutlineStyle;

    /// Returns `outline-width`, in CSS pixels.
    fn outline_width(&self) -> f32;

    /// Returns `outline-color`, `currentcolor` resolved to [`color`](Self::color).
    fn outline_color(&self) -> Color;

    // Background

    /// Returns `background-color`, `currentcolor` resolved to [`color`](Self::color).
    fn background_color(&self) -> Color;

    /// Returns whether `background-image` has any image.
    fn has_background_image(&self) -> bool;

    // Box shadow

    /// Returns whether `box-shadow` has any shadow.
    fn has_box_shadow(&self) -> bool;

    // Group comparisons

    /// Returns whether the font properties of the two styles are the same.
    fn same_font(&self, other: &Self) -> bool {
        self.font_family().eq(other.font_family())
            && self.font_family_is_initial() == other.font_family_is_initial()
            && self.font_size() == other.font_size()
            && self.font_weight() == other.font_weight()
            && self.font_width() == other.font_width()
            && self.font_style() == other.font_style()
            && self.font_synthesis_weight() == other.font_synthesis_weight()
            && self.font_synthesis_style() == other.font_synthesis_style()
            && self.font_synthesis_small_caps() == other.font_synthesis_small_caps()
            && self.font_synthesis_position() == other.font_synthesis_position()
            && self.font_variant_caps() == other.font_variant_caps()
            && self.font_variant_position() == other.font_variant_position()
            && self.font_variant_ligatures() == other.font_variant_ligatures()
            && self.font_variant_numeric() == other.font_variant_numeric()
            && self.font_variant_east_asian() == other.font_variant_east_asian()
            && self
                .font_variant_alternates()
                .eq(other.font_variant_alternates())
            && self.font_variant_emoji() == other.font_variant_emoji()
            && self
                .font_feature_settings()
                .eq(other.font_feature_settings())
            && self
                .font_variation_settings()
                .eq(other.font_variation_settings())
            && self.font_optical_sizing() == other.font_optical_sizing()
            && self.font_kerning() == other.font_kerning()
            && self.font_language_override() == other.font_language_override()
            && self.font_size_adjust() == other.font_size_adjust()
            && self.line_height() == other.line_height()
            && self.language() == other.language()
    }

    /// Returns whether the inherited text properties a block reads for its lines as a whole are
    /// the same: `text-align`, `text-align-last`, `text-indent`, `text-wrap-style` and
    /// `text-fit`.
    fn same_block_text(&self, other: &Self) -> bool {
        self.text_align() == other.text_align()
            && self.text_align_last() == other.text_align_last()
            && self.same_block_text_but_alignment(other)
    }

    /// Returns whether, of the properties [`same_block_text`](Self::same_block_text) compares,
    /// those other than `text-align` and `text-align-last` are the same.
    fn same_block_text_but_alignment(&self, other: &Self) -> bool {
        self.text_indent() == other.text_indent()
            && self.text_wrap_style() == other.text_wrap_style()
            && self.text_fit() == other.text_fit()
    }

    /// Returns whether the inherited text properties other than those
    /// [`same_block_text`](Self::same_block_text) compares are the same.
    fn same_inline_text(&self, other: &Self) -> bool {
        self.color() == other.color()
            && self.text_justify() == other.text_justify()
            && self.text_transform() == other.text_transform()
            && self.white_space_collapse() == other.white_space_collapse()
            && self.text_wrap_mode() == other.text_wrap_mode()
            && self.word_break() == other.word_break()
            && self.line_break() == other.line_break()
            && self.overflow_wrap() == other.overflow_wrap()
            && self.hyphens() == other.hyphens()
            && self.hyphenate_character() == other.hyphenate_character()
            && self.tab_size() == other.tab_size()
            && self.letter_spacing() == other.letter_spacing()
            && self.word_spacing() == other.word_spacing()
            && self.text_autospace() == other.text_autospace()
            && self.text_spacing_trim() == other.text_spacing_trim()
            && self.hanging_punctuation() == other.hanging_punctuation()
            && self.text_emphasis_style() == other.text_emphasis_style()
            && self.text_emphasis_position() == other.text_emphasis_position()
            && self.text_emphasis_color() == other.text_emphasis_color()
            && self.text_security() == other.text_security()
            && self.text_underline_offset() == other.text_underline_offset()
            && self.text_underline_position() == other.text_underline_position()
            && self.text_decoration_skip_spaces() == other.text_decoration_skip_spaces()
            && self.text_shadow().eq(other.text_shadow())
            && self.ruby_position() == other.ruby_position()
            && self.ruby_align() == other.ruby_align()
            && self.ruby_overhang() == other.ruby_overhang()
            && self.text_combine_upright() == other.text_combine_upright()
    }

    /// Returns whether the inherited box properties are the same.
    fn same_inherited_box(&self, other: &Self) -> bool {
        self.direction() == other.direction()
            && self.writing_mode() == other.writing_mode()
            && self.text_orientation() == other.text_orientation()
    }

    /// Returns whether the text properties that do not inherit are the same.
    fn same_text(&self, other: &Self) -> bool {
        self.text_decoration_line() == other.text_decoration_line()
            && self.text_decoration_style() == other.text_decoration_style()
            && self.text_decoration_color() == other.text_decoration_color()
            && self.text_decoration_thickness() == other.text_decoration_thickness()
            && self.text_decoration_inset() == other.text_decoration_inset()
            && self.text_overflow() == other.text_overflow()
            && self.unicode_bidi() == other.unicode_bidi()
            && self.text_box_trim() == other.text_box_trim()
            && self.text_box_edge() == other.text_box_edge()
            && self.initial_letter() == other.initial_letter()
            && self.dominant_baseline() == other.dominant_baseline()
            && self.line_clamp() == other.line_clamp()
    }

    /// Returns whether the box properties are the same.
    fn same_box(&self, other: &Self) -> bool {
        self.display() == other.display()
            && self.float() == other.float()
            && self.position() == other.position()
            && self.alignment_baseline() == other.alignment_baseline()
            && self.baseline_shift() == other.baseline_shift()
            && self.overflow_x() == other.overflow_x()
    }

    /// Returns whether the margins are the same.
    fn same_margin(&self, other: &Self) -> bool {
        self.margin() == other.margin()
    }

    /// Returns whether the paddings are the same.
    fn same_padding(&self, other: &Self) -> bool {
        self.padding() == other.padding()
    }

    /// Returns whether the borders are the same.
    fn same_border(&self, other: &Self) -> bool {
        self.border_widths() == other.border_widths()
            && self.border_styles() == other.border_styles()
            && self.border_colors() == other.border_colors()
            && self.border_radii() == other.border_radii()
            && self.box_decoration_break() == other.box_decoration_break()
    }
}
