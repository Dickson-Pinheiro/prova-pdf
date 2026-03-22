use std::rc::Rc;

use crate::fonts::resolve::{FontResolver, FontRole};
use crate::layout::fragment::{FilledRect, Fragment, FragmentKind, GlyphRun, StrokedRect};
use crate::layout::text::{shape_text, shaped_text_width};
use crate::spec::answer::FileAnswer;
use crate::spec::style::{FontStyle, FontWeight};

use super::ColumnGeometry;
use super::textual::BLANK_BOX_STROKE_PT;

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Default label when `FileAnswer::label` is None.
const FILE_DEFAULT_LABEL: &str = "Anexe o arquivo no sistema";
/// Height of the file upload box in points (2.5 cm).
const FILE_BOX_HEIGHT_PT: f64 = 2.5 * 28.3465;
/// Dash pattern for the dashed border: 4pt on, 4pt off.
const FILE_DASH: [f64; 2] = [4.0, 4.0];
/// Icon placeholder square size in points (0.6 cm).
const FILE_ICON_PT: f64 = 0.6 * 28.3465;
/// Horizontal padding inside the box before/after content.
const FILE_BOX_PAD_PT: f64 = 6.0;

// ─────────────────────────────────────────────────────────────────────────────
// Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Lay out the answer space for `QuestionKind::File`.
///
/// Renders a dashed-border box spanning the column width with:
/// - A filled-rect icon placeholder on the left
/// - The instruction label as a `GlyphRun`
///
/// Returns `(fragments, total_height)` in column-relative coordinates.
pub(super) fn layout_file<'a>(
    file:         &FileAnswer,
    resolver:     &'a FontResolver<'a>,
    geometry:     &ColumnGeometry,
    font_size:    f64,
    line_spacing: f64,
    origin_y:     f64,
    spc:          f64,
) -> (Vec<Fragment>, f64) {
    let box_height = FILE_BOX_HEIGHT_PT * spc;

    // ── Dashed border ────────────────────────────────────────────────────────
    let border = Fragment {
        x:      0.0,
        y:      origin_y,
        width:  geometry.column_width_pt,
        height: box_height,
        kind:   FragmentKind::StrokedRect(StrokedRect {
            stroke_width: BLANK_BOX_STROKE_PT,
            color:        "#000000".to_owned(),
            dash:         Some(FILE_DASH),
        }),
    };

    // ── Icon placeholder (FilledRect, left-aligned, vertically centered) ─────
    let icon_pt  = FILE_ICON_PT * spc;
    let icon_y   = origin_y + (box_height - icon_pt) * 0.5;
    let icon = Fragment {
        x:      FILE_BOX_PAD_PT,
        y:      icon_y,
        width:  icon_pt,
        height: icon_pt,
        kind:   FragmentKind::FilledRect(FilledRect {
            color: "#cccccc".to_owned(),
        }),
    };

    // ── Label GlyphRun ───────────────────────────────────────────────────────
    let label_str = file.label.as_deref().unwrap_or(FILE_DEFAULT_LABEL);
    let fd        = resolver.resolve(FontRole::Body, FontWeight::Normal, FontStyle::Normal, None);
    let glyphs    = shape_text(fd, label_str);
    let label_w   = shaped_text_width(&glyphs, font_size, fd.units_per_em);
    let ascent    = fd.ascender as f64 / fd.units_per_em as f64 * font_size;
    let family    = Rc::from(resolver.resolve_family_name(FontRole::Body, None));
    let label_x   = FILE_BOX_PAD_PT * 2.0 + icon_pt;
    let label_y   = origin_y + (box_height - font_size).max(0.0) * 0.5;

    let label = Fragment {
        x:      label_x,
        y:      label_y,
        width:  label_w,
        height: font_size,
        kind:   FragmentKind::GlyphRun(GlyphRun::from_shaped(
            &glyphs, font_size, family, 0, Rc::from("#000000"), ascent,
        )),
    };

    let _ = line_spacing; // not used for fixed-height box
    (vec![border, icon, label], box_height)
}
