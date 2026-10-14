//! A backend's types, named together.

use crate::edit::{EditEngine, MarkerEngine};
use crate::fonts::DocumentText;
use crate::layout::InlineLayout;
use crate::style::TextStyle;

/// A text backend: the types it lays out, edits and paints text with, for styles of type `S`.
///
/// A host picks one backend when it is built and names its types through this trait, so that
/// the rest of the host is written once against the traits they implement.
pub trait Backend<S: TextStyle>: 'static {
    /// The backend's name, for logs and tests.
    const NAME: &'static str;

    /// The fonts documents lay out text with. Clones share their fonts.
    type Fonts: crate::TextFonts;

    /// What a document keeps for its text: its fonts, and the backend's contexts.
    type Context: DocumentText<Fonts = Self::Fonts>;

    /// An inline formatting context.
    type Layout: InlineLayout<S, Context = Self::Context>;

    /// The text of an `<input>` or `<textarea>`.
    type Editor: EditEngine<S, Context = Self::Context>;

    /// An outside list marker.
    type Marker: MarkerEngine<S, Context = Self::Context, Lines = Self::Layout>;
}
