use crate::layout::fragment::{Fragment, FragmentKind, HRule, StrokedRect};
use crate::spec::answer::TextualAnswer;
use crate::spec::config::{DiscursiveSpaceType, PrintConfig};

use super::ColumnGeometry;

// ─────────────────────────────────────────────────────────────────────────────
// Constants (shared with essay, file, and draft submodules)
// ─────────────────────────────────────────────────────────────────────────────

/// Default number of answer lines when `line_count` is not specified.
pub(super) const DEFAULT_LINE_COUNT: u32 = 5;
/// HRule stroke width in points.
pub(super) const HRULE_STROKE_PT: f64 = 0.5;
/// StrokedRect border stroke width in points (Blank mode).
pub(super) const BLANK_BOX_STROKE_PT: f64 = 0.7;

// ─────────────────────────────────────────────────────────────────────────────
// Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Lay out the answer space for `QuestionKind::Textual`.
///
/// Returns `(fragments, total_height)` in column-relative coordinates
/// (y values start at `origin_y`).
pub(super) fn layout_textual(
    textual:  &TextualAnswer,
    geometry: &ColumnGeometry,
    origin_y: f64,
    config:   &PrintConfig,
    spc:      f64,
) -> (Vec<Fragment>, f64) {
    let line_height_pt = textual.line_height_cm
        .unwrap_or(config.discursive_line_height)
        * 28.3465
        * spc;

    let n_lines = textual.line_count.unwrap_or(DEFAULT_LINE_COUNT);

    // Total height: explicit blank box height takes priority.
    let total_height = if let Some(h_cm) = textual.blank_height_cm {
        h_cm * 28.3465 * spc
    } else {
        n_lines as f64 * line_height_pt
    };

    let mut frags = Vec::new();

    match config.discursive_space_type {
        DiscursiveSpaceType::Lines => {
            // One HRule at the bottom of each row.
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
        }

        DiscursiveSpaceType::Blank => {
            // One outlined box spanning the full answer area — no internal lines.
            frags.push(Fragment {
                x:      0.0,
                y:      origin_y,
                width:  geometry.column_width_pt,
                height: total_height,
                kind:   FragmentKind::StrokedRect(StrokedRect {
                    stroke_width: BLANK_BOX_STROKE_PT,
                    color:        "#000000".to_owned(),
                    dash:         None,
                }),
            });
        }

        DiscursiveSpaceType::NoBorder => {
            // No visual elements — only vertical space is consumed.
        }
    }

    (frags, total_height)
}
