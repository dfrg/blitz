//! The text layout interface Blitz lays out, edits and paints text through.
//!
//! Blitz lays out text with one backend, chosen when it is built: `blitz-text-parley` or
//! `blitz-text-winkin`. This crate is what they share with the host that drives them. It knows
//! nothing of the DOM or of a style engine, and neither do the backends: the host reads both, and
//! hands the backend what it needs through the traits here.
//!
//! # Style
//!
//! [`TextStyle`] is an element's computed style as text reads it: one accessor a property,
//! answering with this crate's small value types, and lists as borrowed iterators. Blitz
//! implements it over Stylo's computed values in `blitz-dom`; [`OwnedTextStyle`] holds every
//! property as plain values, for tests and hosts without a style engine. The `same_*` methods
//! compare a group of properties, which a style engine that shares groups between styles answers
//! by pointer first; each backend's [`InlineLayout::style_change_rebuilds`] is written against
//! them.
//!
//! # Building, layout and paint
//!
//! - [`InlineSource`]: the host's side of a build. It describes an inline formatting context's
//!   block ([`BlockInputs`]) and walks its content into the backend's [`InlineBuilder`], keying
//!   each node with an opaque [`NodeKey`].
//! - [`InlineLayout`] and [`InlineText`]: the content measured with the host's measures of its
//!   atomic inlines, broken into lines around the host's floats ([`LineExclusions`]), placed, and
//!   read back for hit testing, selection, carets and node fragments.
//! - [`EditEngine`] and [`EditableText`]: the text of a form control and its editing.
//! - [`MarkerEngine`]: an outside list marker.
//! - [`TextFonts`] and [`DocumentText`]: the fonts documents lay text out with, `@font-face`
//!   registration, and the font metrics a style engine reads ([`FontMetricsSource`]).
//! - [`TextSink`] and [`PaintHost`]: a backend paints into a sink, which a renderer implements,
//!   and asks the host's document for styles while it paints.
//! - [`Backend`] names one backend's types.
//!
//! # Why generic
//!
//! Every backend type is generic over its style, `S: TextStyle`, and over the sink, the host
//! and the builder it is handed, and is monomorphized where the host instantiates it. The style
//! accessors run in the hottest loops of building and painting, and inline into them without
//! link-time optimization only where the code is generic. Nothing on those paths takes a
//! `dyn` trait object.
//!
//! # What stays in the host
//!
//! The host keeps what reads its document: the walk of the DOM in content order, white space and
//! `text-transform` handling where a backend leaves it to the host, `::first-line` and
//! `::first-letter` cascades, `lang` and `<br clear>` attributes, the measuring and placing of
//! atomic inlines and floats, `position: relative` offsets, the styles a paint asks for by key,
//! the style engine's font-metric and `@font-face` glue, and the editor's key bindings.

mod backend;
mod build;
mod edit;
mod fonts;
mod key;
mod layout;
pub mod paint;
pub mod style;

pub use backend::Backend;
pub use build::{
    BlockInputs, FeatureBlock, FloatSide, FontFeatureValues, InlineBuilder, InlineSource, Marker,
    SpanKind,
};
pub use edit::{Edit, EditEngine, EditableText, EditorMetrics, MarkerEngine, MarkerFrame, Motion};
pub use fonts::{
    BULLET_FONT, DocumentText, FaceDescriptors, FontMetrics, FontMetricsSource, FontQuery,
    TextFonts, decode_font_bytes,
};
pub use key::NodeKey;
pub use layout::{
    BoxMeasure, ContentItem, ContentWidths, FloatRequest, InlineLayout, InlineText, LastBaseline,
    LineArea, LineExclusions, LineFlow, LinesExtent, LinesInputs, Measure, PlacedFloat, Placement,
    Size, TextHit,
};
pub use paint::{PaintFrame, PaintHost, PaintScratch, TextSink};
pub use style::{OwnedTextStyle, TextStyle};

pub use bytes::Bytes;
pub use parlance;
