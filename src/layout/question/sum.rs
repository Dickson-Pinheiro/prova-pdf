use std::rc::Rc;

use crate::fonts::resolve::{FontResolver, FontRole};
use crate::layout::fragment::{Fragment, FragmentKind, GlyphRun, StrokedRect};
use crate::layout::inline::InlineLayoutEngine;
use crate::layout::text::{shape_text, shaped_text_width};
use crate::spec::answer::SumAnswer;
use crate::spec::style::{FontStyle, FontWeight, ResolvedStyle};

use super::ColumnGeometry;

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Side length of the checkbox square for each sum item, in PDF points (0.4 cm).
const SUM_CHECKBOX_PT: f64 = 0.4 * 28.3465;
/// Horizontal gap between the checkbox and the item content.
const SUM_CHECKBOX_CONTENT_GAP_PT: f64 = 4.0;
/// Width of the value column on the right side of each item, in points (~1.5 cm).
const SUM_VALUE_COL_PT: f64 = 1.5 * 28.3465;
/// Vertical gap between consecutive sum items.
const SUM_ITEM_GAP_PT: f64 = 3.0;
/// Vertical margin before the "Soma:" box.
const SUM_BOX_TOP_MARGIN_PT: f64 = 6.0;
/// Height of the "Soma:" answer box in points (0.8 cm).
const SUM_BOX_HEIGHT_PT: f64 = 0.8 * 28.3465;
/// Width of the "Soma:" answer box in points (4 cm), right-aligned.
const SUM_BOX_WIDTH_PT: f64 = 4.0 * 28.3465;
/// Stroke for the checkbox border and the sum box border.
const SUM_STROKE_PT: f64 = 0.7;
/// Horizontal padding inside the sum box before the "Soma:" label.
const SUM_BOX_LABEL_PAD_PT: f64 = 4.0;

// ─────────────────────────────────────────────────────────────────────────────
// Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Lay out the answer space for `QuestionKind::Sum`.
///
/// Each item renders as:
///   `[ ]  <content inline>              <value right-aligned>`
///
/// When `show_sum_box` is true, a "Soma: ___" box is appended at the bottom right.
///
/// Returns `(fragments, total_height)` in column-relative coordinates.
pub(super) fn layout_sum<'a>(
    sum:             &SumAnswer,
    resolver:        &'a FontResolver<'a>,
    geometry:        &ColumnGeometry,
    font_size:       f64,
    line_spacing:    f64,
    blank_default_cm: f64,
    origin_y:        f64,
    spc:             f64,
) -> (Vec<Fragment>, f64) {
    let mut frags   = Vec::new();
    let mut local_y = origin_y;

    let style          = ResolvedStyle { font_size, line_spacing, ..ResolvedStyle::default() };
    let checkbox_pt    = SUM_CHECKBOX_PT * spc;
    let item_gap_pt    = SUM_ITEM_GAP_PT * spc;
    let content_indent = checkbox_pt + SUM_CHECKBOX_CONTENT_GAP_PT * spc;
    let value_col_pt   = SUM_VALUE_COL_PT;
    let content_width  = (geometry.column_width_pt - content_indent - value_col_pt).max(1.0);

    let fd_body    = resolver.resolve(FontRole::Body, FontWeight::Normal, FontStyle::Normal, None);
    let family: Rc<str> = Rc::from(resolver.resolve_family_name(FontRole::Body, None));
    let ascent_off = fd_body.ascender as f64 / fd_body.units_per_em as f64 * font_size;

    let item_height = font_size * line_spacing;

    for item in &sum.items {
        let row_top = local_y;

        // ── Checkbox ────────────────────────────────────────────────────────
        let checkbox_y = row_top + (item_height - checkbox_pt).max(0.0) * 0.5;
        frags.push(Fragment {
            x:      0.0,
            y:      checkbox_y,
            width:  checkbox_pt,
            height: checkbox_pt,
            kind:   FragmentKind::StrokedRect(StrokedRect {
                stroke_width: SUM_STROKE_PT,
                color:        "#000000".to_owned(),
                dash:         None,
            }),
        });

        // ── Item content (inline) ────────────────────────────────────────────
        let engine = InlineLayoutEngine {
            resolver,
            available_width: content_width,
            font_size,
            line_spacing,
            blank_default_cm,
            justify: false,
        };
        let (content_frags, content_h) =
            engine.layout(&item.content, FontRole::Body, &style, content_indent, row_top);
        frags.extend(content_frags);

        // ── Value label (right-aligned) ──────────────────────────────────────
        let value_text  = format!("{:02}", item.value);
        let glyphs      = shape_text(fd_body, &value_text);
        let value_w     = shaped_text_width(&glyphs, font_size, fd_body.units_per_em);
        let value_x     = geometry.column_width_pt - value_w;
        frags.push(Fragment {
            x:      value_x,
            y:      row_top,
            width:  value_w,
            height: font_size,
            kind:   FragmentKind::GlyphRun(GlyphRun::from_shaped(
                &glyphs, font_size, family.clone(), 0, Rc::from("#000000"), ascent_off,
            )),
        });

        local_y += content_h.max(item_height) + item_gap_pt;
    }

    // ── "Soma:" box ──────────────────────────────────────────────────────────
    if sum.show_sum_box {
        local_y += SUM_BOX_TOP_MARGIN_PT * spc;

        let box_width  = SUM_BOX_WIDTH_PT.min(geometry.column_width_pt);
        let box_height = SUM_BOX_HEIGHT_PT * spc;
        let box_x      = (geometry.column_width_pt - box_width).max(0.0);

        // Outlined rectangle.
        frags.push(Fragment {
            x:      box_x,
            y:      local_y,
            width:  box_width,
            height: box_height,
            kind:   FragmentKind::StrokedRect(StrokedRect {
                stroke_width: SUM_STROKE_PT,
                color:        "#000000".to_owned(),
                dash:         None,
            }),
        });

        // "Soma:" label inside the box.
        let label_text  = "Soma:".to_owned();
        let glyphs      = shape_text(fd_body, &label_text);
        let label_w     = shaped_text_width(&glyphs, font_size, fd_body.units_per_em);
        let label_y     = local_y + (box_height - font_size).max(0.0) * 0.5;
        frags.push(Fragment {
            x:      box_x + SUM_BOX_LABEL_PAD_PT,
            y:      label_y,
            width:  label_w,
            height: font_size,
            kind:   FragmentKind::GlyphRun(GlyphRun::from_shaped(
                &glyphs, font_size, family, 0, Rc::from("#000000"), ascent_off,
            )),
        });

        local_y += box_height;
    }

    let total_h = local_y - origin_y;
    (frags, total_h)
}
