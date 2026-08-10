//! Application-owned semantic paint policy.
//!
//! Themes contain named colors and sparse named text styles. They deliberately
//! do not contain layout or terminal geometry.

use std::collections::HashMap;

use crate::presentation::api::{StyleSpec, ThemeColor, ThemeKey};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Theme {
    colors: HashMap<ThemeKey, ThemeColor>,
    styles: HashMap<ThemeKey, StyleSpec>,
}

impl Theme {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_color(mut self, key: impl Into<ThemeKey>, color: ThemeColor) -> Self {
        self.colors.insert(key.into(), color);
        self
    }

    pub fn with_style(mut self, key: impl Into<ThemeKey>, style: StyleSpec) -> Self {
        self.styles.insert(key.into(), style);
        self
    }

    pub fn set_color(&mut self, key: impl Into<ThemeKey>, color: ThemeColor) -> Option<ThemeColor> {
        self.colors.insert(key.into(), color)
    }

    pub fn set_style(&mut self, key: impl Into<ThemeKey>, style: StyleSpec) -> Option<StyleSpec> {
        self.styles.insert(key.into(), style)
    }

    pub fn color(&self, key: &str) -> Option<ThemeColor> {
        self.colors
            .iter()
            .find_map(|(name, color)| (name.as_str() == key).then_some(*color))
    }

    pub fn style(&self, key: &str) -> Option<&StyleSpec> {
        self.styles
            .iter()
            .find_map(|(name, style)| (name.as_str() == key).then_some(style))
    }
}
