//! The text of an `<input>` or `<textarea>`, edited and laid out by Parley's `PlainEditor`.

use std::borrow::Cow;
use std::ops::Range;

use blitz_text::{Edit, EditEngine, EditableText, EditorMetrics, Motion, NodeKey, TextStyle};
use kurbo::{Rect, Size};
use parley::{PlainEditor, PlainEditorDriver, StyleProperty};

use crate::{TextBrush, TextContext, style as css_to_parley};

/// The text of an `<input>` or `<textarea>`, edited and laid out by Parley.
pub struct TextEditor {
    editor: PlainEditor<TextBrush>,
    /// The width lines wrap at, in device pixels, as last set.
    width: Option<f32>,
}

impl TextEditor {
    /// Returns Parley's editor, which the renderer paints the text, the selection and the caret
    /// of.
    #[inline]
    pub fn plain_editor(&self) -> &PlainEditor<TextBrush> {
        &self.editor
    }

    /// Returns Parley's editor, to change directly.
    #[inline]
    pub fn plain_editor_mut(&mut self) -> &mut PlainEditor<TextBrush> {
        &mut self.editor
    }
}

impl EditableText for TextEditor {
    #[inline]
    fn raw_text(&self) -> &str {
        self.editor.raw_text()
    }

    fn text(&self) -> Cow<'_, str> {
        if self.editor.is_composing() {
            Cow::Owned(self.editor.text().to_string())
        } else {
            Cow::Borrowed(self.editor.raw_text())
        }
    }

    #[inline]
    fn selection(&self) -> Range<usize> {
        self.editor.raw_selection().text_range()
    }

    #[inline]
    fn selected_text(&self) -> Option<&str> {
        self.editor.selected_text()
    }

    #[inline]
    fn is_selection_collapsed(&self) -> bool {
        self.editor.raw_selection().is_collapsed()
    }

    fn revealed_range(&self) -> Option<Range<usize>> {
        // Parley draws a password field's text as it is.
        None
    }

    fn metrics(&self) -> Option<EditorMetrics> {
        let layout = self.editor.try_layout()?;
        Some(EditorMetrics {
            size: Size::new(f64::from(layout.full_width()), f64::from(layout.height())),
            scale: layout.scale(),
        })
    }

    fn caret_rect(&self) -> Option<Rect> {
        self.editor.try_layout()?;
        let caret = self.editor.cursor_geometry(1.5)?;
        Some(Rect::new(caret.x0, caret.y0, caret.x1, caret.y1))
    }
}

impl<S: TextStyle> EditEngine<S> for TextEditor {
    type Context = TextContext;

    fn new(_is_multiline: bool) -> Self {
        Self {
            editor: PlainEditor::new(16.0),
            width: None,
        }
    }

    fn set_initial_text(&mut self, text: &str) {
        self.editor.set_text(text);
    }

    fn set_text(&mut self, text: &str) {
        if self.editor.text() != text {
            self.editor.set_text(text);
        }
    }

    fn set_style(&mut self, key: NodeKey, style: Option<&S>, scale: f32) {
        let parley_style = style
            .map(|s| css_to_parley::style(key, s))
            .unwrap_or_default();
        let alignment = style
            .map(|s| css_to_parley::text_align(s.text_align()))
            .unwrap_or(parley::Alignment::Start);
        let base_direction = style
            .map(css_to_parley::base_direction)
            .unwrap_or(parley::BaseDirection::Auto);

        let editor = &mut self.editor;
        editor.set_scale(scale);
        editor.set_width(None);
        self.width = None;

        let styles = editor.edit_styles();
        styles.retain(|_| false);
        styles.insert(StyleProperty::FontFamily(parley_style.font_family));
        styles.insert(StyleProperty::FontSize(parley_style.font_size));
        styles.insert(StyleProperty::FontWidth(parley_style.font_width));
        styles.insert(StyleProperty::FontStyle(parley_style.font_style));
        styles.insert(StyleProperty::FontWeight(parley_style.font_weight));
        styles.insert(StyleProperty::FontVariations(parley_style.font_variations));
        styles.insert(StyleProperty::FontFeatures(parley_style.font_features));
        styles.insert(StyleProperty::LineHeight(parley_style.line_height));
        styles.insert(StyleProperty::WordSpacing(parley_style.word_spacing));
        styles.insert(StyleProperty::LetterSpacing(parley_style.letter_spacing));
        styles.insert(StyleProperty::WordBreak(parley_style.word_break));
        styles.insert(StyleProperty::OverflowWrap(parley_style.overflow_wrap));
        styles.insert(StyleProperty::TextWrapMode(parley_style.text_wrap_mode));
        styles.insert(StyleProperty::WhiteSpaceCollapse(
            parley_style.white_space_collapse,
        ));
        styles.insert(StyleProperty::Brush(parley_style.brush));
        editor.set_alignment(alignment);
        editor.set_base_direction(base_direction);
    }

    fn set_scale(&mut self, scale: f32) {
        if self.editor.get_scale() != scale {
            self.editor.set_scale(scale);
        }
    }

    fn set_width(&mut self, width: Option<f32>) {
        if self.width != width {
            self.width = width;
            self.editor.set_width(width);
        }
    }

    fn refresh(&mut self, cx: &mut TextContext) {
        if self.editor.try_layout().is_none() {
            self.editor
                .refresh_layout(&mut cx.font_ctx.lock().unwrap(), &mut cx.layout_ctx);
        }
    }

    fn edit(&mut self, cx: &mut TextContext, edit: Edit<'_>) {
        let mut font_ctx = cx.font_ctx.lock().unwrap();
        let mut driver = self.editor.driver(&mut font_ctx, &mut cx.layout_ctx);
        driver.refresh_layout();
        match edit {
            // Parley masks nothing, so there is nothing to show in the clear.
            Edit::Insert(text) | Edit::InsertRevealed(text) => {
                driver.insert_or_replace_selection(text)
            }
            Edit::Conceal => {}
            Edit::Delete => driver.delete(),
            Edit::Backdelete => driver.backdelete(),
            Edit::DeleteWord => driver.delete_word(),
            Edit::BackdeleteWord => driver.backdelete_word(),
            Edit::DeleteSelection => driver.delete_selection(),
            Edit::Move(motion) => move_by(&mut driver, motion),
            Edit::Extend(motion) => extend_by(&mut driver, motion),
            Edit::SelectAll => driver.select_all(),
            Edit::CollapseSelection => driver.collapse_selection(),
            Edit::SelectByteRange(start, end) => driver.select_byte_range(start, end),
            Edit::Select(from, to) => {
                move_by(&mut driver, from);
                extend_by(&mut driver, to);
            }
            Edit::SetCompose(text, cursor) => driver.set_compose(text, cursor),
            Edit::ClearCompose => driver.clear_compose(),
            // PlainEditor keeps no history.
            Edit::Undo | Edit::Redo => {}
            Edit::MoveToPoint(x, y) => driver.move_to_point(x, y),
            Edit::ExtendSelectionToPoint(x, y) => driver.extend_selection_to_point(x, y),
            Edit::ShiftClickExtension(x, y) => driver.shift_click_extension(x, y),
            Edit::SelectWordAtPoint(x, y) => driver.select_word_at_point(x, y),
            Edit::SelectHardLineAtPoint(x, y) => driver.select_hard_line_at_point(x, y),
        }
        driver.refresh_layout();
    }
}

/// Moves the caret by `motion`, collapsing the selection.
fn move_by(driver: &mut PlainEditorDriver<'_, TextBrush>, motion: Motion) {
    match motion {
        Motion::Left => driver.move_left(),
        Motion::Right => driver.move_right(),
        Motion::WordLeft => driver.move_word_left(),
        Motion::WordRight => driver.move_word_right(),
        Motion::Up => driver.move_up(),
        Motion::Down => driver.move_down(),
        Motion::LineStart => driver.move_to_line_start(),
        Motion::LineEnd => driver.move_to_line_end(),
        Motion::HardLineStart => driver.move_to_hard_line_start(),
        Motion::HardLineEnd => driver.move_to_hard_line_end(),
        Motion::TextStart => driver.move_to_text_start(),
        Motion::TextEnd => driver.move_to_text_end(),
    }
}

/// Moves the selection's focus by `motion`, keeping its anchor.
fn extend_by(driver: &mut PlainEditorDriver<'_, TextBrush>, motion: Motion) {
    match motion {
        Motion::Left => driver.select_left(),
        Motion::Right => driver.select_right(),
        Motion::WordLeft => driver.select_word_left(),
        Motion::WordRight => driver.select_word_right(),
        Motion::Up => driver.select_up(),
        Motion::Down => driver.select_down(),
        Motion::LineStart => driver.select_to_line_start(),
        Motion::LineEnd => driver.select_to_line_end(),
        Motion::HardLineStart => driver.select_to_hard_line_start(),
        Motion::HardLineEnd => driver.select_to_hard_line_end(),
        Motion::TextStart => driver.select_to_text_start(),
        Motion::TextEnd => driver.select_to_text_end(),
    }
}

#[cfg(test)]
mod tests {
    use blitz_text::{
        DocumentText as _, Edit, EditEngine, EditableText as _, Motion, NodeKey, OwnedTextStyle,
    };

    use super::TextEditor;
    use crate::{FontContext, TextContext};

    fn context() -> TextContext {
        TextContext::new(Some(FontContext::new()))
    }

    /// The setters lay nothing out: the next edit or refresh does, once, and the caret follows
    /// what was set.
    #[test]
    fn setters_lay_out_on_the_next_edit_or_refresh() {
        let mut cx = context();
        let mut editor = <TextEditor as EditEngine<OwnedTextStyle>>::new(false);
        EditEngine::<OwnedTextStyle>::set_style(&mut editor, NodeKey(1), None, 1.0);
        EditEngine::<OwnedTextStyle>::set_text(&mut editor, "hello");
        assert_eq!(editor.metrics(), None);
        assert_eq!(editor.caret_rect(), None);

        EditEngine::<OwnedTextStyle>::edit(&mut editor, &mut cx, Edit::Move(Motion::TextEnd));
        let metrics = editor.metrics().unwrap();
        assert_eq!(metrics.scale, 1.0);
        let caret = editor.caret_rect().unwrap();
        assert!(
            (caret.x0 - metrics.size.width).abs() < 2.0,
            "{caret:?} {metrics:?}"
        );

        EditEngine::<OwnedTextStyle>::set_scale(&mut editor, 2.0);
        assert_eq!(editor.metrics(), None);
        EditEngine::<OwnedTextStyle>::refresh(&mut editor, &mut cx);
        let scaled = editor.metrics().unwrap();
        assert_eq!(scaled.scale, 2.0);
        assert!(
            scaled.size.width > metrics.size.width * 1.5,
            "{scaled:?} {metrics:?}"
        );
    }

    /// Text set in an owned style lays out, and moves its caret by words.
    #[test]
    fn edits_text_set_in_an_owned_style() {
        let mut cx = context();
        let style = OwnedTextStyle::default();
        let mut editor = <TextEditor as EditEngine<OwnedTextStyle>>::new(false);
        editor.set_style(NodeKey(1), Some(&style), 1.0);
        EditEngine::<OwnedTextStyle>::set_initial_text(&mut editor, "one two");
        EditEngine::<OwnedTextStyle>::edit(&mut editor, &mut cx, Edit::Move(Motion::TextStart));
        EditEngine::<OwnedTextStyle>::edit(&mut editor, &mut cx, Edit::Extend(Motion::WordRight));
        assert!(!editor.is_selection_collapsed());
        EditEngine::<OwnedTextStyle>::edit(&mut editor, &mut cx, Edit::Insert("1"));
        assert!(editor.raw_text().ends_with("two"));
        assert!(editor.raw_text().starts_with('1'));
    }
}
