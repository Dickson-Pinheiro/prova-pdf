use serde::{Deserialize, Serialize};
use super::style::Style;

/// All content that can appear inline within a stem, alternative, or base text.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum InlineContent {
    Text(InlineText),
    Math(InlineMath),
    Image(InlineImage),
    Sub(InlineSubSup),
    Sup(InlineSubSup),
    /// A rendered blank (underline) for cloze fill-in.
    Blank(InlineBlank),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineText {
    pub value: String,
    #[serde(default)]
    pub style: Option<Style>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineMath {
    pub latex: String,
    /// true = display (centered, full width), false = inline.
    #[serde(default)]
    pub display: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineImage {
    /// Key registered via add_image().
    pub key: String,
    /// Width in cm. Inferred from height or natural size if None.
    #[serde(default)]
    pub width_cm: Option<f64>,
    #[serde(default)]
    pub height_cm: Option<f64>,
    #[serde(default)]
    pub caption: Option<String>,
}

/// Subscript or superscript run (used for both Sub and Sup variants).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineSubSup {
    /// The nested inline content rendered at reduced size with vertical offset.
    pub content: Vec<InlineContent>,
}

/// A blank/underline placeholder for cloze questions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct InlineBlank {
    /// Width of the blank in cm. Default 3.5cm if not specified.
    pub width_cm: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_text_deserializes() {
        let json = r#"{"type":"text","value":"Hello"}"#;
        let ic: InlineContent = serde_json::from_str(json).unwrap();
        assert!(matches!(ic, InlineContent::Text(_)));
    }

    #[test]
    fn inline_math_display_false_by_default() {
        let json = r#"{"type":"math","latex":"x^2"}"#;
        let ic: InlineContent = serde_json::from_str(json).unwrap();
        if let InlineContent::Math(m) = ic {
            assert!(!m.display);
        } else { panic!(); }
    }

    #[test]
    fn inline_sub_deserializes() {
        let json = r#"{"type":"sub","content":[{"type":"text","value":"2"}]}"#;
        let ic: InlineContent = serde_json::from_str(json).unwrap();
        assert!(matches!(ic, InlineContent::Sub(_)));
    }

    #[test]
    fn inline_sup_deserializes() {
        let json = r#"{"type":"sup","content":[{"type":"text","value":"n"}]}"#;
        let ic: InlineContent = serde_json::from_str(json).unwrap();
        assert!(matches!(ic, InlineContent::Sup(_)));
    }

    #[test]
    fn inline_blank_default_width_is_none() {
        let json = r#"{"type":"blank"}"#;
        let ic: InlineContent = serde_json::from_str(json).unwrap();
        if let InlineContent::Blank(b) = ic {
            assert!(b.width_cm.is_none());
        } else { panic!(); }
    }

    #[test]
    fn inline_blank_with_width() {
        let json = r#"{"type":"blank","widthCm":4.0}"#;
        let ic: InlineContent = serde_json::from_str(json).unwrap();
        if let InlineContent::Blank(b) = ic {
            assert_eq!(b.width_cm, Some(4.0));
        } else { panic!(); }
    }
}
