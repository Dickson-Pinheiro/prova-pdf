//! Phase 1 — Validation.
//!
//! Collects *all* errors in one pass and returns them as a `Vec<ValidationError>`
//! (non-fatal: callers may decide whether to abort or just warn).
//!
//! Rules enforced:
//!   1. `registry.is_ready()` — at least one family with a real body font.
//!   2. At least one section; every section has at least one question.
//!   3. Choice questions: ≥ 2 alternatives; all labels unique within the question.
//!   4. Every image key referenced in any inline content or header is present in
//!      the ImageStore.
//!   5. `StudentField.width_cm`, if provided, must be > 0.

use std::collections::{HashMap, HashSet};

use crate::fonts::FontRegistry;
use crate::spec::answer::AnswerSpace;
use crate::spec::exam::{AppendixItem, ExamSpec};
use crate::spec::inline::InlineContent;

// ─────────────────────────────────────────────────────────────────────────────
// Error type
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    /// No font with real bytes is registered (registry.is_ready() == false).
    NoFont,
    /// The exam has no sections.
    NoSections,
    /// A section exists but has no questions.
    EmptySectionAt { index: usize },
    /// A Choice question has fewer than 2 alternatives.
    ChoiceTooFewAlternatives { section: usize, question: usize, count: usize },
    /// Two or more alternatives in the same question share the same label.
    ChoiceDuplicateLabel { section: usize, question: usize, label: String },
    /// An image key is referenced in the content but not registered in the store.
    MissingImage { key: String },
    /// A StudentField declares `width_cm ≤ 0`.
    InvalidStudentFieldWidth { label: String, width_cm: f64 },
}

// ─────────────────────────────────────────────────────────────────────────────
// Main entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Validate `spec` against the given `registry` and `images` store.
///
/// Always performs all checks and returns every error found, so the caller gets
/// a complete picture in a single call.
pub fn validate(
    spec:     &ExamSpec,
    registry: &FontRegistry,
    images:   &HashMap<String, Vec<u8>>,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    check_fonts(registry,              &mut errors);
    check_sections(spec,               &mut errors);
    check_choice_questions(spec,       &mut errors);
    check_images(spec, images,         &mut errors);
    check_student_fields(spec,         &mut errors);

    errors
}

// ─────────────────────────────────────────────────────────────────────────────
// Individual checks
// ─────────────────────────────────────────────────────────────────────────────

fn check_fonts(registry: &FontRegistry, errors: &mut Vec<ValidationError>) {
    if !registry.is_ready() {
        errors.push(ValidationError::NoFont);
    }
}

fn check_sections(spec: &ExamSpec, errors: &mut Vec<ValidationError>) {
    if spec.sections.is_empty() {
        errors.push(ValidationError::NoSections);
        return; // no point iterating an empty slice
    }
    for (i, section) in spec.sections.iter().enumerate() {
        if section.questions.is_empty() {
            errors.push(ValidationError::EmptySectionAt { index: i });
        }
    }
}

fn check_choice_questions(spec: &ExamSpec, errors: &mut Vec<ValidationError>) {
    for (si, section) in spec.sections.iter().enumerate() {
        for (qi, question) in section.questions.iter().enumerate() {
            if let AnswerSpace::Choice(choice) = &question.answer {
                // Minimum alternatives
                if choice.alternatives.len() < 2 {
                    errors.push(ValidationError::ChoiceTooFewAlternatives {
                        section:  si,
                        question: qi,
                        count:    choice.alternatives.len(),
                    });
                }
                // Unique labels
                let mut seen: HashSet<&str> = HashSet::new();
                for alt in &choice.alternatives {
                    if !seen.insert(alt.label.as_str()) {
                        errors.push(ValidationError::ChoiceDuplicateLabel {
                            section:  si,
                            question: qi,
                            label:    alt.label.clone(),
                        });
                    }
                }
            }
        }
    }
}

fn check_images(
    spec:   &ExamSpec,
    images: &HashMap<String, Vec<u8>>,
    errors: &mut Vec<ValidationError>,
) {
    let mut required: HashSet<String> = HashSet::new();

    // header logo
    if let Some(ref key) = spec.header.logo_key {
        required.insert(key.clone());
    }
    // header instructions
    collect_inline_images(&spec.header.instructions, &mut required);

    // sections
    for section in &spec.sections {
        collect_inline_images(&section.instructions, &mut required);
        for question in &section.questions {
            collect_inline_images(&question.stem, &mut required);
            for bt in &question.base_texts {
                collect_inline_images(&bt.content, &mut required);
            }
            collect_answer_images(&question.answer, &mut required);
        }
    }

    // appendix
    if let Some(ref appendix) = spec.appendix {
        for item in &appendix.content {
            if let AppendixItem::Block(b) = item {
                collect_inline_images(&b.content, &mut required);
            }
        }
    }

    // report missing keys in deterministic order
    let mut missing: Vec<String> = required
        .into_iter()
        .filter(|k| !images.contains_key(k))
        .collect();
    missing.sort();
    for key in missing {
        errors.push(ValidationError::MissingImage { key });
    }
}

fn check_student_fields(spec: &ExamSpec, errors: &mut Vec<ValidationError>) {
    for field in &spec.header.student_fields {
        if let Some(w) = field.width_cm {
            if w <= 0.0 {
                errors.push(ValidationError::InvalidStudentFieldWidth {
                    label:    field.label.clone(),
                    width_cm: w,
                });
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Recursively collect every image key referenced in a slice of InlineContent.
fn collect_inline_images(content: &[InlineContent], keys: &mut HashSet<String>) {
    for item in content {
        match item {
            InlineContent::Image(img) => { keys.insert(img.key.clone()); }
            InlineContent::Sub(s)    => collect_inline_images(&s.content, keys),
            InlineContent::Sup(s)    => collect_inline_images(&s.content, keys),
            InlineContent::Text(_)
            | InlineContent::Math(_)
            | InlineContent::Blank(_) => {}
        }
    }
}

fn collect_answer_images(answer: &AnswerSpace, keys: &mut HashSet<String>) {
    match answer {
        AnswerSpace::Choice(c) => {
            for alt in &c.alternatives {
                collect_inline_images(&alt.content, keys);
            }
        }
        AnswerSpace::Cloze(c) => {
            for entry in &c.word_bank {
                collect_inline_images(entry, keys);
            }
        }
        AnswerSpace::Sum(s) => {
            for item in &s.items {
                collect_inline_images(&item.content, keys);
            }
        }
        AnswerSpace::Textual(_)
        | AnswerSpace::Essay(_)
        | AnswerSpace::File(_) => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::fonts::FontRegistry;
    use crate::spec::answer::{Alternative, AnswerSpace, ChoiceAnswer, AlternativeLayout, TextualAnswer};
    use crate::spec::exam::{ExamSpec, Section};
    use crate::spec::header::{InstitutionalHeader, StudentField};
    use crate::spec::inline::{InlineImage, InlineText, InlineContent};
    use crate::spec::question::{Question, QuestionKind};
    use crate::test_helpers::fixtures::make_resolver_and_rules;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn ready_registry() -> FontRegistry {
        let (reg, _) = make_resolver_and_rules();
        reg
    }

    fn empty_registry() -> FontRegistry {
        FontRegistry::new()
    }

    fn no_images() -> HashMap<String, Vec<u8>> {
        HashMap::new()
    }

    fn text_inline(s: &str) -> InlineContent {
        InlineContent::Text(InlineText { value: s.into(), style: None })
    }

    fn image_inline(key: &str) -> InlineContent {
        InlineContent::Image(InlineImage {
            key:       key.into(),
            width_cm:  None,
            height_cm: None,
            caption:   None,
        })
    }

    fn simple_choice_question(labels: &[&str]) -> Question {
        let alternatives = labels.iter().map(|&l| Alternative {
            label:   l.into(),
            content: vec![text_inline("option")],
        }).collect();
        Question {
            number:            None,
            label:             None,
            kind:              QuestionKind::Choice,
            stem:              vec![text_inline("stem")],
            answer:            AnswerSpace::Choice(ChoiceAnswer {
                alternatives,
                layout: AlternativeLayout::Vertical,
            }),
            base_texts:        vec![],
            points:            None,
            full_width:        false,
            draft_lines:       0,
            draft_line_height: None,
            show_number:       true,
            force_page_break:  false,
            style:             None,
        }
    }

    fn textual_question() -> Question {
        Question {
            number:            None,
            label:             None,
            kind:              QuestionKind::Textual,
            stem:              vec![text_inline("stem")],
            answer:            AnswerSpace::Textual(TextualAnswer::default()),
            base_texts:        vec![],
            points:            None,
            full_width:        false,
            draft_lines:       0,
            draft_line_height: None,
            show_number:       true,
            force_page_break:  false,
            style:             None,
        }
    }

    fn one_section(questions: Vec<Question>) -> ExamSpec {
        ExamSpec {
            sections: vec![Section {
                title:           None,
                instructions:    vec![],
                questions,
                category:        None,
                style:           None,
                force_page_break: false,
            }],
            ..ExamSpec::default()
        }
    }

    fn has_error(errors: &[ValidationError], target: &ValidationError) -> bool {
        errors.contains(target)
    }

    // ── NoFont ────────────────────────────────────────────────────────────────

    #[test]
    fn no_font_reported_when_registry_empty() {
        let spec   = one_section(vec![textual_question()]);
        let errors = validate(&spec, &empty_registry(), &no_images());
        assert!(has_error(&errors, &ValidationError::NoFont));
    }

    #[test]
    fn no_font_error_absent_when_font_registered() {
        let spec   = one_section(vec![textual_question()]);
        let errors = validate(&spec, &ready_registry(), &no_images());
        assert!(!has_error(&errors, &ValidationError::NoFont));
    }

    // ── NoSections ────────────────────────────────────────────────────────────

    #[test]
    fn no_sections_reported_when_sections_empty() {
        let spec   = ExamSpec::default(); // sections = []
        let errors = validate(&spec, &ready_registry(), &no_images());
        assert!(has_error(&errors, &ValidationError::NoSections));
    }

    #[test]
    fn no_sections_absent_when_at_least_one_section() {
        let spec   = one_section(vec![textual_question()]);
        let errors = validate(&spec, &ready_registry(), &no_images());
        assert!(!has_error(&errors, &ValidationError::NoSections));
    }

    // ── EmptySection ──────────────────────────────────────────────────────────

    #[test]
    fn empty_section_reported_at_correct_index() {
        let spec = ExamSpec {
            sections: vec![
                Section { questions: vec![textual_question()], title: None, instructions: vec![], category: None, style: None, force_page_break: false },
                Section { questions: vec![],                   title: None, instructions: vec![], category: None, style: None, force_page_break: false },
            ],
            ..ExamSpec::default()
        };
        let errors = validate(&spec, &ready_registry(), &no_images());
        assert!(has_error(&errors, &ValidationError::EmptySectionAt { index: 1 }));
        assert!(!has_error(&errors, &ValidationError::EmptySectionAt { index: 0 }));
    }

    // ── Choice: too few alternatives ──────────────────────────────────────────

    #[test]
    fn choice_one_alternative_is_invalid() {
        let spec   = one_section(vec![simple_choice_question(&["A"])]);
        let errors = validate(&spec, &ready_registry(), &no_images());
        assert!(has_error(&errors, &ValidationError::ChoiceTooFewAlternatives {
            section: 0, question: 0, count: 1,
        }));
    }

    #[test]
    fn choice_two_alternatives_is_valid() {
        let spec   = one_section(vec![simple_choice_question(&["A", "B"])]);
        let errors = validate(&spec, &ready_registry(), &no_images());
        assert!(!has_error(&errors, &ValidationError::ChoiceTooFewAlternatives {
            section: 0, question: 0, count: 2,
        }));
    }

    #[test]
    fn choice_zero_alternatives_reports_count_zero() {
        let spec   = one_section(vec![simple_choice_question(&[])]);
        let errors = validate(&spec, &ready_registry(), &no_images());
        assert!(has_error(&errors, &ValidationError::ChoiceTooFewAlternatives {
            section: 0, question: 0, count: 0,
        }));
    }

    // ── Choice: duplicate labels ──────────────────────────────────────────────

    #[test]
    fn choice_duplicate_label_reported() {
        let spec   = one_section(vec![simple_choice_question(&["A", "B", "A"])]);
        let errors = validate(&spec, &ready_registry(), &no_images());
        assert!(has_error(&errors, &ValidationError::ChoiceDuplicateLabel {
            section: 0, question: 0, label: "A".into(),
        }));
    }

    #[test]
    fn choice_unique_labels_no_error() {
        let spec   = one_section(vec![simple_choice_question(&["A", "B", "C", "D"])]);
        let errors = validate(&spec, &ready_registry(), &no_images());
        assert!(!errors.iter().any(|e| matches!(e, ValidationError::ChoiceDuplicateLabel { .. })));
    }

    // ── Missing images ────────────────────────────────────────────────────────

    #[test]
    fn missing_image_in_stem_reported() {
        let mut q  = textual_question();
        q.stem.push(image_inline("fig1"));
        let spec   = one_section(vec![q]);
        let errors = validate(&spec, &ready_registry(), &no_images());
        assert!(has_error(&errors, &ValidationError::MissingImage { key: "fig1".into() }));
    }

    #[test]
    fn present_image_in_stem_no_error() {
        let mut q   = textual_question();
        q.stem.push(image_inline("fig1"));
        let spec    = one_section(vec![q]);
        let mut img = HashMap::new();
        img.insert("fig1".into(), vec![0u8]);
        let errors  = validate(&spec, &ready_registry(), &img);
        assert!(!has_error(&errors, &ValidationError::MissingImage { key: "fig1".into() }));
    }

    #[test]
    fn missing_logo_key_reported() {
        let spec = ExamSpec {
            header: InstitutionalHeader { logo_key: Some("logo.png".into()), ..Default::default() },
            sections: vec![Section { questions: vec![textual_question()], title: None, instructions: vec![], category: None, style: None, force_page_break: false }],
            ..ExamSpec::default()
        };
        let errors = validate(&spec, &ready_registry(), &no_images());
        assert!(has_error(&errors, &ValidationError::MissingImage { key: "logo.png".into() }));
    }

    #[test]
    fn image_in_choice_alternative_reported() {
        let mut q = simple_choice_question(&["A", "B"]);
        if let AnswerSpace::Choice(ref mut c) = q.answer {
            c.alternatives[0].content.push(image_inline("alt_img"));
        }
        let spec   = one_section(vec![q]);
        let errors = validate(&spec, &ready_registry(), &no_images());
        assert!(has_error(&errors, &ValidationError::MissingImage { key: "alt_img".into() }));
    }

    // ── StudentField width ────────────────────────────────────────────────────

    #[test]
    fn student_field_zero_width_reported() {
        let spec = ExamSpec {
            header: InstitutionalHeader {
                student_fields: vec![StudentField { label: "Nome".into(), width_cm: Some(0.0) }],
                ..Default::default()
            },
            sections: vec![Section { questions: vec![textual_question()], title: None, instructions: vec![], category: None, style: None, force_page_break: false }],
            ..ExamSpec::default()
        };
        let errors = validate(&spec, &ready_registry(), &no_images());
        assert!(has_error(&errors, &ValidationError::InvalidStudentFieldWidth {
            label: "Nome".into(), width_cm: 0.0,
        }));
    }

    #[test]
    fn student_field_negative_width_reported() {
        let spec = ExamSpec {
            header: InstitutionalHeader {
                student_fields: vec![StudentField { label: "Turma".into(), width_cm: Some(-1.5) }],
                ..Default::default()
            },
            sections: vec![Section { questions: vec![textual_question()], title: None, instructions: vec![], category: None, style: None, force_page_break: false }],
            ..ExamSpec::default()
        };
        let errors = validate(&spec, &ready_registry(), &no_images());
        assert!(has_error(&errors, &ValidationError::InvalidStudentFieldWidth {
            label: "Turma".into(), width_cm: -1.5,
        }));
    }

    #[test]
    fn student_field_none_width_is_valid() {
        let spec = ExamSpec {
            header: InstitutionalHeader {
                student_fields: vec![StudentField { label: "Nome".into(), width_cm: None }],
                ..Default::default()
            },
            sections: vec![Section { questions: vec![textual_question()], title: None, instructions: vec![], category: None, style: None, force_page_break: false }],
            ..ExamSpec::default()
        };
        let errors = validate(&spec, &ready_registry(), &no_images());
        assert!(!errors.iter().any(|e| matches!(e, ValidationError::InvalidStudentFieldWidth { .. })));
    }

    // ── Multiple errors in one pass ───────────────────────────────────────────

    #[test]
    fn multiple_errors_collected_in_single_call() {
        // No font + no sections → two errors at once.
        let spec   = ExamSpec::default();
        let errors = validate(&spec, &empty_registry(), &no_images());
        assert!(has_error(&errors, &ValidationError::NoFont));
        assert!(has_error(&errors, &ValidationError::NoSections));
        assert_eq!(errors.len(), 2);
    }

    // ── Clean spec produces no errors ─────────────────────────────────────────

    #[test]
    fn valid_spec_produces_no_errors() {
        let spec   = one_section(vec![
            textual_question(),
            simple_choice_question(&["A", "B", "C", "D", "E"]),
        ]);
        let errors = validate(&spec, &ready_registry(), &no_images());
        assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
    }
}
