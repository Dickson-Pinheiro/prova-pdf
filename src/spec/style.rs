use serde::{Deserialize, Serialize};

/// Partial style — all fields are Option so absent fields cascade from context.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Style {
    pub font_size:        Option<f64>,
    pub font_weight:      Option<FontWeight>,
    pub font_style:       Option<FontStyle>,
    /// Named font family from the FontRegistry (overrides role-based rules).
    pub font_family:      Option<String>,
    pub color:            Option<String>,
    pub background_color: Option<String>,
    pub underline:        Option<bool>,
    pub text_align:       Option<TextAlign>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum FontWeight {
    #[default]
    Normal,
    Bold,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justified,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Border {
    pub width: Option<f64>,
    pub color: Option<String>,
    pub style: Option<BorderStyle>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum BorderStyle {
    Solid,
    Dashed,
    Dotted,
}

/// Fully-resolved style for a specific element (after cascading from PrintConfig
/// → Section → Question → InlineContent). All fields have concrete values.
///
/// Produced by the style-cascade phase (TASK-010); consumed by the layout engine.
#[derive(Debug, Clone)]
pub struct ResolvedStyle {
    pub font_size:    f64,
    pub font_weight:  FontWeight,
    pub font_style:   FontStyle,
    /// Explicit font-family override; `None` = follow FontRules for the current role.
    pub font_family:  Option<String>,
    /// Normalised RGB in \[0, 1\].
    pub color:        (f32, f32, f32),
    pub underline:    bool,
    pub text_align:   TextAlign,
    /// Multiplier applied to `font_size` to get the inter-baseline distance.
    pub line_spacing: f64,
}

impl Default for ResolvedStyle {
    fn default() -> Self {
        Self {
            font_size:    12.0,
            font_weight:  FontWeight::Normal,
            font_style:   FontStyle::Normal,
            font_family:  None,
            color:        (0.0, 0.0, 0.0),
            underline:    false,
            text_align:   TextAlign::Left,
            line_spacing: 1.4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_font_family_deserializes() {
        let json = r#"{"fontFamily":"IBM Plex Sans","fontSize":11.0}"#;
        let s: Style = serde_json::from_str(json).unwrap();
        assert_eq!(s.font_family.as_deref(), Some("IBM Plex Sans"));
        assert_eq!(s.font_size, Some(11.0));
    }

    #[test]
    fn style_defaults_to_none() {
        let s = Style::default();
        assert!(s.font_size.is_none());
        assert!(s.font_family.is_none());
    }
}
