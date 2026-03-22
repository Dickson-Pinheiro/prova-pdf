use std::rc::Rc;

use crate::fonts::resolve::{FontResolver, FontRole};
use crate::layout::fragment::{Fragment, FragmentKind, GlyphRun, HRule};
use crate::layout::text::{shape_text, shaped_text_width};
use crate::spec::style::{FontStyle, FontWeight};

use super::ColumnGeometry;
use super::textual::HRULE_STROKE_PT;

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Font size for the "Rascunho" label, in points.
pub(super) const DRAFT_LABEL_SIZE_PT: f64 = 8.0;
/// Default height of each draft line when `draft_line_height` is not specified (0.7 cm).
const DRAFT_DEFAULT_LINE_HEIGHT_CM: f64 = 0.7;
/// HRule stroke color for draft lines (light gray).
const DRAFT_LINE_COLOR: &str = "#AAAAAA";
/// Vertical gap between the answer space and the "Rascunho" label.
const DRAFT_TOP_MARGIN_PT: f64 = 4.0;
/// Vertical gap between the "Rascunho" label and the first draft line.
const DRAFT_LABEL_GAP_PT: f64 = 2.0;

// ─────────────────────────────────────────────────────────────────────────────
// Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Lay out N draft lines with a "Rascunho" header label.
///
/// Returns `(fragments, total_height)` in column-relative coordinates starting
/// from `origin_y`.
pub(super) fn layout_draft_lines<'a>(
    n_lines:          u32,
    line_height_cm:   Option<f64>,
    resolver:         &'a FontResolver<'a>,
    geometry:         &ColumnGeometry,
    origin_y:         f64,
    spc:              f64,
) -> (Vec<Fragment>, f64) {
    let mut frags   = Vec::new();
    let mut local_y = origin_y + DRAFT_TOP_MARGIN_PT * spc;

    let line_height_pt = line_height_cm
        .unwrap_or(DRAFT_DEFAULT_LINE_HEIGHT_CM)
        * 28.3465
        * spc;

    // ── "Rascunho" label ─────────────────────────────────────────────────────
    let fd     = resolver.resolve(FontRole::Body, FontWeight::Normal, FontStyle::Italic, None);
    let glyphs = shape_text(fd, "Rascunho");
    let w      = shaped_text_width(&glyphs, DRAFT_LABEL_SIZE_PT, fd.units_per_em);
    let ascent = fd.ascender as f64 / fd.units_per_em as f64 * DRAFT_LABEL_SIZE_PT;
    let family = Rc::from(resolver.resolve_family_name(FontRole::Body, None));

    frags.push(Fragment {
        x:      0.0,
        y:      local_y,
        width:  w,
        height: DRAFT_LABEL_SIZE_PT,
        kind:   FragmentKind::GlyphRun(GlyphRun::from_shaped(
            &glyphs, DRAFT_LABEL_SIZE_PT, family, 0, Rc::from(DRAFT_LINE_COLOR), ascent,
        )),
    });
    local_y += DRAFT_LABEL_SIZE_PT + DRAFT_LABEL_GAP_PT * spc;

    // ── N gray HRules ─────────────────────────────────────────────────────────
    for i in 0..n_lines {
        let y = local_y + (i as f64 + 1.0) * line_height_pt;
        frags.push(Fragment {
            x:      0.0,
            y,
            width:  geometry.column_width_pt,
            height: HRULE_STROKE_PT,
            kind:   FragmentKind::HRule(HRule {
                stroke_width: HRULE_STROKE_PT,
                color:        DRAFT_LINE_COLOR.to_owned(),
            }),
        });
    }
    local_y += n_lines as f64 * line_height_pt;

    let total_h = local_y - origin_y;
    (frags, total_h)
}
