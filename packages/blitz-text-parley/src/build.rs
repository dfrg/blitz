//! Building an inline formatting context's Parley layout from what the host pushes, and laying
//! out outside list markers.

use blitz_text::style::Clear;
use blitz_text::{
    FloatSide, InlineBuilder, InlineSource, Marker, MarkerEngine, MarkerFrame, NodeKey, PaintHost,
    PaintScratch, SpanKind, TextSink, TextStyle,
};
use parley::{
    FontContext, FontFamily, InlineBox, LayoutContext, StyleProperty, TreeBuilder,
    WhiteSpaceCollapse,
};

use crate::{MarkerLayout, TextBrush, TextContext, TextLayout, style as css_to_parley};

/// The families a list marker's bullet is set in.
const BULLET_FONT_FAMILY: &str = "Bullet, monospace, sans-serif";

/// Builds the Parley layout of the inline formatting context rooted at `root`.
pub(crate) fn build_inline_layout_into<S: TextStyle, C: InlineSource<S>>(
    source: &C,
    layout_ctx: &mut LayoutContext<TextBrush>,
    font_ctx: &mut FontContext,
    text_layout: &mut TextLayout,
    scale: f32,
    root: NodeKey,
) {
    // The style of the inline formatting context's root, or for an anonymous block, of the box
    // around it.
    let root_style = source.block_style(root);

    let parley_style = root_style
        .as_ref()
        .map(|style| css_to_parley::style(root, style))
        .unwrap_or_else(|| parley::TextStyle {
            white_space_collapse: WhiteSpaceCollapse::Collapse,
            ..Default::default()
        });

    let mut tree = layout_ctx.tree_builder(font_ctx, scale, true, &parley_style);
    if let Some(style) = &root_style {
        tree.set_base_direction(css_to_parley::base_direction(style));
    }
    drop(root_style);

    let mut builder = ParleyBuilder {
        tree,
        root_style: &parley_style,
    };
    source.push_content(root, &mut builder);
    text_layout.text = builder.tree.build_into(&mut text_layout.layout);
}

/// A Parley tree builder, as the host pushes into it.
struct ParleyBuilder<'a, 'b> {
    tree: TreeBuilder<'b, TextBrush>,
    /// The style of the inline formatting context's root, which an inside list marker is set in.
    root_style: &'a parley::TextStyle<'static, 'static, TextBrush>,
}

impl<S: TextStyle> InlineBuilder<S> for ParleyBuilder<'_, '_> {
    const TRANSFORMS_TEXT: bool = false;
    const SETS_RUBY: bool = false;
    const READS_FIRST_LINE: bool = false;
    const READS_LANG: bool = false;

    #[inline]
    fn pushed_text(&self) -> &str {
        self.tree.text()
    }

    #[inline]
    fn has_pending_whitespace(&self) -> bool {
        self.tree.has_pending_whitespace()
    }

    fn marker(&mut self, marker: &Marker) {
        match marker {
            // Bullet glyphs live in the bundled bullet font. An outside marker asks for it; without
            // the same span here a marker like disclosure-closed (U+25B8) falls back to the
            // element's own font and draws as a missing glyph.
            Marker::Char(char) => {
                let mut marker_style = self.root_style.clone();
                marker_style.font_family = BULLET_FONT_FAMILY.into();
                self.tree.push_style_span(marker_style);
                self.tree.push_text(&format!("{char} "));
                self.tree.pop_style_span();
            }
            Marker::String(str) => self.tree.push_text(str),
        }
    }

    fn open_box(
        &mut self,
        key: NodeKey,
        style: &S,
        kind: SpanKind,
        _first_line: Option<&S>,
        _lang: Option<&str>,
    ) {
        match kind {
            // A `display: contents` element keeps only its white space handling.
            SpanKind::Contents => self
                .tree
                .push_style_modification_span(&css_to_parley::contents_modifications(style)),
            _ => self.tree.push_style_span(css_to_parley::style(key, style)),
        }
    }

    #[inline]
    fn close_box(&mut self, _kind: SpanKind) {
        self.tree.pop_style_span();
    }

    #[inline]
    fn text(&mut self, _key: NodeKey, _parent: NodeKey, text: &str) {
        self.tree.push_text(text);
    }

    fn atomic(&mut self, key: NodeKey, style: &S, _lang: Option<&str>) {
        self.inline_box(key, style, parley::InlineBoxKind::InFlow);
    }

    fn float(&mut self, key: NodeKey, style: &S, _side: FloatSide, _lang: Option<&str>) {
        self.inline_box(key, style, parley::InlineBoxKind::CustomOutOfFlow);
    }

    fn absolute(&mut self, key: NodeKey, style: &S, _inline_level: bool) {
        self.inline_box(key, style, parley::InlineBoxKind::OutOfFlow);
    }

    fn line_break(&mut self, _key: NodeKey, _style: &S, _clear: Option<Clear>) {
        self.tree
            .push_style_modification_span(&[StyleProperty::WhiteSpaceCollapse(
                WhiteSpaceCollapse::Preserve,
            )]);
        self.tree.push_text("\n");
        self.tree.pop_style_span();
    }

    #[inline]
    fn break_opportunity(&mut self, _key: NodeKey) -> bool {
        false
    }
}

impl ParleyBuilder<'_, '_> {
    /// Pushes an inline box, which the lines size and place as `kind` says.
    fn inline_box<S: TextStyle>(&mut self, key: NodeKey, style: &S, kind: parley::InlineBoxKind) {
        self.tree.push_inline_box(InlineBox {
            id: key.0,
            kind,
            // Set by `push_inline_box`
            index: 0,
            // Width and height are set during layout
            width: 0.0,
            height: 0.0,
            baseline: None,
            vertical_align: css_to_parley::vertical_align(style),
        });
    }
}

impl<S: TextStyle> MarkerEngine<S> for MarkerLayout {
    type Context = TextContext;
    type Lines = TextLayout;

    fn build(
        cx: &mut TextContext,
        key: NodeKey,
        style: &S,
        marker: &Marker,
        bullet: bool,
        scale: f32,
    ) -> Self {
        let mut parley_style = css_to_parley::style(key, style);

        // Bullets are set in the bullet font.
        if bullet {
            parley_style.font_family = FontFamily::from(BULLET_FONT_FAMILY);
        }

        let mut font_ctx = cx.font_ctx.lock().unwrap();
        let mut builder = cx
            .layout_ctx
            .tree_builder(&mut font_ctx, scale, true, &parley_style);

        match marker {
            Marker::Char(char) => {
                let mut buf = [0u8; 4];
                builder.push_text(char.encode_utf8(&mut buf));
            }
            Marker::String(str) => builder.push_text(str),
        };

        let mut layout = builder.build().0;
        let width = layout.calculate_content_widths().max;
        layout.break_all_lines(Some(width));
        Self { layout }
    }

    fn paint<K: TextSink, H: PaintHost<S>>(
        &self,
        sink: &mut K,
        host: &H,
        scratch: &std::cell::RefCell<PaintScratch>,
        marker: &Marker,
        lines: Option<&TextLayout>,
        frame: &MarkerFrame,
    ) {
        crate::paint::paint_marker(sink, marker, self, lines, host, scratch, frame);
    }
}
