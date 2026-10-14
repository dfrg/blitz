//! Editing the text of a form control, and laying out list markers.

use core::cell::RefCell;
use std::borrow::Cow;
use std::ops::Range;

use kurbo::{Affine, Point, Rect, Size};

use crate::NodeKey;
use crate::build::Marker;
use crate::paint::{PaintHost, PaintScratch, TextSink};
use crate::style::{Color, TextStyle};

/// The size and scale of the laid-out text of an `<input>` or `<textarea>`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorMetrics {
    /// How wide and tall the laid-out text is, in device pixels.
    pub size: Size,
    /// Device pixels per CSS pixel that the text is laid out at.
    pub scale: f32,
}

/// The text of an `<input>` or `<textarea>`, as it is read back.
pub trait EditableText {
    /// Returns the text, with any text an input method is composing, which the selection
    /// indexes.
    fn raw_text(&self) -> &str;

    /// Returns the text without any text an input method is composing, as a form submits it:
    /// borrowed where nothing is composing.
    fn text(&self) -> Cow<'_, str>;

    /// Returns the selection's byte range in [`raw_text`](Self::raw_text).
    fn selection(&self) -> Range<usize>;

    /// Returns the selected text, where the selection is not empty.
    fn selected_text(&self) -> Option<&str>;

    /// Returns whether the selection is a caret.
    fn is_selection_collapsed(&self) -> bool;

    /// Returns the text a password field shows in the clear, in bytes of
    /// [`raw_text`](Self::raw_text), where it shows any.
    fn revealed_range(&self) -> Option<Range<usize>>;

    /// Returns the size and scale of the laid-out text, or `None` where it is not laid out.
    fn metrics(&self) -> Option<EditorMetrics>;

    /// Returns the caret, in device pixels relative to the text's origin, where the editor shows
    /// one.
    fn caret_rect(&self) -> Option<Rect>;
}

/// Changing and painting the text of an `<input>` or `<textarea>`: the part of
/// [`EditableText`] the host drives.
///
/// The setters record what changes, and the text is laid out again once, by the next
/// [`refresh`](Self::refresh) or [`edit`](Self::edit).
pub trait EditEngine<S: TextStyle>: EditableText + Sized {
    /// What a document keeps for its text, which the text is laid out with.
    type Context;

    /// Returns an empty editor.
    fn new(is_multiline: bool) -> Self;

    /// Sets the text the editor starts with.
    fn set_initial_text(&mut self, text: &str);

    /// Sets the text, where it changed.
    fn set_text(&mut self, text: &str);

    /// Sets the text in `style`, the computed style of the control `key`, at `scale`, with lines
    /// unwrapped until a width is set.
    fn set_style(&mut self, key: NodeKey, style: Option<&S>, scale: f32);

    /// Takes `style`, the computed style of the control `key` after a restyle that kept its box,
    /// where the text is set in properties such a restyle changes. Returns whether the text is to
    /// be laid out again.
    fn restyle(&mut self, key: NodeKey, style: &S) -> bool {
        let _ = (key, style);
        false
    }

    /// Sets the scale.
    fn set_scale(&mut self, scale: f32);

    /// Sets the width lines wrap at, in device pixels.
    fn set_width(&mut self, width: Option<f32>);

    /// Lays the text out again, where a setter changed it since it was last laid out.
    fn refresh(&mut self, cx: &mut Self::Context);

    /// Applies `edit`, leaving the text laid out.
    fn edit(&mut self, cx: &mut Self::Context, edit: Edit<'_>);

    /// Paints the text into `sink`, and its selection and caret where `focussed`, the caret in
    /// `caret_color`. `transform` takes the text's device pixels onto the scene, `scale` is
    /// device pixels per CSS pixel, and `key` is the control's node.
    #[allow(clippy::too_many_arguments)]
    fn paint<K: TextSink, H: PaintHost<S>>(
        &self,
        sink: &mut K,
        host: &H,
        scratch: &RefCell<PaintScratch>,
        focussed: bool,
        caret_color: Color,
        transform: Affine,
        scale: f64,
        key: NodeKey,
    );
}

/// Where an outside list marker is painted from: its list item's geometry.
#[derive(Clone, Copy, Debug)]
pub struct MarkerFrame {
    /// The item's content box origin, in CSS pixels.
    pub origin: Point,
    /// The item's left padding, in CSS pixels, which the marker is placed beyond.
    pub padding_left: f32,
    /// The item's left border, in CSS pixels, which the marker is placed beyond.
    pub border_left: f32,
    /// Takes the element's CSS pixels, scaled to device pixels, onto the scene.
    pub transform: Affine,
    /// Device pixels per CSS pixel.
    pub scale: f64,
    /// The list item's node.
    pub key: NodeKey,
}

/// Laying out and painting an outside list marker.
pub trait MarkerEngine<S: TextStyle>: Sized {
    /// What a document keeps for its text, which the marker is laid out with.
    type Context;

    /// The layout of the list item's own content, whose first baseline the marker sits on.
    type Lines;

    /// Lays out `marker` for the list item `key`, set in `style`, in the bullet font where
    /// `bullet`.
    fn build(
        cx: &mut Self::Context,
        key: NodeKey,
        style: &S,
        marker: &Marker,
        bullet: bool,
        scale: f32,
    ) -> Self;

    /// Paints the marker into `sink`, right-aligned before the list item's border box, on the
    /// baseline of the first line of `lines`, the item's own content.
    fn paint<K: TextSink, H: PaintHost<S>>(
        &self,
        sink: &mut K,
        host: &H,
        scratch: &RefCell<PaintScratch>,
        marker: &Marker,
        lines: Option<&Self::Lines>,
        frame: &MarkerFrame,
    );
}

/// A change to the text or the selection of an `<input>` or `<textarea>`.
///
/// Points are in device pixels relative to the text's origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edit<'a> {
    /// Replaces the selection with the text.
    Insert(&'a str),
    /// Replaces the selection with the text, which a password field shows in the clear until the
    /// next edit, where the backend masks password fields.
    InsertRevealed(&'a str),
    /// Masks the text a password field shows in the clear.
    Conceal,
    /// Deletes the selection, or the character after the caret.
    Delete,
    /// Deletes the selection, or the character before the caret.
    Backdelete,
    /// Deletes the selection, or the word after the caret.
    DeleteWord,
    /// Deletes the selection, or the word before the caret.
    BackdeleteWord,
    /// Deletes the selection.
    DeleteSelection,
    /// Moves the caret, collapsing the selection.
    Move(Motion),
    /// Moves the selection's focus, keeping its anchor.
    Extend(Motion),
    /// Selects all the text.
    SelectAll,
    /// Collapses the selection to its focus.
    CollapseSelection,
    /// Selects the bytes from the first offset to the second.
    SelectByteRange(usize, usize),
    /// Moves the caret by the first motion, then extends the selection by the second:
    /// `Select(LineStart, LineEnd)` selects the line.
    Select(Motion, Motion),
    /// Sets the text an input method is composing, and the cursor within it.
    SetCompose(&'a str, Option<(usize, usize)>),
    /// Clears the text an input method is composing.
    ClearCompose,
    /// Undoes the last change to the text, where the backend keeps a history.
    Undo,
    /// Redoes the last change undone, where the backend keeps a history.
    Redo,
    /// Moves the caret to the point.
    MoveToPoint(f32, f32),
    /// Extends the selection to the point, as a drag does.
    ExtendSelectionToPoint(f32, f32),
    /// Extends the selection to the point, as a shift-click does.
    ShiftClickExtension(f32, f32),
    /// Selects the word at the point.
    SelectWordAtPoint(f32, f32),
    /// Selects the hard line at the point.
    SelectHardLineAtPoint(f32, f32),
}

/// Where an [`Edit::Move`] or an [`Edit::Extend`] takes the caret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    /// One character to the left.
    Left,
    /// One character to the right.
    Right,
    /// One word to the left.
    WordLeft,
    /// One word to the right.
    WordRight,
    /// One line up.
    Up,
    /// One line down.
    Down,
    /// The start of the visual line.
    LineStart,
    /// The end of the visual line.
    LineEnd,
    /// The start of the line, as the text's line breaks end it.
    HardLineStart,
    /// The end of the line, as the text's line breaks end it.
    HardLineEnd,
    /// The start of the text.
    TextStart,
    /// The end of the text.
    TextEnd,
}
