//! Semantic style cascade and theme-key resolution into physical styles.

use crate::{
    Theme,
    component::{ComponentId, MountGraph},
    physical::{AnsiColor as PhysicalAnsiColor, PhysicalColor, PhysicalStyle},
    presentation::api::{
        AnsiColor, ColorSpec, StyleFacts, StyleRef, StyleSpec, StyleStates, ThemeColor,
    },
};

#[derive(Debug, Clone, Default)]
pub(crate) struct StyleContext {
    pub(crate) inherited_states: StyleStates,
    pub(crate) local_facts: StyleFacts,
    pub(crate) focused: bool,
    pub(crate) focus_within: bool,
}

impl StyleContext {
    /// Enters a View node, installing its inheritable state and self-only
    /// semantic facts while updating the node's focus scope.
    pub(crate) fn enter_node(&self, states: &StyleStates, facts: &StyleFacts, scope: Self) -> Self {
        let mut next = self.clone();
        next.inherited_states.overlay(states);
        next.local_facts = facts.clone();
        next.focused = scope.focused;
        next.focus_within = scope.focus_within;
        next
    }

    /// Context passed to descendants. Physical style is passed separately;
    /// only the current node's local facts are cleared.
    pub(crate) fn for_descendant(&self) -> Self {
        Self {
            inherited_states: self.inherited_states.clone(),
            local_facts: StyleFacts::default(),
            focused: self.focused,
            focus_within: self.focus_within,
        }
    }

    pub(crate) fn with_local_facts(&self, facts: &StyleFacts) -> Self {
        let mut next = self.for_descendant();
        next.local_facts = facts.clone();
        next
    }

    pub(crate) fn for_scope(
        scope: Option<ComponentId>,
        focused: Option<ComponentId>,
        graph: Option<&MountGraph>,
    ) -> Self {
        let is_focused = scope.is_some_and(|scope| focused == Some(scope));
        let focus_within = scope.is_some_and(|scope| {
            focused.is_some_and(|focused| {
                graph.is_some_and(|graph| graph.is_descendant_or_self(focused, scope))
            })
        });
        Self {
            inherited_states: StyleStates::default(),
            local_facts: StyleFacts::default(),
            focused: is_focused,
            focus_within,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ThemeResolver {
    pub(crate) theme: Theme,
}

impl Default for ThemeResolver {
    fn default() -> Self {
        Self::new(&Theme::default())
    }
}

impl ThemeResolver {
    pub(crate) fn new(theme: &Theme) -> Self {
        Self {
            theme: theme.clone(),
        }
    }

    pub(crate) fn resolve_text_style(
        &self,
        inherited: PhysicalStyle,
        patch: &StyleRef,
        context: &StyleContext,
    ) -> PhysicalStyle {
        let mut resolved = inherited;
        if let Some(key) = &patch.theme
            && let Some(named) = self.theme.resolve_style(
                key.as_str(),
                context.focused,
                context.focus_within,
                &context.inherited_states,
                &context.local_facts,
            )
        {
            resolved = self.apply_style(resolved, &named, context);
        }
        self.apply_style(resolved, &patch.local, context)
    }

    fn apply_style(
        &self,
        mut resolved: PhysicalStyle,
        patch: &StyleSpec,
        context: &StyleContext,
    ) -> PhysicalStyle {
        if let Some(foreground) = &patch.foreground {
            resolved.foreground = Some(self.resolve_color(foreground, context));
        }
        if let Some(background) = &patch.background {
            resolved.background = Some(self.resolve_color(background, context));
        }
        if let Some(value) = patch.attributes.bold {
            resolved.bold = value;
        }
        if let Some(value) = patch.attributes.dim {
            resolved.dim = value;
        }
        if let Some(value) = patch.attributes.italic {
            resolved.italic = value;
        }
        if let Some(value) = patch.attributes.underline {
            resolved.underline = value;
        }
        if let Some(value) = patch.attributes.reversed {
            resolved.reversed = value;
        }
        resolved
    }

    pub(crate) fn resolve_color(&self, color: &ColorSpec, context: &StyleContext) -> PhysicalColor {
        match color {
            ColorSpec::Ansi(value) => PhysicalColor::Indexed(*value),
            ColorSpec::Named(color) => PhysicalColor::Named(to_physical_ansi(*color)),
            ColorSpec::Rgb { r, g, b } => PhysicalColor::Rgb {
                r: *r,
                g: *g,
                b: *b,
            },
            ColorSpec::Theme(key) => self
                .theme
                .resolve_color(
                    key.as_str(),
                    context.focused,
                    context.focus_within,
                    &context.inherited_states,
                    &context.local_facts,
                )
                .map_or(PhysicalColor::Default, to_physical_color),
        }
    }
}

fn to_physical_color(color: ThemeColor) -> PhysicalColor {
    match color {
        ThemeColor::Default => PhysicalColor::Default,
        ThemeColor::Named(color) => PhysicalColor::Named(to_physical_ansi(color)),
        ThemeColor::Indexed(value) => PhysicalColor::Indexed(value),
        ThemeColor::Rgb { r, g, b } => PhysicalColor::Rgb { r, g, b },
    }
}

fn to_physical_ansi(color: AnsiColor) -> PhysicalAnsiColor {
    match color {
        AnsiColor::Black => PhysicalAnsiColor::Black,
        AnsiColor::Red => PhysicalAnsiColor::Red,
        AnsiColor::Green => PhysicalAnsiColor::Green,
        AnsiColor::Yellow => PhysicalAnsiColor::Yellow,
        AnsiColor::Blue => PhysicalAnsiColor::Blue,
        AnsiColor::Magenta => PhysicalAnsiColor::Magenta,
        AnsiColor::Cyan => PhysicalAnsiColor::Cyan,
        AnsiColor::Gray => PhysicalAnsiColor::Gray,
        AnsiColor::DarkGray => PhysicalAnsiColor::DarkGray,
        AnsiColor::LightRed => PhysicalAnsiColor::LightRed,
        AnsiColor::LightGreen => PhysicalAnsiColor::LightGreen,
        AnsiColor::LightYellow => PhysicalAnsiColor::LightYellow,
        AnsiColor::LightBlue => PhysicalAnsiColor::LightBlue,
        AnsiColor::LightMagenta => PhysicalAnsiColor::LightMagenta,
        AnsiColor::LightCyan => PhysicalAnsiColor::LightCyan,
        AnsiColor::White => PhysicalAnsiColor::White,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        IntoView, StyleSelector, StyleStateKey, StyleStateValue, View,
        presentation::layout::compile_view_with_theme,
    };

    #[test]
    fn selectors_normalize_and_specific_variants_win() {
        let selector = StyleSelector::state("severity", "warning")
            .and_state("mode", "compact")
            .and_state("severity", "error");
        let equivalent = StyleSelector::state("mode", "compact").and_state("severity", "error");
        assert_eq!(selector, equivalent);

        let theme = Theme::new()
            .with_color("accent", ThemeColor::Named(AnsiColor::Green))
            .with_color_variant(
                "accent",
                StyleSelector::state("severity", "error"),
                ThemeColor::Named(AnsiColor::Red),
            )
            .with_color_variant(
                "accent",
                StyleSelector::state("severity", "error").and_focused(),
                ThemeColor::Named(AnsiColor::Yellow),
            );
        let view = View::text("x")
            .foreground(ColorSpec::theme("accent"))
            .into_view()
            .style_state("severity", "error");
        let rows = compile_view_with_theme(&view, 10, &theme).rows;
        assert_eq!(
            rows[0].style_at(0).unwrap().foreground,
            Some(PhysicalColor::Named(PhysicalAnsiColor::Red))
        );
        let mut focused_states = StyleStates::default();
        focused_states.set(
            StyleStateKey::from_static("severity"),
            StyleStateValue::from_static("error"),
        );
        let focused_context = StyleContext {
            inherited_states: focused_states,
            focused: true,
            focus_within: false,
            ..StyleContext::default()
        };
        assert_eq!(
            ThemeResolver::new(&theme)
                .resolve_color(&ColorSpec::theme("accent"), &focused_context,),
            PhysicalColor::Named(PhysicalAnsiColor::Yellow)
        );
    }

    #[test]
    fn setting_same_variant_replaces_and_returns_previous_value() {
        let mut theme = Theme::new();
        let selector = StyleSelector::focused();
        assert_eq!(
            theme.set_color_variant("accent", selector.clone(), ThemeColor::Indexed(1)),
            None
        );
        assert_eq!(
            theme.set_color_variant("accent", selector, ThemeColor::Indexed(2)),
            Some(ThemeColor::Indexed(1))
        );
        assert_eq!(
            theme.resolve_color(
                "accent",
                true,
                false,
                &StyleStates::default(),
                &StyleFacts::default(),
            ),
            Some(ThemeColor::Indexed(2))
        );

        let selector = StyleSelector::focus_within();
        assert_eq!(
            theme.set_style_variant("field", selector.clone(), StyleSpec::new().bold()),
            None
        );
        assert_eq!(
            theme.set_style_variant("field", selector, StyleSpec::new().italic()),
            Some(StyleSpec::new().bold())
        );
    }

    #[test]
    fn missing_theme_tokens_fall_back_to_default_physical_color() {
        let resolver = ThemeResolver::default();
        assert_eq!(
            resolver.resolve_color(&ColorSpec::theme("missing"), &StyleContext::default()),
            PhysicalColor::Default
        );
    }

    #[test]
    fn missing_named_style_is_a_no_op_and_local_style_overrides_named_fields() {
        let theme = Theme::new().with_style(
            "heading",
            StyleSpec::new().foreground(ColorSpec::ansi(1)).bold(),
        );
        let resolver = ThemeResolver::new(&theme);
        assert_eq!(
            resolver.resolve_text_style(
                PhysicalStyle::default(),
                &StyleRef::theme("missing"),
                &StyleContext::default(),
            ),
            PhysicalStyle::default()
        );
        let resolved = resolver.resolve_text_style(
            PhysicalStyle::default(),
            &StyleRef::themed("heading", StyleSpec::new().foreground(ColorSpec::ansi(2))),
            &StyleContext::default(),
        );
        assert_eq!(resolved.foreground, Some(PhysicalColor::Indexed(2)));
        assert!(resolved.bold);
    }

    #[test]
    fn theme_changes_paint_without_changing_geometry() {
        let themed = View::text("hello")
            .foreground(ColorSpec::theme("accent"))
            .into_view();
        let plain = View::text("hello").into_view();
        let theme = Theme::new().with_color("accent", ThemeColor::Indexed(1));
        let themed = compile_view_with_theme(&themed, 20, &theme);
        let plain = crate::presentation::layout::compile_view(&plain, 20);
        assert_eq!(
            (themed.width, themed.rows.len()),
            (plain.width, plain.rows.len())
        );
        assert_eq!(
            themed.rows[0].style_at(0).unwrap().foreground,
            Some(PhysicalColor::Indexed(1))
        );
    }

    #[test]
    fn named_style_variants_overlay_sparse_fields() {
        let theme = Theme::new()
            .with_style("field", StyleSpec::new().foreground(ColorSpec::ansi(1)))
            .with_style_variant("field", StyleSelector::focused(), StyleSpec::new().bold())
            .with_style_variant(
                "field",
                StyleSelector::state("severity", "error"),
                StyleSpec::new().foreground(ColorSpec::ansi(2)),
            );
        let mut states = StyleStates::default();
        states.set(
            StyleStateKey::from_static("severity"),
            StyleStateValue::from_static("error"),
        );
        let context = StyleContext {
            inherited_states: states,
            focused: true,
            focus_within: false,
            ..StyleContext::default()
        };
        let resolved = ThemeResolver::new(&theme).resolve_text_style(
            PhysicalStyle::default(),
            &StyleRef::theme("field"),
            &context,
        );
        assert_eq!(resolved.foreground, Some(PhysicalColor::Indexed(2)));
        assert!(resolved.bold);
    }
}
