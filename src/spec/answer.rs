use serde::{Deserialize, Serialize};
use super::inline::InlineContent;

/// Describes the answer space. Must be compatible with the parent Question.kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AnswerSpace {
    Choice(ChoiceAnswer),
    Textual(TextualAnswer),
    Cloze(ClozeAnswer),
    Sum(SumAnswer),
    Essay(EssayAnswer),
    File(FileAnswer),
}

// ── Choice ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceAnswer {
    pub alternatives: Vec<Alternative>,
    #[serde(default)]
    pub layout: AlternativeLayout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alternative {
    /// "A", "B", "C" for multiple-choice; "01", "02", "04" for somatório.
    pub label: String,
    pub content: Vec<InlineContent>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AlternativeLayout {
    #[default]
    Vertical,
    Horizontal,
}

// ── Textual ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct TextualAnswer {
    /// Number of answer lines (mutually exclusive with blank_height_cm).
    pub line_count: Option<u32>,
    /// Height of blank answer box in cm.
    pub blank_height_cm: Option<f64>,
    /// Height per line in cm (overrides PrintConfig.discursive_line_height).
    pub line_height_cm: Option<f64>,
}

// ── Cloze ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClozeAnswer {
    /// Words/expressions available to fill the blanks (word bank).
    pub word_bank: Vec<Vec<InlineContent>>,
    #[serde(default)]
    pub shuffle_display: bool,
}

// ── Sum (Somatório) ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SumAnswer {
    pub items: Vec<SumItem>,
    #[serde(default = "default_true")]
    pub show_sum_box: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SumItem {
    /// Numeric value: 1, 2, 4, 8, 16, 32, 64 etc.
    pub value: u32,
    pub content: Vec<InlineContent>,
}

fn default_true() -> bool { true }

// ── Essay ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct EssayAnswer {
    pub line_count: Option<u32>,
    pub height_cm: Option<f64>,
}

// ── File ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FileAnswer {
    pub label: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choice_answer_deserializes() {
        let json = r#"{
            "type": "choice",
            "alternatives": [
                {"label": "A", "content": [{"type":"text","value":"Sim"}]},
                {"label": "B", "content": [{"type":"text","value":"Não"}]}
            ]
        }"#;
        let a: AnswerSpace = serde_json::from_str(json).unwrap();
        if let AnswerSpace::Choice(c) = a {
            assert_eq!(c.alternatives.len(), 2);
            assert_eq!(c.alternatives[0].label, "A");
        } else { panic!(); }
    }

    #[test]
    fn textual_answer_with_line_count() {
        let json = r#"{"type":"textual","lineCount":5}"#;
        let a: AnswerSpace = serde_json::from_str(json).unwrap();
        if let AnswerSpace::Textual(t) = a {
            assert_eq!(t.line_count, Some(5));
        } else { panic!(); }
    }

    #[test]
    fn sum_answer_show_box_default_true() {
        let json = r#"{"type":"sum","items":[{"value":1,"content":[]}]}"#;
        let a: AnswerSpace = serde_json::from_str(json).unwrap();
        if let AnswerSpace::Sum(s) = a {
            assert!(s.show_sum_box);
        } else { panic!(); }
    }

    #[test]
    fn essay_answer_defaults() {
        let a: EssayAnswer = serde_json::from_str("{}").unwrap();
        assert!(a.line_count.is_none());
        assert!(a.height_cm.is_none());
    }
}
