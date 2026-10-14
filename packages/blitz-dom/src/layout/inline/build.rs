//! Walking the DOM of an inline formatting context into a text backend's builder: the host's side
//! of a build, [`DomSource`].

use blitz_text::style::Clear;
use blitz_text::{BlockInputs, FeatureBlock, FontFeatureValues, InlineSource};
use markup5ever::{local_name, ns};
use style::Atom;
use style::context::{CascadeInputs, TreeCountingCaches};
use style::properties::{ComputedValues, FirstLineReparenting};
use style::rule_cache::RuleCacheConditions;
use style::selector_parser::PseudoElement;
use style::servo_arc::Arc as ServoArc;
use style::shared_lock::{SharedRwLock, StylesheetGuards};
use style::stylesheets::FontFeatureValuesRule;
use style::stylesheets::font_feature_values_rule::{FFVDeclaration, PairValues, SingleValue};
use style::stylist::Stylist;
use style::values::computed::{Display, Float};
use style::values::specified::box_::{DisplayInside, DisplayOutside};
use style::values::specified::position::PositionTryFallbacksTryTactic;

use crate::layout::replaced::is_inline_box_element;
use crate::layout::text_transform::{CaseTransform, PushedText, TextTransformer};
use crate::node::{ListItemLayout, ListItemLayoutPosition};
use crate::text::{
    DocumentCascade, FloatSide, InlineBuilder, NodeKey, SpanKind, StyloStyle, key_node, node_key,
};
use crate::{Node, NodeData, NodeTree};

/// A document's inline formatting contexts, as a text backend builds them.
#[derive(Clone, Copy)]
pub(crate) struct DomSource<'a> {
    pub(crate) nodes: &'a NodeTree,
    pub(crate) stylist: &'a Stylist,
    /// The lock the document's stylesheets are read under, which `::first-line` and
    /// `::first-letter` are cascaded with.
    pub(crate) lock: &'a SharedRwLock,
}

impl InlineSource<StyloStyle> for DomSource<'_> {
    fn block(&self, root: NodeKey) -> Option<BlockInputs<'_, StyloStyle>> {
        let nodes = self.nodes;
        let root_id = key_node(root);
        let root = &nodes[root_id];
        let style = root_style(nodes, root)?;
        let anonymous = matches!(root.data, NodeData::AnonymousBlock(_));
        let container = if anonymous {
            root.parent
                .and_then(|parent| nodes[parent].primary_styles().map(|style| (*style).clone()))
                .unwrap_or_else(|| style.clone())
        } else {
            style.clone()
        };
        let first_line = root
            .try_stylo_element_data()
            .and_then(|data| data.get())
            .and_then(|data| data.styles.pseudos.get(&PseudoElement::FirstLine).cloned());
        // The root element's lines run in the body's direction, which CSS Writing Modes
        // propagates to the root as its used value.
        let element = if anonymous {
            root.parent
        } else {
            Some(root_id)
        };
        let direction = element
            .and_then(|element| body_is_rtl(nodes, element))
            .map(|rtl| {
                if rtl {
                    blitz_text::style::Direction::Rtl
                } else {
                    blitz_text::style::Direction::Ltr
                }
            });
        Some(BlockInputs {
            style: StyloStyle(style),
            container: StyloStyle(container),
            anonymous,
            first_line: first_line.map(StyloStyle),
            language: language_of(nodes, Some(root_id)),
            direction,
            follows_box: anonymous && follows_a_box(nodes, root),
        })
    }

    fn block_style(&self, root: NodeKey) -> Option<StyloStyle> {
        root_style(self.nodes, &self.nodes[key_node(root)]).map(StyloStyle)
    }

    fn style(&self, key: NodeKey) -> Option<StyloStyle> {
        self.nodes
            .get(key_node(key))?
            .primary_styles()
            .map(|style| StyloStyle((*style).clone()))
    }

    fn push_content<B: InlineBuilder<StyloStyle>>(&self, root: NodeKey, builder: &mut B) {
        push_inline_content(self, &self.nodes[key_node(root)], builder);
    }

    fn font_feature_values(&self) -> impl FontFeatureValues + '_ {
        StyloFeatureValues::of(self.stylist)
    }
}

/// The computed style the content of the inline formatting context rooted at `root` is built in.
///
/// An anonymous block may have no style of its own: its text is set in the box around it.
pub(crate) fn root_style(nodes: &NodeTree, root: &Node) -> Option<ServoArc<ComputedValues>> {
    root.primary_styles()
        .map(|style| (*style).clone())
        .or_else(|| {
            root.parent
                .and_then(|parent| nodes[parent].primary_styles().map(|style| (*style).clone()))
        })
}

/// The `lang` of `node`: its nearest `lang`, its own or an ancestor's, as written.
fn language_of(nodes: &NodeTree, mut node: Option<crate::NodeId>) -> Option<&str> {
    while let Some(id) = node {
        let held = &nodes[id];
        if let Some(language) = own_language(held) {
            return Some(language);
        }
        node = held.parent;
    }
    None
}

/// `node`'s own `lang` attribute, where it has one.
fn own_language(node: &Node) -> Option<&str> {
    let element = node.element_data()?;
    let lang = element.attrs.iter().find(|attr| {
        attr.name.local == local_name!("lang")
            && (attr.name.ns == ns!() || attr.name.ns == ns!(xml))
    })?;
    Some(&lang.value)
}

/// Whether the body is right-to-left, where `element` is the root element, has a body, and
/// neither is contained; `None` otherwise.
fn body_is_rtl(nodes: &NodeTree, element: crate::NodeId) -> Option<bool> {
    let html = &nodes[element];
    let parent = html.parent?;
    if !matches!(nodes[parent].data, NodeData::Document(_))
        || html
            .data
            .downcast_element()
            .is_none_or(|data| data.name.local != local_name!("html"))
    {
        return None;
    }
    let body = html
        .children
        .iter()
        .map(|&child| &nodes[child])
        .find(|child| {
            child
                .data
                .downcast_element()
                .is_some_and(|data| data.name.local == local_name!("body"))
        })?;
    // Containment on either keeps the body's direction its own.
    let contained = |node: &Node| {
        node.primary_styles()
            .is_some_and(|computed| !computed.slow_clone_contain().is_empty())
    };
    if contained(html) || contained(body) {
        return None;
    }
    let computed = body.primary_styles()?;
    Some(computed.slow_clone_direction() == style::computed_values::direction::T::Rtl)
}

/// Whether `node` comes after another box among its parent's layout children, where the parent
/// is a block container: one in the flow, not floated or absolutely positioned. A flex or grid
/// item is a block of its own, with a first line of its own.
fn follows_a_box(nodes: &NodeTree, node: &Node) -> bool {
    let Some(parent) = node.parent else {
        return false;
    };
    let in_block_container = nodes[parent].display_style().is_some_and(|display| {
        matches!(
            display.inside(),
            DisplayInside::Flow | DisplayInside::FlowRoot
        )
    });
    if !in_block_container {
        return false;
    }
    let children = nodes[parent].layout_children.borrow();
    let Some(children) = children.as_ref() else {
        return false;
    };
    children
        .iter()
        .take_while(|&&child| child != node.id)
        .any(|&child| {
            nodes[child].primary_styles().is_none_or(|computed| {
                !computed.slow_clone_position().is_absolutely_positioned()
                    && computed.slow_clone_float() == Float::None
            })
        })
}

/// The floats a `<br>` clears: its `clear`, or its `clear` attribute, which Chrome maps to the
/// property: `left`, `right`, and `all` or `both`.
fn br_clear(computed: &ComputedValues, element: &crate::node::ElementData) -> Option<Clear> {
    use style::values::computed::Clear as StyloClear;
    let rtl = computed.slow_clone_direction() == style::computed_values::direction::T::Rtl;
    match computed.slow_clone_clear() {
        StyloClear::Left | StyloClear::InlineStart if !rtl => return Some(Clear::Left),
        StyloClear::Right | StyloClear::InlineEnd if !rtl => return Some(Clear::Right),
        StyloClear::Left | StyloClear::InlineEnd => return Some(Clear::Left),
        StyloClear::Right | StyloClear::InlineStart => return Some(Clear::Right),
        StyloClear::Both => return Some(Clear::Both),
        StyloClear::None => {}
    }
    let clear = element.attr(local_name!("clear"))?.trim();
    if clear.eq_ignore_ascii_case("left") {
        Some(Clear::Left)
    } else if clear.eq_ignore_ascii_case("right") {
        Some(Clear::Right)
    } else if clear.eq_ignore_ascii_case("all") || clear.eq_ignore_ascii_case("both") {
        Some(Clear::Both)
    } else {
        None
    }
}

impl DocumentCascade<'_> {
    /// `style`, the computed style of `node`, with its inherited properties taken from `parent`
    /// instead of its parent's style.
    ///
    /// This is what an element on the first formatted line is set in: it inherits from its
    /// parent's `::first-line` style, and keeps its own reset properties.
    fn reparent(
        self,
        node: &Node,
        style: &ComputedValues,
        parent: &ComputedValues,
    ) -> ServoArc<ComputedValues> {
        self.stylist.cascade_style_and_visited(
            Some(node),
            None,
            &CascadeInputs::new_from_style(style),
            self.guards,
            Some(parent),
            Some(parent),
            FirstLineReparenting::Yes {
                style_to_reparent: style,
            },
            &PositionTryFallbacksTryTactic::default(),
            None,
            &mut RuleCacheConditions::default(),
            &mut TreeCountingCaches::default(),
        )
    }

    /// The block `block`'s `::first-letter` style, `letter`, cascaded again to inherit from
    /// `parent`, the style of the box the letter is in.
    ///
    /// Stylo cascades the pseudo-element against the block, where CSS has it inherit from the
    /// innermost box its text is in.
    fn first_letter(
        self,
        block: &Node,
        letter: &ComputedValues,
        parent: &ComputedValues,
    ) -> ServoArc<ComputedValues> {
        self.stylist.cascade_style_and_visited(
            Some(block),
            Some(&PseudoElement::FirstLetter),
            &CascadeInputs::new_from_style(letter),
            self.guards,
            Some(parent),
            Some(parent),
            FirstLineReparenting::No,
            &PositionTryFallbacksTryTactic::default(),
            None,
            &mut RuleCacheConditions::default(),
            &mut TreeCountingCaches::default(),
        )
    }
}

/// The document's `@font-feature-values` rules, in cascade order: by origin, then by layer, the
/// least precedent first.
#[derive(Default)]
pub(crate) struct StyloFeatureValues(Vec<ServoArc<FontFeatureValuesRule>>);

impl StyloFeatureValues {
    /// The rules in effect in `stylist`.
    pub(crate) fn of(stylist: &Stylist) -> Self {
        Self(
            stylist
                .iter_extra_data_origins_rev()
                .flat_map(|(data, _)| data.font_feature_values.iter())
                .map(|(rule, _)| rule.clone())
                .collect(),
        )
    }
}

/// Whether `rule` names the family `family`.
fn names_family(rule: &FontFeatureValuesRule, family: &str) -> bool {
    rule.family_names
        .iter()
        .any(|name| name.name.as_str().eq_ignore_ascii_case(family))
}

impl FontFeatureValues for StyloFeatureValues {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn names_family(&self, family: &str) -> bool {
        self.0.iter().any(|rule| names_family(rule, family))
    }

    fn find(&self, family: &str, block: FeatureBlock, name: &str) -> Option<Vec<u32>> {
        let name = Atom::from(name);
        let single = |declarations: &[FFVDeclaration<SingleValue>]| {
            declarations
                .iter()
                .find(|declaration| declaration.name == name)
                .map(|declaration| vec![declaration.value.0])
        };
        // A later rule outweighs an earlier one, name by name.
        self.0
            .iter()
            .rev()
            .filter(|rule| names_family(rule, family))
            .find_map(|rule| match block {
                FeatureBlock::Stylistic => single(&rule.stylistic),
                FeatureBlock::Swash => single(&rule.swash),
                FeatureBlock::Ornaments => single(&rule.ornaments),
                FeatureBlock::Annotation => single(&rule.annotation),
                FeatureBlock::Styleset => rule
                    .styleset
                    .iter()
                    .find(|declaration| declaration.name == name)
                    .map(|declaration| declaration.value.0.clone()),
                FeatureBlock::CharacterVariant => rule
                    .character_variant
                    .iter()
                    .find(|declaration| declaration.name == name)
                    .map(|declaration| {
                        let PairValues(first, second) = declaration.value;
                        std::iter::once(first).chain(second).collect()
                    }),
            })
    }
}

/// A builder's pushed text, as `capitalize` reads it.
struct Pushed<'a, B>(&'a B);

impl<B: InlineBuilder<StyloStyle>> PushedText for Pushed<'_, B> {
    #[inline]
    fn text(&self) -> &str {
        self.0.pushed_text()
    }

    #[inline]
    fn has_pending_whitespace(&self) -> bool {
        InlineBuilder::has_pending_whitespace(self.0)
    }
}

/// Pushes the content of the inline formatting context rooted at `root` into `builder`: the
/// marker of a list item whose marker is inside it, then its `::before`, its children and its
/// `::after`, in tree order.
pub(crate) fn push_inline_content<B: InlineBuilder<StyloStyle>>(
    source: &DomSource<'_>,
    root: &Node,
    builder: &mut B,
) {
    let nodes = source.nodes;
    // An anonymous block has no style of its own: its text is set in the box around it.
    let root_style = root_style(nodes, root);
    let text_transform = case_transform::<B>(root_style.as_deref());

    if let Some(ListItemLayout {
        marker,
        position: ListItemLayoutPosition::Inside,
    }) = root
        .element_data()
        .and_then(|element| element.list_item_data.as_deref())
    {
        builder.marker(marker);
    }

    // The block's `::first-line`, which the first line's elements inherit from, and its
    // `::first-letter`, where the backend reads them.
    let pseudo = |pseudo: &PseudoElement| {
        root.try_stylo_element_data()
            .and_then(|data| data.get())
            .and_then(|data| data.styles.pseudos.get(pseudo).cloned())
    };
    let first_line = B::READS_FIRST_LINE
        .then(|| pseudo(&PseudoElement::FirstLine))
        .flatten();
    let first_letter = builder
        .wants_first_letter()
        .then(|| pseudo(&PseudoElement::FirstLetter))
        .flatten();
    let guard = (first_line.is_some() || first_letter.is_some()).then(|| source.lock.read());
    let guards = guard.as_ref().map(StylesheetGuards::same);
    let cascade = guards.as_ref().map(|guards| DocumentCascade {
        stylist: source.stylist,
        guards,
    });
    // `inline-start` and `inline-end` floats go to the containing block's sides, as its
    // direction has them.
    let rtl = root.primary_styles().is_some_and(|block| {
        block.get_inherited_box().direction == style::computed_values::direction::T::Rtl
    });

    let mut walk = Walk {
        nodes,
        root,
        // The marker is a box of its own, so the content's words do not continue from it.
        transformer: TextTransformer::default(),
        cascade,
        parents: Vec::new(),
        first_letter,
        rtl,
    };
    if cascade.is_some()
        && let Some(computed) = root_style
    {
        walk.parents.push(Parent {
            computed,
            first_line,
            is_root: true,
        });
    }
    walk.transformer.word_break(&Pushed(builder));
    walk.children_and_pseudos(builder, root, &text_transform);
}

/// The text transform text inside an element set in `style` is pushed with, where Blitz
/// transforms text for the backend.
fn case_transform<B: InlineBuilder<StyloStyle>>(style: Option<&ComputedValues>) -> CaseTransform {
    match style {
        Some(style) if !B::TRANSFORMS_TEXT => CaseTransform::from_style(style),
        _ => CaseTransform::NONE,
    }
}

/// A box the walk is in, where it cascades `::first-line` or `::first-letter` styles.
struct Parent {
    computed: ServoArc<ComputedValues>,
    /// Its style on the first formatted line, where the block has a `::first-line`.
    first_line: Option<ServoArc<ComputedValues>>,
    /// Whether it is the block itself.
    is_root: bool,
}

/// The walk of an inline formatting context's DOM.
struct Walk<'a> {
    nodes: &'a NodeTree,
    /// The block the content is for.
    root: &'a Node,
    /// Transforms text where the backend does not.
    transformer: TextTransformer,
    /// What cascades `::first-line` and `::first-letter` styles, where the block has either.
    cascade: Option<DocumentCascade<'a>>,
    /// The boxes open around what is pushed next, the block's first, where the walk cascades.
    parents: Vec<Parent>,
    /// The block's `::first-letter` style, while the backend looks for the letter.
    first_letter: Option<ServoArc<ComputedValues>>,
    /// Whether the block is right-to-left.
    rtl: bool,
}

impl Walk<'_> {
    /// Pushes `parent`'s `::before`, children and `::after`, whose text is transformed by
    /// `text_transform`.
    fn children_and_pseudos<B: InlineBuilder<StyloStyle>>(
        &mut self,
        builder: &mut B,
        parent: &Node,
        text_transform: &CaseTransform,
    ) {
        if let Some(before) = parent.before() {
            self.node(builder, &self.nodes[before], text_transform);
        }
        for &child in &parent.children {
            self.node(builder, &self.nodes[child], text_transform);
        }
        if let Some(after) = parent.after() {
            self.node(builder, &self.nodes[after], text_transform);
        }
    }

    /// The element's own `lang`, where the backend reads it.
    #[inline]
    fn lang<B: InlineBuilder<StyloStyle>>(node: &Node) -> Option<&str> {
        if B::READS_LANG {
            own_language(node)
        } else {
            None
        }
    }

    /// Opens a box for `node`, set in `style`, with its style on the first line where the walk
    /// cascades it.
    fn open_box<B: InlineBuilder<StyloStyle>>(
        &mut self,
        builder: &mut B,
        node: &Node,
        style: &ServoArc<ComputedValues>,
        kind: SpanKind,
    ) {
        let Some(cascade) = self.cascade else {
            builder.open_box(
                node_key(node.id),
                StyloStyle::from_arc(style),
                kind,
                None,
                Self::lang::<B>(node),
            );
            return;
        };
        // On the first line, the same element inheriting from its parent's `::first-line`
        // instead.
        let first_line = self
            .parents
            .last()
            .and_then(|parent| parent.first_line.as_ref())
            .map(|first_line| cascade.reparent(node, style, first_line));
        builder.open_box(
            node_key(node.id),
            StyloStyle::from_arc(style),
            kind,
            first_line.as_ref().map(StyloStyle::from_arc),
            Self::lang::<B>(node),
        );
        self.parents.push(Parent {
            computed: style.clone(),
            first_line,
            is_root: false,
        });
    }

    /// Closes the box opened last.
    fn close_box<B: InlineBuilder<StyloStyle>>(&mut self, builder: &mut B, kind: SpanKind) {
        if self.cascade.is_some() && self.parents.len() > 1 {
            self.parents.pop();
        }
        builder.close_box(kind);
    }

    /// Hands the backend the block's `::first-letter` style in the box the text pushed next is
    /// in, while it looks for the letter.
    fn arm_first_letter<B: InlineBuilder<StyloStyle>>(&mut self, builder: &mut B) {
        let (Some(pseudo), Some(cascade)) = (&self.first_letter, self.cascade) else {
            return;
        };
        if !builder.wants_first_letter() {
            self.first_letter = None;
            return;
        }
        let Some(parent) = self.parents.last() else {
            return;
        };
        let letter = if parent.is_root {
            pseudo.clone()
        } else {
            cascade.first_letter(self.root, pseudo, &parent.computed)
        };
        let letter_first_line = parent
            .first_line
            .as_ref()
            .map(|first_line| cascade.first_letter(self.root, pseudo, first_line));
        builder.first_letter(
            StyloStyle::from_arc(&letter),
            letter_first_line.as_ref().map(StyloStyle::from_arc),
        );
    }

    /// Pushes `node`, whose text is transformed by its parent's `parent_transform`.
    fn node<B: InlineBuilder<StyloStyle>>(
        &mut self,
        builder: &mut B,
        node: &Node,
        parent_transform: &CaseTransform,
    ) {
        let element = match &node.data {
            NodeData::Element(element) | NodeData::AnonymousBlock(element) => element,
            NodeData::Text(data) => {
                // A backend that looks for the first letter transforms its text itself, so the
                // text it is pushed is the node's.
                if !data.content.is_empty() {
                    self.arm_first_letter(builder);
                }
                let text =
                    self.transformer
                        .transform(&data.content, parent_transform, &Pushed(builder));
                let key = node_key(node.id);
                builder.text(key, node.parent.map_or(key, node_key), text);
                return;
            }
            NodeData::Comment { .. } | NodeData::Document(_) => return,
        };
        if *element.name.local == *"input" && element.attr(local_name!("type")) == Some("hidden") {
            return;
        }
        let Some(style) = node.primary_styles() else {
            return;
        };
        let key = node_key(node.id);
        let display = node.display_style().unwrap_or(Display::inline());
        match (display.outside(), display.inside()) {
            (DisplayOutside::None, DisplayInside::None) => {}
            // No box, but its children inherit from it.
            (DisplayOutside::None, DisplayInside::Contents) => {
                let text_transform = case_transform::<B>(Some(&**style));
                self.open_box(builder, node, &style, SpanKind::Contents);
                self.children_and_pseudos(builder, node, &text_transform);
                self.close_box(builder, SpanKind::Contents);
            }
            _ if style.slow_clone_position().is_absolutely_positioned() => {
                let inline_level =
                    style.get_box().original_display.outside() == DisplayOutside::Inline;
                builder.absolute(key, StyloStyle::from_arc(&style), inline_level);
            }
            (DisplayOutside::Inline, DisplayInside::Flow) => {
                let tag = &element.name.local;
                if is_inline_box_element(tag) {
                    self.transformer.word_break(&Pushed(builder));
                    self.atomic(builder, node, &style);
                } else if *tag == local_name!("br") {
                    builder.line_break(
                        key,
                        StyloStyle::from_arc(&style),
                        br_clear(&style, element),
                    );
                    self.transformer.word_break(&Pushed(builder));
                } else if *tag == local_name!("wbr") && builder.break_opportunity(key) {
                } else if B::SETS_RUBY && *tag == local_name!("rp") {
                    // The fallback parentheses, for a renderer that cannot set ruby.
                } else {
                    let kind = if !B::SETS_RUBY {
                        SpanKind::Span
                    } else if *tag == local_name!("ruby") {
                        SpanKind::Ruby
                    } else if *tag == local_name!("rt") {
                        SpanKind::Annotation {
                            in_container: node.parent.is_some_and(|parent| {
                                self.nodes[parent]
                                    .data
                                    .is_element_with_tag_name(&local_name!("rtc"))
                            }),
                        }
                    } else if *tag == local_name!("rtc") {
                        // An annotation container is the annotation itself where it holds no
                        // `<rt>`; one that does is transparent, each `<rt>` an annotation.
                        let holds_annotation = node.children.iter().any(|&child| {
                            self.nodes[child]
                                .data
                                .is_element_with_tag_name(&local_name!("rt"))
                        });
                        if holds_annotation {
                            SpanKind::Contents
                        } else {
                            SpanKind::Annotation {
                                in_container: false,
                            }
                        }
                    } else {
                        SpanKind::Span
                    };
                    let text_transform = case_transform::<B>(Some(&**style));
                    self.open_box(builder, node, &style, kind);
                    self.children_and_pseudos(builder, node, &text_transform);
                    self.close_box(builder, kind);
                }
            }
            _ if style.slow_clone_float().is_floating() => {
                let side = match (style.slow_clone_float(), self.rtl) {
                    (Float::Right, _) | (Float::InlineStart, true) | (Float::InlineEnd, false) => {
                        FloatSide::Right
                    }
                    _ => FloatSide::Left,
                };
                builder.float(
                    key,
                    StyloStyle::from_arc(&style),
                    side,
                    Self::lang::<B>(node),
                );
            }
            _ => {
                self.transformer.word_break(&Pushed(builder));
                self.atomic(builder, node, &style);
            }
        }
    }

    /// Pushes the atomic inline `node`, set in `style`.
    fn atomic<B: InlineBuilder<StyloStyle>>(
        &mut self,
        builder: &mut B,
        node: &Node,
        style: &ServoArc<ComputedValues>,
    ) {
        builder.atomic(
            node_key(node.id),
            StyloStyle::from_arc(style),
            Self::lang::<B>(node),
        );
    }
}
