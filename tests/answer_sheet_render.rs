//! Integration test: render the answer-sheet fixture with IBM Plex Sans and
//! write the candidate PDF used by the visual-comparison harness
//! (`tests/answer_sheet/compare.py`).

#![cfg(feature = "answer-sheet")]

use std::collections::HashMap;

use prova_pdf::fonts::{FontRegistry, FontRules};
use prova_pdf::pipeline::answer_sheet::render_answer_sheet;
use prova_pdf::pipeline::RenderContext;
use prova_pdf::spec::AnswerSheetSpec;

fn ibm_plex_ctx() -> RenderContext {
    let mut registry = FontRegistry::new();
    registry
        .add_variant(
            "body",
            0,
            include_bytes!("../fonts/IBMPlexSans-Regular.ttf").to_vec(),
        )
        .unwrap();
    registry
        .add_variant(
            "body",
            1,
            include_bytes!("../fonts/IBMPlexSans-Bold.ttf").to_vec(),
        )
        .unwrap();

    let mut images = HashMap::new();
    images.insert(
        "client_logo".to_owned(),
        include_bytes!("answer_sheet/fixtures/logo_placeholder.png").to_vec(),
    );

    RenderContext { registry, rules: FontRules::default(), images }
}

fn fixture() -> AnswerSheetSpec {
    serde_json::from_str(include_str!("answer_sheet/fixtures/rede_decisao.json"))
        .expect("fixture must be a valid AnswerSheetSpec")
}

#[test]
fn render_fixture_and_write_candidate() {
    let pdf = render_answer_sheet(&fixture(), &ibm_plex_ctx())
        .expect("fixture must render");
    assert!(pdf.starts_with(b"%PDF-"));

    // Write the candidate for compare.py; the out/ dir is git-ignored.
    let out_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/answer_sheet/out");
    std::fs::create_dir_all(out_dir).unwrap();
    std::fs::write(format!("{out_dir}/candidate.pdf"), &pdf).unwrap();
}

#[test]
fn fixture_is_single_page() {
    let pdf = render_answer_sheet(&fixture(), &ibm_plex_ctx()).unwrap();
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/Count 1"), "5 questions must fit one page");
}
