use crate::fonts::resolve::{FontResolver, FontRole};
use crate::layout::fragment::Fragment;
use crate::layout::inline::InlineLayoutEngine;
use crate::spec::answer::ClozeAnswer;
use crate::spec::config::PrintConfig;
use crate::spec::style::ResolvedStyle;

use super::choice::build_alt_content;
use super::ColumnGeometry;

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Vertical gap between the stem (with inline blanks) and the word bank.
const WORD_BANK_TOP_MARGIN_PT: f64 = 6.0;
/// Vertical gap between consecutive word-bank items.
const WORD_BANK_ITEM_GAP_PT: f64 = 2.0;

// ─────────────────────────────────────────────────────────────────────────────
// Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Lay out the answer-space section for `QuestionKind::Cloze`.
///
/// The stem blanks are already rendered as part of the stem (InlineLayoutEngine handles
/// `InlineContent::Blank` → `FilledRect` underline).  This function renders only the
/// **word bank** below the stem (if present).
///
/// Returns `(fragments, total_height)`.
pub(super) fn layout_cloze<'a>(
    cloze:           &ClozeAnswer,
    resolver:        &'a FontResolver<'a>,
    geometry:        &ColumnGeometry,
    font_size:       f64,
    line_spacing:    f64,
    blank_default_cm: f64,
    origin_y:        f64,
    spc:             f64,
    _config:         &PrintConfig,
) -> (Vec<Fragment>, f64) {
    if cloze.word_bank.is_empty() {
        return (vec![], 0.0);
    }

    let mut frags   = Vec::new();
    let mut local_y = origin_y + WORD_BANK_TOP_MARGIN_PT * spc;

    let style = ResolvedStyle { font_size, line_spacing, ..ResolvedStyle::default() };

    for (idx, item_content) in cloze.word_bank.iter().enumerate() {
        // Render each word-bank entry as "1) content", "2) content", …
        let prefix  = format!("{}) ", idx + 1);
        let content = build_alt_content(&prefix, item_content);
        let engine  = InlineLayoutEngine {
            resolver,
            available_width:  geometry.column_width_pt,
            font_size,
            line_spacing,
            blank_default_cm,
            justify: false,
        };
        let (f, h) = engine.layout(&content, FontRole::Body, &style, 0.0, local_y);
        frags.extend(f);
        local_y += h + WORD_BANK_ITEM_GAP_PT * spc;
    }

    let total_h = local_y - origin_y;
    (frags, total_h)
}
