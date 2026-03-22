use super::data::{FontData, FontFamily};
use super::registry::{FontRegistry, FontRules};
use crate::spec::style::{FontStyle, FontWeight};

/// Semantic role of a text run — determines which FontRules entry is consulted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FontRole {
    Body,
    Heading,
    Question,
    Math,
}

/// Resolves a (role, weight, style, family_override) to a &FontData.
///
/// Resolution order:
///   1. family_override (from Style.font_family)
///   2. FontRules[role]
///   3. "body" (fallback)
///   4. First registered family
///   5. Panics (no fonts registered at all)
pub struct FontResolver<'a> {
    registry: &'a FontRegistry,
    rules:    &'a FontRules,
}

impl<'a> FontResolver<'a> {
    pub fn new(registry: &'a FontRegistry, rules: &'a FontRules) -> Self {
        Self { registry, rules }
    }

    pub fn resolve(
        &self,
        role: FontRole,
        weight: FontWeight,
        style: FontStyle,
        family_override: Option<&str>,
    ) -> &FontData {
        let name = family_override
            .or_else(|| Some(self.role_name(role)))
            .unwrap();

        let family = self.registry.get(name)
            .or_else(|| self.registry.body())
            .or_else(|| self.registry.family_names().next().and_then(|n| self.registry.get(n)))
            .expect("FontResolver::resolve called with no fonts registered");

        pick_variant(family, weight, style)
    }

    /// Return the registered family name that would be chosen for the given role
    /// and optional override. Mirrors the lookup order in `resolve()`.
    pub fn resolve_family_name<'b>(&self, role: FontRole, family_override: Option<&'b str>) -> &'b str
    where
        'a: 'b,
    {
        if let Some(name) = family_override {
            if self.registry.get(name).is_some() {
                return name;
            }
        }
        let role_name = self.role_name(role);
        if self.registry.get(role_name).is_some() {
            return role_name;
        }
        if self.registry.body().is_some() {
            return "body";
        }
        self.registry.family_names().next().unwrap_or("body")
    }

    fn role_name(&self, role: FontRole) -> &'a str {
        match role {
            FontRole::Body     => &self.rules.body,
            FontRole::Heading  => &self.rules.heading,
            FontRole::Question => &self.rules.question,
            FontRole::Math     => &self.rules.math,
        }
    }
}

fn pick_variant(family: &FontFamily, weight: FontWeight, style: FontStyle) -> &FontData {
    match (weight, style) {
        (FontWeight::Bold, FontStyle::Italic) => family.bold_italic.as_ref()
            .or(family.bold.as_ref())
            .unwrap_or(&family.regular),
        (FontWeight::Bold, _) => family.bold.as_ref().unwrap_or(&family.regular),
        (_, FontStyle::Italic) => family.italic.as_ref().unwrap_or(&family.regular),
        _ => &family.regular,
    }
}
