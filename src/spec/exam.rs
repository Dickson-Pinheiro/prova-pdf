use serde::{Deserialize, Serialize};
use super::{
    config::PrintConfig,
    header::InstitutionalHeader,
    inline::InlineContent,
    question::Question,
    style::Style,
};

/// Root document — the complete exam specification passed to generate_pdf().
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ExamSpec {
    pub metadata: ExamMetadata,
    pub config:   PrintConfig,
    pub header:   InstitutionalHeader,
    /// Ordered list of question groups (by subject, category, etc.).
    pub sections: Vec<Section>,
    /// Formula sheets, global base texts, or figures appended at the end.
    pub appendix: Option<Appendix>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ExamMetadata {
    pub title:    Option<String>,
    pub author:   Option<String>,
    pub subject:  Option<String>,
    pub date:     Option<String>,
    pub keywords: Vec<String>,
}

/// A labeled group of questions, optionally preceded by instructions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    /// Section heading (e.g., "Seção A — Geometria").
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub instructions: Vec<InlineContent>,
    pub questions: Vec<Question>,
    /// Tag used for SeparateMode::ByCategory grouping.
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub style: Option<Style>,
    /// Force page break before this section.
    #[serde(default)]
    pub force_page_break: bool,
}

/// Document-level appendix rendered after all sections.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Appendix {
    pub title: Option<String>,
    pub content: Vec<AppendixItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AppendixItem {
    Block(AppendixBlock),
    FormulaSheet(FormulaSheet),
    PageBreak,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppendixBlock {
    pub content: Vec<InlineContent>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub style: Option<Style>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaSheet {
    #[serde(default)]
    pub title: Option<String>,
    pub formulas: Vec<FormulaEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaEntry {
    #[serde(default)]
    pub label: Option<String>,
    pub latex: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exam_spec_empty_is_valid() {
        let spec = ExamSpec::default();
        assert!(spec.sections.is_empty());
        assert!(spec.appendix.is_none());
    }

    #[test]
    fn exam_spec_deserializes_from_minimal_json() {
        let json = r#"{"sections":[]}"#;
        let spec: ExamSpec = serde_json::from_str(json).unwrap();
        assert!(spec.sections.is_empty());
    }

    #[test]
    fn section_force_page_break_default_false() {
        let json = r#"{"questions":[]}"#;
        let s: Section = serde_json::from_str(json).unwrap();
        assert!(!s.force_page_break);
    }

    #[test]
    fn appendix_item_page_break_deserializes() {
        let json = r#"{"type":"pageBreak"}"#;
        let item: AppendixItem = serde_json::from_str(json).unwrap();
        assert!(matches!(item, AppendixItem::PageBreak));
    }

    #[test]
    fn formula_entry_with_label() {
        let json = r#"{"label":"Bhaskara","latex":"x = \\frac{-b \\pm \\sqrt{b^2-4ac}}{2a}"}"#;
        let f: FormulaEntry = serde_json::from_str(json).unwrap();
        assert_eq!(f.label.as_deref(), Some("Bhaskara"));
        assert!(f.latex.contains("frac"));
    }
}
