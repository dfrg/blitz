//! The document, as the text backends ask after it while they paint.

use blitz_dom::text::blitz_text::{NodeKey, PaintHost};
use blitz_dom::text::{StyloStyle, key_node, node_key};
use blitz_dom::{BaseDocument, Node, NodeData, NodeId};

/// The document's nodes and styles, by the keys the text backends name them by.
#[derive(Clone, Copy)]
pub(crate) struct DomPaintHost<'a> {
    pub(crate) doc: &'a BaseDocument,
}

impl DomPaintHost<'_> {
    /// The node `key` names.
    #[inline]
    fn node(&self, key: NodeKey) -> Option<&Node> {
        self.doc.get_node(key_node(key))
    }

    /// The element `node` is or is in: a text node's parent, or the element itself.
    #[inline]
    fn styled<'n>(&'n self, node: &'n Node) -> Option<&'n Node> {
        match node.data {
            NodeData::Text(_) => self.doc.get_node(node.parent?),
            _ => Some(node),
        }
    }
}

impl PaintHost<StyloStyle> for DomPaintHost<'_> {
    fn style(&self, key: NodeKey) -> Option<StyloStyle> {
        let node = self.node(key)?;
        self.styled(node)?
            .primary_styles()
            .map(|style| StyloStyle((*style).clone()))
    }

    fn parent(&self, key: NodeKey) -> Option<NodeKey> {
        self.node(key)?.parent.map(node_key)
    }

    fn element(&self, key: NodeKey) -> Option<NodeKey> {
        let node = self.node(key)?;
        self.styled(node).map(|element| node_key(element.id))
    }

    fn relative_shift(&self, key: NodeKey, containing: kurbo::Size, rtl: bool) -> kurbo::Vec2 {
        blitz_dom::text::relative_shift(self.node(key), containing, rtl)
    }

    fn propagated_decorations(&self, root: NodeKey) -> Vec<StyloStyle> {
        propagated_decorations(self.doc, key_node(root))
    }
}

/// The styles of the boxes around `root` whose decorations propagate to its inline content, the
/// outermost first.
///
/// Decorations propagate to in-flow children, so the walk stops at a box that is floated,
/// absolutely positioned or an atomic inline: its own decorations are its content's, its
/// ancestors' are not.
fn propagated_decorations(doc: &BaseDocument, root: NodeId) -> Vec<StyloStyle> {
    use style::values::specified::box_::{DisplayInside, DisplayOutside};
    let mut propagated = Vec::new();
    let mut at = doc.get_node(root);
    while let Some(node) = at {
        let Some(styles) = node.primary_styles() else {
            // An anonymous block may have no style of its own: it is in the flow of the box
            // around it.
            at = node.parent.and_then(|parent| doc.get_node(parent));
            continue;
        };
        if node.id != root && !styles.slow_clone_text_decoration_line().is_empty() {
            propagated.push(StyloStyle((*styles).clone()));
        }
        let display = styles.slow_clone_display();
        let atomic = display.outside() == DisplayOutside::Inline
            && display.inside() != DisplayInside::Flow
            && !display.is_contents();
        // A `display: contents` element generates no box, so its `float` and `position` take
        // nothing out of the flow.
        let out_of_flow = !display.is_contents()
            && (styles.slow_clone_position().is_absolutely_positioned()
                || styles.slow_clone_float() != style::values::computed::Float::None);
        if atomic || out_of_flow {
            break;
        }
        at = node.parent.and_then(|parent| doc.get_node(parent));
    }
    propagated.reverse();
    propagated
}
