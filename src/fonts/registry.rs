use std::collections::HashMap;
use super::data::{FontData, FontFamily};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("no font registered with name '{0}'")]
    FamilyNotFound(String),
    #[error("invalid variant index {0}: must be 0 (regular), 1 (bold), 2 (italic), or 3 (bold-italic)")]
    InvalidVariant(u8),
    #[error("font parse error: {0}")]
    ParseError(String),
}

/// Registry of named font families.
///
/// The key "body" is the default and must be registered before generating a PDF.
/// Other names are used via Style.font_family or FontRules.
pub struct FontRegistry {
    families: HashMap<String, FontFamily>,
}

impl FontRegistry {
    pub fn new() -> Self {
        Self { families: HashMap::new() }
    }

    /// Register a complete family under a name.
    pub fn add_family(&mut self, name: impl Into<String>, family: FontFamily) {
        self.families.insert(name.into(), family);
    }

    /// Register a single variant of a (possibly new) family.
    /// variant: 0=regular (mandatory first), 1=bold, 2=italic, 3=bold-italic.
    pub fn add_variant(&mut self, family_name: &str, variant: u8, data: Vec<u8>) -> Result<(), RegistryError> {
        let font_data = FontData::from_bytes(&data)
            .map_err(|e| RegistryError::ParseError(e.to_string()))?;

        let family = self.families
            .entry(family_name.to_string())
            .or_insert_with(|| FontFamily {
                regular: FontData::empty(),
                bold: None,
                italic: None,
                bold_italic: None,
            });

        match variant {
            0 => family.regular     = font_data,
            1 => family.bold        = Some(font_data),
            2 => family.italic      = Some(font_data),
            3 => family.bold_italic = Some(font_data),
            v => return Err(RegistryError::InvalidVariant(v)),
        }
        Ok(())
    }

    /// Get a family by name.
    pub fn get(&self, name: &str) -> Option<&FontFamily> {
        self.families.get(name)
    }

    /// Get the default body family.
    pub fn body(&self) -> Option<&FontFamily> {
        self.get("body")
    }

    /// True if at least one family with a real regular font is registered.
    pub fn is_ready(&self) -> bool {
        self.families.values().any(|f| !f.regular.is_empty())
    }

    pub fn family_names(&self) -> impl Iterator<Item = &str> {
        self.families.keys().map(String::as_str)
    }
}

impl Default for FontRegistry {
    fn default() -> Self { Self::new() }
}

/// Semantic roles mapped to font family names.
/// All roles default to "body" so a one-family setup works out of the box.
#[derive(Debug, Clone)]
pub struct FontRules {
    pub body:     String,
    pub heading:  String,
    pub question: String,
    pub math:     String,
}

impl Default for FontRules {
    fn default() -> Self {
        Self {
            body:     "body".into(),
            heading:  "body".into(),
            question: "body".into(),
            math:     "body".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_registry_is_not_ready() {
        assert!(!FontRegistry::new().is_ready());
    }

    #[test]
    fn invalid_variant_returns_error() {
        let mut reg = FontRegistry::new();
        // We can't easily add a valid font in unit tests without a TTF file,
        // but we can test the variant validation path.
        // Use a dummy call expecting variant 9 to fail with InvalidVariant.
        let result = reg.add_variant("test", 9, vec![]);
        assert!(matches!(result, Err(RegistryError::InvalidVariant(9))));
    }

    #[test]
    fn font_rules_default_all_body() {
        let rules = FontRules::default();
        assert_eq!(rules.body, "body");
        assert_eq!(rules.heading, "body");
        assert_eq!(rules.question, "body");
        assert_eq!(rules.math, "body");
    }
}
