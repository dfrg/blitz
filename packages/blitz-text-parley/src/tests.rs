//! The backend driven by a host of owned styles alone: building, breaking, reading back and
//! painting an inline formatting context.

use std::cell::RefCell;

use blitz_text::paint::{
    DecorationLine, Glyph, GlyphRun, InlineBoxPaint, PaintFrame, PaintHost, PaintScratch, TextSink,
};
use blitz_text::style::{
    Color, FontFamilyName, LengthPercentage, TextDecorationLine, TextDecorationStyle,
};
use blitz_text::{
    BlockInputs, DocumentText as _, InlineBuilder, InlineLayout, InlineSource, InlineText,
    LineArea, LinesInputs, Measure, NodeKey, OwnedTextStyle, SpanKind,
};
use kurbo::{Affine, Rect, Vec2};

use crate::{TextContext, TextLayout};

const ROOT: NodeKey = NodeKey(1);
const SPAN: NodeKey = NodeKey(2);
const TEXT: NodeKey = NodeKey(3);

/// A block holding one span of text, every node set in one owned style.
struct Host {
    style: OwnedTextStyle,
    text: &'static str,
}

impl InlineSource<OwnedTextStyle> for Host {
    fn block(&self, _root: NodeKey) -> Option<BlockInputs<'_, OwnedTextStyle>> {
        Some(BlockInputs {
            style: self.style.clone(),
            container: self.style.clone(),
            anonymous: false,
            first_line: None,
            language: None,
            direction: None,
            follows_box: false,
        })
    }

    fn block_style(&self, _root: NodeKey) -> Option<OwnedTextStyle> {
        Some(self.style.clone())
    }

    fn style(&self, _key: NodeKey) -> Option<OwnedTextStyle> {
        Some(self.style.clone())
    }

    fn push_content<B: InlineBuilder<OwnedTextStyle>>(&self, _root: NodeKey, builder: &mut B) {
        builder.open_box(SPAN, &self.style, SpanKind::Span, None, None);
        builder.text(TEXT, SPAN, self.text);
        builder.close_box(SpanKind::Span);
    }

    fn font_feature_values(&self) -> impl blitz_text::FontFeatureValues + '_ {}
}

impl PaintHost<OwnedTextStyle> for Host {
    fn style(&self, _key: NodeKey) -> Option<OwnedTextStyle> {
        Some(self.style.clone())
    }

    fn parent(&self, key: NodeKey) -> Option<NodeKey> {
        match key {
            TEXT => Some(SPAN),
            SPAN => Some(ROOT),
            _ => None,
        }
    }

    fn element(&self, key: NodeKey) -> Option<NodeKey> {
        Some(if key == TEXT { SPAN } else { key })
    }

    fn relative_shift(&self, _key: NodeKey, _containing: kurbo::Size, _rtl: bool) -> Vec2 {
        Vec2::ZERO
    }

    fn propagated_decorations(&self, _root: NodeKey) -> Vec<OwnedTextStyle> {
        Vec::new()
    }
}

/// What a backend painted.
#[derive(Default)]
struct Recorded {
    glyphs: usize,
    decorations: Vec<DecorationLine>,
    selections: usize,
}

impl TextSink for Recorded {
    fn glyphs(&mut self, _run: &GlyphRun<'_>, glyphs: impl Iterator<Item = Glyph> + Clone) {
        self.glyphs += glyphs.count();
    }
    fn fill(&mut self, _transform: Affine, _rect: Rect, _color: Color) {}
    fn selection(&mut self, _transform: Affine, _rect: Rect) {
        self.selections += 1;
    }
    fn decoration(&mut self, line: &DecorationLine) {
        self.decorations.push(*line);
    }
    fn inline_box(&mut self, _inline_box: &InlineBoxPaint) {}
    fn push_blur(&mut self, _transform: Affine, _clip: Rect, _deviation: f32) {}
    fn pop_blur(&mut self) {}
    fn atomic(&mut self, _key: NodeKey) {}
}

/// Builds and lays out `host`'s block `width` device pixels wide.
fn lay_out(cx: &mut TextContext, host: &Host, width: f32) -> TextLayout {
    let mut layouts = vec![(ROOT, Box::default())];
    <TextLayout as InlineLayout<OwnedTextStyle>>::build_layouts(cx, host, 1.0, &mut layouts);
    let mut layout = layouts.pop().unwrap().1;
    let lines = LinesInputs {
        sizes: &[],
        pass: Measure::Layout,
        basis: width,
        vertical: false,
    };
    InlineLayout::<OwnedTextStyle>::prepare(&mut *layout, cx, host, ROOT, 1.0, lines);
    let area = LineArea {
        width,
        room_above: 0.0,
        block_end: None,
        sizes_only: false,
        pass: 0,
    };
    let style = host.style.clone();
    InlineLayout::<OwnedTextStyle>::break_lines(
        &mut *layout,
        cx,
        area,
        Some(&style),
        &mut blitz_text::NoExclusions { width },
    );
    *layout
}

/// Text set in the bundled bullet font, which needs no fonts of the platform's.
fn bullets(text: &'static str) -> Host {
    Host {
        style: OwnedTextStyle {
            font_size: 20.0,
            line_height: blitz_text::style::LineHeight::Length(20.0),
            ..OwnedTextStyle::with_families([FontFamilyName::named("Bullet")])
        },
        text,
    }
}

#[test]
fn builds_breaks_and_reads_back_owned_styles() {
    let mut cx = TextContext::new(None);
    let host = bullets("\u{2022} \u{2022} \u{2022} \u{2022}");
    let wide = lay_out(&mut cx, &host, 1000.0);
    assert_eq!(wide.text(), host.text);
    let extent = InlineLayout::<OwnedTextStyle>::extent(&wide, 0.0);
    assert_eq!(extent.height, 20.0);
    // Parley keys each run by the element it is in.
    assert!(
        wide.logical_content()
            .all(|item| matches!(item, blitz_text::ContentItem::Text { key: SPAN, .. }))
    );
    let hit = wide.hit_test(1.0, 10.0, false).unwrap();
    assert_eq!((hit.key, hit.offset), (SPAN, 0));
    // Too narrow for two bullets, the text breaks at each space.
    let narrow = lay_out(&mut cx, &host, 25.0);
    assert_eq!(
        InlineLayout::<OwnedTextStyle>::extent(&narrow, 0.0).height,
        80.0
    );
}

#[test]
fn paints_glyphs_selection_and_decorations() {
    let mut cx = TextContext::new(None);
    let mut host = bullets("\u{2022}\u{2022}");
    host.style.text_decoration_line = TextDecorationLine::UNDERLINE;
    host.style.text_decoration_style = TextDecorationStyle::Dashed;
    host.style.letter_spacing = LengthPercentage::Length(1.0);
    let layout = lay_out(&mut cx, &host, 1000.0);
    let mut sink = Recorded::default();
    let scratch = RefCell::new(PaintScratch::default());
    InlineLayout::<OwnedTextStyle>::paint(
        &layout,
        &mut sink,
        &host,
        &scratch,
        &PaintFrame {
            transform: Affine::IDENTITY,
            content_size: kurbo::Size::new(1000.0, 20.0),
            scale: 1.0,
            root: ROOT,
            selection: Some((0, 3)),
        },
    );
    assert_eq!(sink.glyphs, 2);
    assert!(sink.selections > 0);
    // The root and the span both set the underline, so each draws one.
    assert_eq!(sink.decorations.len(), 2);
    assert!(
        sink.decorations
            .iter()
            .all(|line| line.style == TextDecorationStyle::Dashed)
    );
}
