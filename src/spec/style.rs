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
