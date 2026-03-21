use serde::{Deserialize, Serialize};
use super::{answer::AnswerSpace, inline::InlineContent, style::Style};

/// A single exam question.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    // ── Identity ──────────────────────────────────────────────────────────────
    /// Display number (auto-incremented from section start if None).
    #[serde(default)]
    pub number: Option<u32>,
    /// Optional label overriding the number (e.g., "Questão Extra").
    #[serde(default)]
    pub label: Option<String>,

    // ── Content ───────────────────────────────────────────────────────────────
    /// Category of answer expected.
    pub kind: QuestionKind,
    /// The question stem (enunciado).
    pub stem: Vec<InlineContent>,
    /// The answer space descriptor.
    pub answer: AnswerSpace,

    // ── Supporting material ───────────────────────────────────────────────────
    #[serde(default)]
    pub base_texts: Vec<BaseText>,

    // ── Scoring ───────────────────────────────────────────────────────────────
    #[serde(default)]
    pub points: Option<f64>,

    // ── Layout modifiers ──────────────────────────────────────────────────────
    /// In 2-column layout, this question spans both columns (like CSS column-span: all).
    #[serde(default)]
    pub full_width: bool,
    /// Number of ruled scratch/draft lines rendered after the answer space.
    #[serde(default)]
    pub draft_lines: u32,
    /// Height of each draft line in cm. Falls back to PrintConfig.discursive_line_height.
    #[serde(default)]
    pub draft_line_height: Option<f64>,
    /// Whether to render the question number badge. Default true.
    #[serde(default = "default_true")]
    pub show_number: bool,
    /// Force a page break immediately before this question.
    #[serde(default)]
    pub force_page_break: bool,

    // ── Style override ────────────────────────────────────────────────────────
    #[serde(default)]
    pub style: Option<Style>,
}

fn default_true() -> bool { true }

/// Determines how the answer space is rendered and which AnswerSpace variant is expected.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum QuestionKind {
    /// Multiple-choice with labeled alternatives.
    Choice,
    /// Open-ended written response.
    Textual,
    /// Fill-in-the-blanks with optional word bank.
    Cloze,
    /// "Somatório": binary-value items; student sums selected values.
    Sum,
    /// Long essay with large space.
    Essay,
    /// File-upload placeholder (digital exam context).
    File,
}

/// A supporting text, figure, or quotation positioned relative to a question or section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseText {
    pub content: Vec<InlineContent>,
    pub position: BaseTextPosition,
    /// Optional title label (e.g., "Texto I", "Figura 1").
    #[serde(default)]
    pub title: Option<String>,
    /// Attribution line rendered below the content (author, source, year).
    #[serde(default)]
    pub attribution: Option<String>,
    #[serde(default)]
    pub style: Option<Style>,
}

/// Positioning of a BaseText relative to its parent (question or section).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum BaseTextPosition {
    /// Full-width block rendered before the question stem.
    #[default]
    BeforeQuestion,
    /// Full-width block rendered after the answer space.
    AfterQuestion,
    /// Left column; question stem + answer in right column (mini 2-col layout).
    LeftOfQuestion,
    /// Right column; question in left column.
    RightOfQuestion,
    /// Before all questions in the section (section-level).
    SectionTop,
    /// Before all sections (document-level).
    ExamTop,
    /// After all sections (document-level).
    ExamBottom,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::answer::{AnswerSpace, ChoiceAnswer, Alternative};

    fn make_simple_choice() -> Question {
        Question {
            number: Some(1),
            label: None,
            kind: QuestionKind::Choice,
            stem: vec![InlineContent::Text(crate::spec::inline::InlineText {
                value: "Qual a capital do Brasil?".into(),
                style: None,
            })],
            answer: AnswerSpace::Choice(ChoiceAnswer {
                alternatives: vec![
                    Alternative { label: "A".into(), content: vec![] },
                    Alternative { label: "B".into(), content: vec![] },
                ],
                layout: Default::default(),
            }),
            base_texts: vec![],
            points: Some(1.0),
            full_width: false,
            draft_lines: 0,
            draft_line_height: None,
            show_number: true,
            force_page_break: false,
            style: None,
        }
    }

    #[test]
    fn question_show_number_default_true() {
        let json = r#"{
            "kind": "textual",
            "stem": [],
            "answer": {"type": "textual"}
        }"#;
        let q: Question = serde_json::from_str(json).unwrap();
        assert!(q.show_number);
    }

    #[test]
    fn question_draft_lines_default_zero() {
        let q: Question = serde_json::from_str(r#"{"kind":"file","stem":[],"answer":{"type":"file"}}"#).unwrap();
        assert_eq!(q.draft_lines, 0);
    }

    #[test]
    fn full_width_default_false() {
        let q = make_simple_choice();
        assert!(!q.full_width);
    }

    #[test]
    fn base_text_position_default_before_question() {
        let bt: BaseTextPosition = serde_json::from_str(r#""beforeQuestion""#).unwrap();
        assert_eq!(bt, BaseTextPosition::BeforeQuestion);
    }

    #[test]
    fn all_base_text_positions_roundtrip() {
        for pos in [
            BaseTextPosition::BeforeQuestion,
            BaseTextPosition::AfterQuestion,
            BaseTextPosition::LeftOfQuestion,
            BaseTextPosition::RightOfQuestion,
            BaseTextPosition::SectionTop,
            BaseTextPosition::ExamTop,
            BaseTextPosition::ExamBottom,
        ] {
            let s = serde_json::to_string(&pos).unwrap();
            let back: BaseTextPosition = serde_json::from_str(&s).unwrap();
            assert_eq!(pos, back);
        }
    }

    #[test]
    fn all_question_kinds_roundtrip() {
        for kind in [
            QuestionKind::Choice,
            QuestionKind::Textual,
            QuestionKind::Cloze,
            QuestionKind::Sum,
            QuestionKind::Essay,
            QuestionKind::File,
        ] {
            let s = serde_json::to_string(&kind).unwrap();
            let back: QuestionKind = serde_json::from_str(&s).unwrap();
            assert_eq!(kind, back);
        }
    }
}
