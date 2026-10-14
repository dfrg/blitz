//! The values [`TextStyle`](super::TextStyle) answers with: computed CSS values, as text layout
//! and painting read them.
//!
//! Lengths are in CSS pixels. A percentage is kept as a fraction of whatever it is resolved
//! against, which only layout knows.

/// A color, in sRGB with alpha.
pub type Color = color::AlphaColor<color::Srgb>;

/// A computed `<length-percentage>`: a length in CSS pixels, a fraction of a basis, or a `calc()`
/// of both.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LengthPercentage {
    /// A length, in CSS pixels.
    Length(f32),
    /// A percentage, as a fraction: `0.5` is `50%`.
    Percentage(f32),
    /// A `calc()` mixing a length and a percentage, as the length it is against a basis of zero
    /// and the fraction it grows by with the basis.
    Calc {
        /// The length at a basis of zero, in CSS pixels.
        px: f32,
        /// How much the value grows a CSS pixel of basis.
        fraction: f32,
    },
}

impl Default for LengthPercentage {
    fn default() -> Self {
        Self::Length(0.0)
    }
}

impl LengthPercentage {
    /// Returns the value against `basis` CSS pixels.
    #[inline]
    pub fn resolve(self, basis: f32) -> f32 {
        match self {
            Self::Length(px) => px,
            Self::Percentage(fraction) => basis * fraction,
            Self::Calc { px, fraction } => px + basis * fraction,
        }
    }

    /// Returns the length part, in CSS pixels.
    #[inline]
    pub fn px(self) -> f32 {
        match self {
            Self::Length(px) | Self::Calc { px, .. } => px,
            Self::Percentage(_) => 0.0,
        }
    }

    /// Returns the percentage part, as a fraction.
    #[inline]
    pub fn fraction(self) -> f32 {
        match self {
            Self::Length(_) => 0.0,
            Self::Percentage(fraction) | Self::Calc { fraction, .. } => fraction,
        }
    }

    /// Returns whether the value has a percentage part.
    #[inline]
    pub fn has_percentage(self) -> bool {
        !matches!(self, Self::Length(_))
    }
}

/// A computed `<length-percentage> | auto`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum LengthPercentageOrAuto {
    /// `auto`.
    #[default]
    Auto,
    /// A length or a percentage.
    LengthPercentage(LengthPercentage),
}

/// One value for each side of a box.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sides<T> {
    /// The top side.
    pub top: T,
    /// The right side.
    pub right: T,
    /// The bottom side.
    pub bottom: T,
    /// The left side.
    pub left: T,
}

impl<T: Copy> Sides<T> {
    /// Returns the same value on every side.
    pub const fn all(value: T) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}

/// One value for each corner of a box.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Corners<T> {
    /// The top left corner.
    pub top_left: T,
    /// The top right corner.
    pub top_right: T,
    /// The bottom right corner.
    pub bottom_right: T,
    /// The bottom left corner.
    pub bottom_left: T,
}

/// A computed `border-*-radius`: the horizontal and vertical radius.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CornerRadius {
    /// The horizontal radius, whose percentages are of the box's width.
    pub width: LengthPercentage,
    /// The vertical radius, whose percentages are of the box's height.
    pub height: LengthPercentage,
}

macro_rules! flags {
    ($(#[$meta:meta])* $name:ident: $repr:ty { $($(#[$flag_meta:meta])* $flag:ident = $bit:expr;)* }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $name($repr);

        impl $name {
            $($(#[$flag_meta])* pub const $flag: Self = Self($bit);)*

            /// Returns the set with no flag.
            #[inline]
            pub const fn empty() -> Self {
                Self(0)
            }

            /// Returns whether every flag of `other` is set.
            #[inline]
            pub const fn contains(self, other: Self) -> bool {
                self.0 & other.0 == other.0
            }

            /// Returns whether any flag of `other` is set.
            #[inline]
            pub const fn intersects(self, other: Self) -> bool {
                self.0 & other.0 != 0
            }

            /// Returns whether no flag is set.
            #[inline]
            pub const fn is_empty(self) -> bool {
                self.0 == 0
            }

            /// Returns the flags of both sets.
            #[inline]
            pub const fn union(self, other: Self) -> Self {
                Self(self.0 | other.0)
            }

            /// Returns the flags of `self` that are set in `other`.
            #[inline]
            pub const fn intersection(self, other: Self) -> Self {
                Self(self.0 & other.0)
            }
        }

        impl core::ops::BitOr for $name {
            type Output = Self;
            #[inline]
            fn bitor(self, other: Self) -> Self {
                self.union(other)
            }
        }

        impl core::ops::BitOrAssign for $name {
            #[inline]
            fn bitor_assign(&mut self, other: Self) {
                *self = self.union(other);
            }
        }

        impl core::ops::BitAnd for $name {
            type Output = Self;
            #[inline]
            fn bitand(self, other: Self) -> Self {
                self.intersection(other)
            }
        }
    };
}

// Font

/// A computed `line-height`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum LineHeight {
    /// `normal`: the font's own line spacing.
    #[default]
    Normal,
    /// A number, which multiplies the font size.
    Number(f32),
    /// A length, in CSS pixels.
    Length(f32),
}

/// A computed `font-synthesis-weight`, `font-synthesis-small-caps` or
/// `font-synthesis-position`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontSynthesis {
    /// `auto`: the face may be synthesized.
    #[default]
    Auto,
    /// `none`.
    None,
}

/// A computed `font-synthesis-style`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontSynthesisStyle {
    /// `auto`: an italic or an oblique may be synthesized.
    #[default]
    Auto,
    /// `none`.
    None,
    /// `oblique-only`: only an oblique may be synthesized.
    ObliqueOnly,
}

/// A computed `font-variant-caps`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontVariantCaps {
    #[default]
    Normal,
    SmallCaps,
    AllSmallCaps,
    PetiteCaps,
    AllPetiteCaps,
    Unicase,
    TitlingCaps,
}

/// A computed `font-variant-position`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontVariantPosition {
    #[default]
    Normal,
    Sub,
    Super,
}

/// A computed `font-variant-emoji`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontVariantEmoji {
    #[default]
    Normal,
    Text,
    Emoji,
    Unicode,
}

/// A computed `font-optical-sizing`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontOpticalSizing {
    #[default]
    Auto,
    None,
}

/// A computed `font-kerning`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontKerning {
    #[default]
    Auto,
    Normal,
    None,
}

flags! {
    /// A computed `font-variant-ligatures`: `normal` is the empty set.
    FontVariantLigatures: u16 {
        /// `none`.
        NONE = 1 << 0;
        COMMON_LIGATURES = 1 << 1;
        NO_COMMON_LIGATURES = 1 << 2;
        DISCRETIONARY_LIGATURES = 1 << 3;
        NO_DISCRETIONARY_LIGATURES = 1 << 4;
        HISTORICAL_LIGATURES = 1 << 5;
        NO_HISTORICAL_LIGATURES = 1 << 6;
        CONTEXTUAL = 1 << 7;
        NO_CONTEXTUAL = 1 << 8;
    }
}

flags! {
    /// A computed `font-variant-numeric`: `normal` is the empty set.
    FontVariantNumeric: u16 {
        LINING_NUMS = 1 << 0;
        OLDSTYLE_NUMS = 1 << 1;
        PROPORTIONAL_NUMS = 1 << 2;
        TABULAR_NUMS = 1 << 3;
        DIAGONAL_FRACTIONS = 1 << 4;
        STACKED_FRACTIONS = 1 << 5;
        ORDINAL = 1 << 6;
        SLASHED_ZERO = 1 << 7;
    }
}

flags! {
    /// A computed `font-variant-east-asian`: `normal` is the empty set.
    FontVariantEastAsian: u16 {
        JIS78 = 1 << 0;
        JIS83 = 1 << 1;
        JIS90 = 1 << 2;
        JIS04 = 1 << 3;
        SIMPLIFIED = 1 << 4;
        TRADITIONAL = 1 << 5;
        FULL_WIDTH = 1 << 6;
        PROPORTIONAL_WIDTH = 1 << 7;
        RUBY = 1 << 8;
    }
}

/// One function or keyword of a computed `font-variant-alternates`, naming a value of the
/// document's `@font-feature-values` rules.
///
/// `styleset()` and `character-variant()` come as one item for each name they list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VariantAlternate<'a> {
    /// `stylistic(name)`.
    Stylistic(&'a str),
    /// `swash(name)`.
    Swash(&'a str),
    /// `ornaments(name)`.
    Ornaments(&'a str),
    /// `annotation(name)`.
    Annotation(&'a str),
    /// A name of `styleset()`.
    Styleset(&'a str),
    /// A name of `character-variant()`.
    CharacterVariant(&'a str),
    /// `historical-forms`.
    HistoricalForms,
}

/// A computed `font-size-adjust`: the metric to scale the font by, and its value.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum FontSizeAdjust {
    #[default]
    None,
    ExHeight(f32),
    CapHeight(f32),
    ChWidth(f32),
    IcWidth(f32),
    IcHeight(f32),
}

// Inherited text

/// A computed `text-align`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextAlign {
    #[default]
    Start,
    End,
    Left,
    Right,
    Center,
    Justify,
}

/// A computed `text-align-last`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextAlignLast {
    #[default]
    Auto,
    Start,
    End,
    Left,
    Right,
    Center,
    Justify,
}

/// A computed `text-indent`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextIndent {
    /// The indent, whose percentages are of the containing block's inline size.
    pub length: LengthPercentage,
    /// `hanging`: every line but the first is indented.
    pub hanging: bool,
    /// `each-line`: each line after a forced break is indented too.
    pub each_line: bool,
}

/// A computed `text-justify`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextJustify {
    #[default]
    Auto,
    None,
    InterWord,
    InterCharacter,
}

flags! {
    /// A computed `text-transform`: `none` is the empty set.
    TextTransform: u8 {
        UPPERCASE = 1 << 0;
        LOWERCASE = 1 << 1;
        CAPITALIZE = 1 << 2;
        MATH_AUTO = 1 << 3;
        FULL_WIDTH = 1 << 4;
        FULL_SIZE_KANA = 1 << 5;
    }
}

/// A computed `white-space-collapse`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WhiteSpaceCollapse {
    #[default]
    Collapse,
    Preserve,
    PreserveBreaks,
    PreserveSpaces,
    BreakSpaces,
}

/// A computed `text-wrap-mode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextWrapMode {
    #[default]
    Wrap,
    NoWrap,
}

/// A computed `text-wrap-style`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextWrapStyle {
    #[default]
    Auto,
    Stable,
    Balance,
    Pretty,
}

/// A computed `word-break`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WordBreak {
    #[default]
    Normal,
    BreakAll,
    KeepAll,
    /// `break-word`: `normal` with `overflow-wrap: anywhere`.
    BreakWord,
}

/// A computed `line-break`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineBreak {
    #[default]
    Auto,
    Loose,
    Normal,
    Strict,
    Anywhere,
}

/// A computed `overflow-wrap`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum OverflowWrap {
    #[default]
    Normal,
    BreakWord,
    Anywhere,
}

/// A computed `hyphens`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Hyphens {
    None,
    #[default]
    Manual,
    Auto,
}

/// A computed `tab-size`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TabSize {
    /// A number of spaces.
    Spaces(f32),
    /// A length, in CSS pixels.
    Length(f32),
}

impl Default for TabSize {
    fn default() -> Self {
        Self::Spaces(8.0)
    }
}

flags! {
    /// A computed `text-autospace`.
    TextAutospace: u8 {
        NORMAL = 1 << 0;
        AUTO = 1 << 1;
        NO_AUTOSPACE = 1 << 2;
        IDEOGRAPH_ALPHA = 1 << 3;
        IDEOGRAPH_NUMERIC = 1 << 4;
        INSERT = 1 << 5;
        REPLACE = 1 << 6;
    }
}

/// A computed `text-spacing-trim`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextSpacingTrim {
    #[default]
    Normal,
    SpaceAll,
    SpaceFirst,
    TrimStart,
}

flags! {
    /// A computed `hanging-punctuation`: `none` is the empty set.
    HangingPunctuation: u8 {
        FIRST = 1 << 0;
        LAST = 1 << 1;
        FORCE_END = 1 << 2;
        ALLOW_END = 1 << 3;
    }
}

/// Whether `text-emphasis-style` marks are filled or open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EmphasisFill {
    #[default]
    Filled,
    Open,
}

/// The shape of a keyword `text-emphasis-style` mark.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EmphasisShape {
    #[default]
    Dot,
    Circle,
    DoubleCircle,
    Triangle,
    Sesame,
}

/// A computed `text-emphasis-style`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextEmphasisStyle<'a> {
    #[default]
    None,
    /// A keyword mark.
    Keyword {
        fill: EmphasisFill,
        shape: EmphasisShape,
    },
    /// A string, whose first character is the mark.
    String(&'a str),
}

flags! {
    /// A computed `text-emphasis-position`.
    TextEmphasisPosition: u8 {
        AUTO = 1 << 0;
        OVER = 1 << 1;
        UNDER = 1 << 2;
        LEFT = 1 << 3;
        RIGHT = 1 << 4;
    }
}

/// A computed `-webkit-text-security`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextSecurity {
    #[default]
    None,
    Disc,
    Circle,
    Square,
}

/// What a computed `text-fit` does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextFitKind {
    #[default]
    None,
    Grow,
    Shrink,
}

/// Which lines a computed `text-fit` fits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextFitTarget {
    #[default]
    Consistent,
    PerLine,
    PerLineAll,
}

/// A computed `text-fit`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextFit {
    pub kind: TextFitKind,
    pub target: TextFitTarget,
    /// The limit, as a fraction, where one is given.
    pub limit: Option<f32>,
}

flags! {
    /// A computed `text-underline-position`: `auto` is the empty set.
    TextUnderlinePosition: u8 {
        FROM_FONT = 1 << 0;
        UNDER = 1 << 1;
        LEFT = 1 << 2;
        RIGHT = 1 << 3;
    }
}

/// One shadow of a computed `text-shadow`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextShadow {
    /// The shadow's color, `currentcolor` resolved to the style's `color`.
    pub color: Color,
    /// Its offset, in CSS pixels.
    pub x: f32,
    pub y: f32,
    /// Its blur radius, in CSS pixels.
    pub blur: f32,
}

/// A computed `ruby-position`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RubyPosition {
    /// `alternate` (`alternate over`).
    #[default]
    AlternateOver,
    /// `alternate under`.
    AlternateUnder,
    Over,
    Under,
}

/// A computed `ruby-align`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RubyAlign {
    #[default]
    SpaceAround,
    SpaceBetween,
    Center,
    Start,
}

/// A computed `ruby-overhang`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RubyOverhang {
    #[default]
    Auto,
    Spaces,
}

/// A computed `text-orientation`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextOrientation {
    #[default]
    Mixed,
    Upright,
    Sideways,
}

/// A computed `text-combine-upright`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextCombineUpright {
    #[default]
    None,
    All,
    /// `digits`, and how many.
    Digits(u8),
}

// Inherited box

/// A computed `direction`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

/// A computed `writing-mode`, the way lines run and stack.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WritingMode {
    #[default]
    HorizontalTb,
    VerticalRl,
    VerticalLr,
    SidewaysRl,
    SidewaysLr,
}

impl WritingMode {
    /// Returns whether lines run down or up the page.
    #[inline]
    pub fn is_vertical(self) -> bool {
        self != Self::HorizontalTb
    }
}

// Text

flags! {
    /// A computed `text-decoration-line`: `none` is the empty set.
    TextDecorationLine: u8 {
        UNDERLINE = 1 << 0;
        OVERLINE = 1 << 1;
        LINE_THROUGH = 1 << 2;
        BLINK = 1 << 3;
        SPELLING_ERROR = 1 << 4;
        GRAMMAR_ERROR = 1 << 5;
    }
}

impl TextDecorationLine {
    /// The lines that are drawn: the underline, the overline and the line through.
    pub const DRAWN: Self = Self(Self::UNDERLINE.0 | Self::OVERLINE.0 | Self::LINE_THROUGH.0);

    /// Returns every line.
    pub const fn all() -> Self {
        Self(0b11_1111)
    }
}

/// A computed `text-decoration-style`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextDecorationStyle {
    #[default]
    Solid,
    Double,
    Dotted,
    Dashed,
    Wavy,
    /// No line: `-moz-none`.
    None,
}

/// A computed `text-decoration-thickness`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TextDecorationThickness {
    #[default]
    Auto,
    FromFont,
    /// A length, whose percentages are of the font size.
    Length(LengthPercentage),
}

/// A computed `text-decoration-inset`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TextDecorationInset {
    #[default]
    Auto,
    /// The insets from the start and the end, whose percentages are of the line's length.
    Length {
        start: LengthPercentage,
        end: LengthPercentage,
    },
}

/// The end side of a computed `text-overflow`, which is the side a line overflows at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
    /// A string.
    String,
}

/// A computed `unicode-bidi`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UnicodeBidi {
    #[default]
    Normal,
    Embed,
    Isolate,
    BidiOverride,
    IsolateOverride,
    Plaintext,
}

/// A computed `text-box-trim`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextBoxTrim {
    #[default]
    None,
    TrimStart,
    TrimEnd,
    TrimBoth,
}

/// A keyword of a computed `text-box-edge`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextEdge {
    #[default]
    Text,
    Cap,
    Ex,
    Ideographic,
    IdeographicInk,
    Alphabetic,
}

/// A computed `text-box-edge`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextBoxEdge {
    #[default]
    Auto,
    /// The over and under edges.
    Edges { over: TextEdge, under: TextEdge },
}

/// How a computed `initial-letter` sinks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum InitialLetterSink {
    /// Left out, or `drop`: as many lines as the size, floored.
    #[default]
    Drop,
    /// `raise`.
    Raise,
    /// A number of lines.
    Lines(i32),
}

/// A computed `initial-letter`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InitialLetter {
    /// How many lines tall it is; zero for `normal`.
    pub size: f32,
    pub sink: InitialLetterSink,
}

/// A computed `dominant-baseline`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DominantBaseline {
    #[default]
    Auto,
    Alphabetic,
    Ideographic,
    Central,
    Middle,
    Mathematical,
    Hanging,
    TextTop,
    TextBottom,
}

/// A computed `line-clamp`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineClamp {
    #[default]
    None,
    Auto,
    /// A number of lines.
    Lines(i32),
}

// Box

/// The outer display type of a computed `display`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DisplayOutside {
    #[default]
    Inline,
    Block,
    /// `none`, or `contents`, which generate no box.
    None,
    /// Any other, such as a table's internal boxes.
    Other,
}

/// The inner display type of a computed `display`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DisplayInside {
    #[default]
    Flow,
    FlowRoot,
    /// `contents`.
    Contents,
    /// `none`.
    None,
    /// Any other: flex, grid, table and the rest.
    Other,
}

/// A computed `display`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Display {
    pub outside: DisplayOutside,
    pub inside: DisplayInside,
}

impl Display {
    /// Returns whether it is `contents`, which generates no box.
    #[inline]
    pub fn is_contents(self) -> bool {
        self.inside == DisplayInside::Contents
    }
}

/// A computed `float`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Float {
    #[default]
    None,
    Left,
    Right,
    InlineStart,
    InlineEnd,
}

/// A computed `position`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

/// A computed `alignment-baseline`, the keyword half of `vertical-align`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AlignmentBaseline {
    #[default]
    Baseline,
    TextTop,
    TextBottom,
    Middle,
}

/// A computed `baseline-shift`, the shift half of `vertical-align`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BaselineShift {
    Sub,
    Super,
    Top,
    Bottom,
    Center,
    /// A length, whose percentages are of the line height.
    Length(LengthPercentage),
}

impl Default for BaselineShift {
    fn default() -> Self {
        Self::Length(LengthPercentage::default())
    }
}

/// A computed `overflow-x` or `overflow-y`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Overflow {
    #[default]
    Visible,
    Hidden,
    Clip,
    Scroll,
    Auto,
}

/// A computed `box-decoration-break`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BoxDecorationBreak {
    #[default]
    Slice,
    Clone,
}

// Border and outline

/// A computed `border-*-style`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BorderStyle {
    Hidden,
    #[default]
    None,
    Inset,
    Groove,
    Outset,
    Ridge,
    Dotted,
    Dashed,
    Solid,
    Double,
}

impl BorderStyle {
    /// Returns whether no border is drawn in it.
    #[inline]
    pub fn none_or_hidden(self) -> bool {
        matches!(self, Self::None | Self::Hidden)
    }
}

/// A computed `outline-style`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OutlineStyle {
    Auto,
    /// A border style.
    Border(BorderStyle),
}

impl Default for OutlineStyle {
    fn default() -> Self {
        Self::Border(BorderStyle::None)
    }
}

/// The side a `<br>` clears floats on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Clear {
    Left,
    Right,
    Both,
}
