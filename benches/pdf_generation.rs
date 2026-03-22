//! Benchmarks for prova-pdf PDF generation pipeline.
//!
//! Run with: `cargo bench`
//!
//! Groups:
//! - **end_to_end**: Full `render()` pipeline with N questions.
//! - **micro**: Individual hot-path functions.

use std::collections::HashMap;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

use prova_pdf::fonts::data::{FontData, FontFamily};
use prova_pdf::fonts::registry::{FontRegistry, FontRules};
use prova_pdf::pipeline::{render, RenderContext};
use prova_pdf::spec::answer::{Alternative, AlternativeLayout, AnswerSpace, ChoiceAnswer};
use prova_pdf::spec::config::PrintConfig;
use prova_pdf::spec::exam::{ExamSpec, Section};
use prova_pdf::spec::inline::{InlineContent, InlineText};
use prova_pdf::spec::question::{Question, QuestionKind};

// ─────────────────────────────────────────────────────────────────────────────
// Fixtures
// ─────────────────────────────────────────────────────────────────────────────

const DEJAVU: &[u8] = include_bytes!("../fonts/DejaVuSans.ttf");

fn make_registry() -> FontRegistry {
    let mut reg = FontRegistry::new();
    let fd = FontData::from_bytes(DEJAVU).unwrap();
    reg.add_family("body", FontFamily::new(fd));
    reg
}

fn make_ctx() -> RenderContext {
    RenderContext {
        registry: make_registry(),
        rules: FontRules::default(),
        images: HashMap::new(),
    }
}

/// Build an ExamSpec with `n` choice questions, each having 5 alternatives.
fn make_choice_spec(n: usize) -> ExamSpec {
    let questions: Vec<Question> = (1..=n)
        .map(|i| {
            let stem = vec![InlineContent::Text(InlineText {
                value: format!(
                    "Questão {i}: Considere a seguinte afirmação sobre o tema proposto. \
                     Qual das alternativas abaixo apresenta a resposta correta para o problema?"
                ),
                style: None,
            })];
            let alternatives: Vec<Alternative> = ["A", "B", "C", "D", "E"]
                .iter()
                .map(|label| Alternative {
                    label: label.to_string(),
                    content: vec![InlineContent::Text(InlineText {
                        value: format!("Alternativa {label} com texto de exemplo para benchmark."),
                        style: None,
                    })],
                })
                .collect();
            Question {
                number: Some(i as u32),
                label: None,
                kind: QuestionKind::Choice,
                stem,
                answer: AnswerSpace::Choice(ChoiceAnswer {
                    alternatives,
                    layout: AlternativeLayout::default(),
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
        })
        .collect();

    ExamSpec {
        sections: vec![Section {
            title: Some("Conhecimentos Gerais".into()),
            instructions: vec![],
            category: None,
            style: None,
            force_page_break: false,
            questions,
        }],
        config: PrintConfig {
            columns: 2,
            ..Default::default()
        },
        ..Default::default()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// End-to-end benchmarks
// ─────────────────────────────────────────────────────────────────────────────

fn bench_end_to_end(c: &mut Criterion) {
    let ctx = make_ctx();
    let mut group = c.benchmark_group("end_to_end");

    for n in [10, 50, 100] {
        let spec = make_choice_spec(n);
        group.bench_with_input(BenchmarkId::new("choice_questions", n), &n, |b, _| {
            b.iter(|| {
                let result = render(black_box(&spec), black_box(&ctx));
                assert!(result.is_ok());
                black_box(result.unwrap())
            });
        });
    }

    group.finish();
}

// ─────────────────────────────────────────────────────────────────────────────
// Micro-benchmarks
// ─────────────────────────────────────────────────────────────────────────────

fn bench_font_data(c: &mut Criterion) {
    let fd = FontData::from_bytes(DEJAVU).unwrap();
    let mut group = c.benchmark_group("micro");

    // FontData::text_width — called hundreds of times per exam
    group.bench_function("text_width_short", |b| {
        b.iter(|| black_box(fd.text_width(black_box("Questão 42"), 12.0)));
    });

    group.bench_function("text_width_long", |b| {
        let long_text = "Considere a seguinte afirmação sobre o tema proposto e analise as alternativas disponíveis para determinar a resposta correta.";
        b.iter(|| black_box(fd.text_width(black_box(long_text), 12.0)));
    });

    // FontData::glyph_id — called per character
    group.bench_function("glyph_id", |b| {
        b.iter(|| black_box(fd.glyph_id(black_box('A'))));
    });

    group.finish();
}

fn bench_shape_text(c: &mut Criterion) {
    use prova_pdf::layout::text::{shape_text, shaped_text_width};

    let fd = FontData::from_bytes(DEJAVU).unwrap();
    let mut group = c.benchmark_group("micro");

    let short = "Questão 42";
    let long = "Considere a seguinte afirmação sobre o tema proposto e analise as alternativas disponíveis para determinar a resposta correta ao problema apresentado.";

    group.bench_function("shape_text_short", |b| {
        b.iter(|| black_box(shape_text(black_box(&fd), black_box(short))));
    });

    group.bench_function("shape_text_long", |b| {
        b.iter(|| black_box(shape_text(black_box(&fd), black_box(long))));
    });

    // shaped_text_width — post-shaping width calculation
    let glyphs = shape_text(&fd, long);
    group.bench_function("shaped_text_width", |b| {
        b.iter(|| {
            black_box(shaped_text_width(
                black_box(&glyphs),
                black_box(12.0),
                black_box(fd.units_per_em),
            ))
        });
    });

    group.finish();
}

fn bench_all_kinds_fixture(c: &mut Criterion) {
    let ctx = make_ctx();
    let json = include_str!("../tests/fixtures/all_kinds.json");
    let spec: ExamSpec = serde_json::from_str(json).expect("all_kinds.json must be valid");

    c.bench_function("all_kinds_render", |b| {
        b.iter(|| {
            let result = render(black_box(&spec), black_box(&ctx));
            assert!(result.is_ok());
            black_box(result.unwrap())
        });
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Groups
// ─────────────────────────────────────────────────────────────────────────────

criterion_group!(
    benches,
    bench_end_to_end,
    bench_font_data,
    bench_shape_text,
    bench_all_kinds_fixture,
);
criterion_main!(benches);
