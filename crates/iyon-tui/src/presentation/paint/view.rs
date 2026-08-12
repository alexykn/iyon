use crate::{
    physical::{PhysicalStyle, Surface},
    presentation::{IntoView, TextSpan, View},
};

use crate::presentation::{
    ir::WidthRule,
    layout::{LayoutContent, LayoutNode, LayoutNodeId, LayoutTree, ViewCompiler},
};

/// Physical lowering facade. The compiler supplies root bounds; bounded
/// callers compute retained geometry before requesting lowering.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ViewPainter;

impl ViewPainter {
    pub(crate) fn paint_tree(&self, compiler: &ViewCompiler, tree: &LayoutTree) -> Surface {
        self.paint_tree_with_style(compiler, tree, PhysicalStyle::default())
    }

    pub(crate) fn paint_tree_with_style(
        &self,
        compiler: &ViewCompiler,
        tree: &LayoutTree,
        inherited: PhysicalStyle,
    ) -> Surface {
        let mut surface = self.paint_node(
            compiler,
            tree,
            tree.root,
            inherited,
            compiler.style_context(tree.node(tree.root).style.component_scope),
        );
        surface.physically_complete = tree.physically_complete;
        surface
    }

    fn paint_node(
        &self,
        compiler: &ViewCompiler,
        tree: &LayoutTree,
        id: LayoutNodeId,
        inherited: PhysicalStyle,
        inherited_context: crate::presentation::paint::StyleContext,
    ) -> Surface {
        let node = tree.node(id);
        let context = inherited_context
            .with_states(&node.style.style_states)
            .with_scope(compiler.style_context(node.style.component_scope));
        let resolved = compiler.theme.resolve_text_style(
            inherited,
            &node.style.decoration.text_style,
            &context,
        );
        let mut output = Surface::new(node.rect.width, node.rect.height);

        match &node.content {
            LayoutContent::Text { text, width_rule } => {
                let painted = compiler.paint_text(
                    text,
                    node.content_rect.width,
                    *width_rule,
                    resolved,
                    &context,
                );
                let x = node.content_rect.x.saturating_sub(node.rect.x);
                let y = node.content_rect.y.saturating_sub(node.rect.y);
                output.composite(&painted, x, y);
                output.physically_complete = painted.physically_complete;
            }
            LayoutContent::Spacer { rows } => {
                let height = (*rows).min(node.content_rect.height);
                let painted = Surface::new(node.content_rect.width, height);
                let x = node.content_rect.x.saturating_sub(node.rect.x);
                let y = node.content_rect.y.saturating_sub(node.rect.y);
                output.composite(&painted, x, y);
            }
            LayoutContent::Children | LayoutContent::Clamp { .. } => {
                self.paint_children(compiler, tree, node, &mut output, resolved, &context);
                if let LayoutContent::Clamp { overflow } = &node.content
                    && node
                        .children
                        .first()
                        .is_some_and(|child| tree.node(*child).rect.height > node.rect.height)
                {
                    self.paint_overflow_indicator(
                        compiler,
                        &mut output,
                        node,
                        overflow,
                        resolved,
                        &context,
                    );
                }
            }
            LayoutContent::RowViewport { skip_rows } => {
                if output.width() != 0 && output.height() != 0 {
                    let child_id = node
                        .children
                        .first()
                        .copied()
                        .expect("row viewport must have one child");
                    let painted =
                        self.paint_node(compiler, tree, child_id, resolved, context.clone());
                    for y in 0..output.height() {
                        let source_y = usize::from(*skip_rows).saturating_add(usize::from(y));
                        if source_y >= usize::from(painted.height()) {
                            continue;
                        }
                        for x in 0..output.width().min(painted.width()) {
                            *output.get_mut(x, y) = painted.get(x, source_y as u16).clone();
                        }
                    }
                    output.physically_complete = painted.physically_complete;
                }
            }
        }

        if let Some(color) = &node.style.decoration.surface_background {
            output.apply_surface_background(compiler.theme.resolve_color(color, &context));
        }
        if let Some(border) = &node.style.decoration.border {
            crate::presentation::paint::paint_border(
                &mut output,
                border,
                &compiler.theme,
                resolved,
                &context,
            );
        }
        output
    }

    fn paint_children(
        &self,
        compiler: &ViewCompiler,
        tree: &LayoutTree,
        node: &LayoutNode,
        output: &mut Surface,
        resolved: PhysicalStyle,
        context: &crate::presentation::paint::StyleContext,
    ) {
        for child in &node.children {
            let child_node = tree.node(*child);
            let painted = self.paint_node(compiler, tree, *child, resolved, context.clone());
            let x = child_node.rect.x.saturating_sub(node.rect.x);
            let y = child_node.rect.y.saturating_sub(node.rect.y);
            output.composite(&painted, x, y);
        }
    }

    fn paint_overflow_indicator(
        &self,
        compiler: &ViewCompiler,
        output: &mut Surface,
        node: &LayoutNode,
        overflow: &crate::presentation::OverflowIndicator,
        inherited: PhysicalStyle,
        context: &crate::presentation::paint::StyleContext,
    ) {
        if output.height() == 0 {
            return;
        }
        let Some((text, style)) = (match overflow {
            crate::presentation::OverflowIndicator::None => None,
            crate::presentation::OverflowIndicator::Ellipsis { style } => {
                Some(("…".to_owned(), style.clone()))
            }
            crate::presentation::OverflowIndicator::Footer { prefix, style } => {
                Some((prefix.clone(), style.clone()))
            }
        }) else {
            return;
        };
        let indicator_view = View::styled_text(vec![TextSpan::styled(text, style)])
            .fill_width()
            .no_wrap()
            .into_view();
        let crate::presentation::ir::ViewKind::Text(indicator_text) = &indicator_view.kind else {
            unreachable!("overflow indicator must be text")
        };
        let indicator = compiler.paint_text(
            indicator_text,
            node.rect.width,
            WidthRule::Fill,
            inherited,
            context,
        );
        let row = output.height() - 1;
        for x in 0..output.width() {
            *output.get_mut(x, row) = crate::physical::PhysicalCell::transparent();
            if x < indicator.width() {
                *output.get_mut(x, row) = indicator.get(x, 0).clone();
            }
        }
    }
}
