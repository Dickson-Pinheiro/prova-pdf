use crate::layout::fragment::{Fragment, FragmentKind, HRule, StrokedRect};
use crate::spec::answer::EssayAnswer;
use crate::spec::config::PrintConfig;

use super::ColumnGeometry;
use super::textual::{BLANK_BOX_STROKE_PT, DEFAULT_LINE_COUNT, HRULE_STROKE_PT};

// ─────────────────────────────────────────────────────────────────────────────
// Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Lay out the answer space for `QuestionKind::Essay`.
///
/// Priority: `height_cm` → one blank box of fixed height;
///           `line_count` → N `HRule` lines spaced by `config.discursive_line_height`;
///           neither     → falls back to `DEFAULT_LINE_COUNT` HRules.
///
/// Returns `(fragments, total_height)` in column-relative coordinates.
pub(super) fn layout_essay(
    essay:    &EssayAnswer,
    geometry: &ColumnGeometry,
    origin_y: f64,
    config:   &PrintConfig,
    spc:      f64,
) -> (Vec<Fragment>, f64) {
    let line_height_pt = config.discursive_line_height * 28.3465 * spc;

    if let Some(h_cm) = essay.height_cm {
        // Fixed-height blank box — no internal lines.
        let box_height = h_cm * 28.3465 * spc;
        let frag = Fragment {
            x:      0.0,
            y:      origin_y,
            width:  geometry.column_width_pt,
            height: box_height,
            kind:   FragmentKind::StrokedRect(StrokedRect {
                stroke_width: BLANK_BOX_STROKE_PT,
                color:        "#000000".to_owned(),
                dash:         None,
            }),
        };
        return (vec![frag], box_height);
    }

    // N HRule lines.
    let n_lines = essay.line_count.unwrap_or(DEFAULT_LINE_COUNT);
    let total_height = n_lines as f64 * line_height_pt;
    let mut frags = Vec::with_capacity(n_lines as usize);
    for i in 0..n_lines {
        let y = origin_y + (i as f64 + 1.0) * line_height_pt;
        frags.push(Fragment {
            x:      0.0,
            y,
            width:  geometry.column_width_pt,
            height: HRULE_STROKE_PT,
            kind:   FragmentKind::HRule(HRule {
                stroke_width: HRULE_STROKE_PT,
                color:        "#000000".to_owned(),
            }),
        });
    }
    (frags, total_height)
}
