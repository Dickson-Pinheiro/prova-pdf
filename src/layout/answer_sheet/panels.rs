//! Middle panels of the answer sheet:
//! - "Orientações" bullet list + signature line (left panel)
//! - "Matrícula" registration bubble grid (right panel)
//! - grey fill-instructions strip with the vector Correto/Errado example

use crate::layout::fragment::{FilledCircle, Fragment, FragmentKind, StrokedCircle};
use crate::spec::answer_sheet::{
    AnswerSheetSpec, DEFAULT_FILL_INSTRUCTIONS, DEFAULT_ORIENTATIONS, DEFAULT_SIGNATURE_LABEL,
};

use super::{
    filled_rect, SheetCtx, BODY_LINE_H, BUBBLE_GRAY, CONTENT_X0, CONTENT_X1, IDX_SEP, NAVY,
    SHADE, SIZE_BODY, SIZE_BUBBLE, SIZE_HEADER, STRIP_BG, STRIP_TOP, HAIR_W, WHITE,
};

// ── Orientations panel (x 23 → 422.24) ──────────────────────────────────────

/// Right edge of the orientations panel (the invisible cell border).
const ORIENT_X1: f64 = 422.24;
/// Panel horizontal center.
const ORIENT_CX: f64 = (CONTENT_X0 + ORIENT_X1) / 2.0;
/// "Orientações" title glyph top.
const ORIENT_TITLE_TOP: f64 = 127.37;
/// First bullet line glyph top.
const BULLETS_TOP: f64 = 146.86;
/// Bullet text left edge and wrap width.
const BULLET_TEXT_X: f64 = 49.51;
const BULLET_TEXT_W: f64 = 367.1;
/// Bullet dot: x, diameter, offset from the line's glyph top.
const BULLET_DOT_X: f64 = 42.23;
const BULLET_DOT_D: f64 = 2.08;
const BULLET_DOT_DY: f64 = 3.34;

// ── Signature ────────────────────────────────────────────────────────────────

const SIG_LINE_X: f64 = 86.94;
const SIG_LINE_W: f64 = 271.36;
const SIG_LINE_Y: f64 = 280.16;
const SIG_LABEL_TOP: f64 = 282.54;

// ── Registration (matrícula) panel (x 421.72 → 573) ─────────────────────────

const REG_X0: f64 = 421.72;
const REG_CX: f64 = (REG_X0 + CONTENT_X1) / 2.0;
const REG_TITLE_TOP: f64 = 123.21;
/// First bubble column left edge and horizontal pitch.
const REG_COL_X0: f64 = 439.66;
const REG_COL_PITCH: f64 = 11.783;
/// First bubble row top edge and vertical pitch.
const REG_ROW_Y0: f64 = 155.14;
const REG_ROW_PITCH: f64 = 13.7511;
/// Bubble diameter and outline width (1px CSS).
pub const BUBBLE_D: f64 = 9.36;
pub const BUBBLE_STROKE: f64 = 0.52;
/// Digit glyph top relative to its bubble top.
const REG_DIGIT_DY: f64 = 2.17;
/// Shaded-cell offsets relative to the bubble box.
const REG_SHADE_DX: f64 = -1.86;
const REG_SHADE_DY: f64 = -2.34;
const REG_SHADE_W: f64 = 11.96;
/// Invisible column-index row: white digits + grey separators (template
/// artifact preserved for scanner compatibility and snapshot parity).
const IDX_DIGIT_TOP: f64 = 143.48;
const IDX_SEP_Y: f64 = 141.88;
const IDX_SEP_H: f64 = 10.92;

// ── Fill-instructions strip ──────────────────────────────────────────────────

const STRIP_Y0: f64 = 299.92;
const STRIP_H: f64 = 41.07;
const STRIP_TEXT_X: f64 = 31.32;
const STRIP_TEXT_TOP: f64 = 311.13;
const STRIP_TEXT_W: f64 = 262.0;
/// Vector example area (the reference embeds a PNG in this box).
const EXAMPLE_X0: f64 = 397.29;
const EXAMPLE_ROW1_TOP: f64 = 308.9;
const EXAMPLE_ROW2_TOP: f64 = 322.4;
const EXAMPLE_BUBBLE_D: f64 = 7.8;
const EXAMPLE_PITCH: f64 = 9.9;
const EXAMPLE_LABEL_W: f64 = 26.0;

/// Invisible white rects of the panels table (cell backgrounds + borders),
/// verbatim from the reference snapshot (x, y, w, h).
const PANEL_WHITES: [(f64, f64, f64, f64); 7] = [
    (23.0, 116.93, 0.52, 178.83),
    (23.0, 116.93, 399.24, 0.52),
    (421.72, 116.93, 0.52, 178.83),
    (422.24, 116.93, 150.76, 0.52),
    (572.48, 116.93, 0.52, 178.83),
    (23.52, 117.45, 398.2, 178.31),
    (421.72, 117.45, 151.28, 178.31),
];

pub(crate) fn layout_panels(spec: &AnswerSheetSpec, ctx: &SheetCtx<'_>, out: &mut Vec<Fragment>) {
    for (x, y, w, h) in PANEL_WHITES {
        out.push(filled_rect(x, y, w, h, WHITE));
    }
    layout_orientations(spec, ctx, out);
    if let Some(ref reg) = spec.registration {
        layout_registration(reg, ctx, out);
    }
    layout_strip(spec, ctx, out);
}

// ─────────────────────────────────────────────────────────────────────────────
// Orientations + signature
// ─────────────────────────────────────────────────────────────────────────────

fn layout_orientations(spec: &AnswerSheetSpec, ctx: &SheetCtx<'_>, out: &mut Vec<Fragment>) {
    out.push(ctx.text_centered(ORIENT_CX, ORIENT_TITLE_TOP, "Orientações", SIZE_HEADER, true, NAVY));

    let default_items: Vec<String> = DEFAULT_ORIENTATIONS.iter().map(|s| s.to_string()).collect();
    let items: &[String] = if spec.orientations.is_empty() { &default_items } else { &spec.orientations };

    // Bullets flow continuously (line pitch only, no inter-item spacing).
    let mut top = BULLETS_TOP;
    for item in items {
        out.push(Fragment {
            x: BULLET_DOT_X,
            y: top + BULLET_DOT_DY,
            width: BULLET_DOT_D,
            height: BULLET_DOT_D,
            kind: FragmentKind::FilledCircle(FilledCircle { color: NAVY.to_owned() }),
        });
        let lines = ctx.paragraph_justified(
            BULLET_TEXT_X, top, BULLET_TEXT_W, item, SIZE_BODY, BODY_LINE_H, true, out,
        );
        top += lines as f64 * BODY_LINE_H;
    }

    // Signature line + label.
    out.push(filled_rect(SIG_LINE_X, SIG_LINE_Y, SIG_LINE_W, HAIR_W, NAVY));
    let label = spec.signature_label.as_deref().unwrap_or(DEFAULT_SIGNATURE_LABEL);
    out.push(ctx.text_centered(ORIENT_CX, SIG_LABEL_TOP, label, SIZE_BODY, false, NAVY));
}

// ─────────────────────────────────────────────────────────────────────────────
// Registration grid
// ─────────────────────────────────────────────────────────────────────────────

fn layout_registration(reg: &crate::spec::answer_sheet::RegistrationGrid, ctx: &SheetCtx<'_>, out: &mut Vec<Fragment>) {
    let digits = reg.digits as usize;
    if digits == 0 {
        return;
    }

    out.push(ctx.text_centered(REG_CX, REG_TITLE_TOP, &reg.label, SIZE_HEADER, true, NAVY));

    let col_cx = |k: usize| REG_COL_X0 + k as f64 * REG_COL_PITCH + BUBBLE_D / 2.0;

    // Invisible index row: one white digit per column + separators between.
    for k in 0..digits {
        let d = (k % 10).to_string();
        out.push(ctx.text_centered(col_cx(k), IDX_DIGIT_TOP, &d, SIZE_HEADER, false, WHITE));
        if k + 1 < digits {
            let sep_x = (col_cx(k) + col_cx(k + 1)) / 2.0 - HAIR_W / 2.0;
            out.push(filled_rect(sep_x, IDX_SEP_Y, HAIR_W, IDX_SEP_H, IDX_SEP));
        }
    }

    // Alternate-column shading (even columns), one cell per row.
    for k in (0..digits).step_by(2) {
        let x = REG_COL_X0 + k as f64 * REG_COL_PITCH + REG_SHADE_DX;
        for r in 0..10 {
            let y = REG_ROW_Y0 + r as f64 * REG_ROW_PITCH + REG_SHADE_DY;
            out.push(filled_rect(x, y, REG_SHADE_W, REG_ROW_PITCH, SHADE));
        }
    }

    // 10 rows × N columns of digit bubbles; row r carries digit r.
    for r in 0..10u32 {
        let y = REG_ROW_Y0 + r as f64 * REG_ROW_PITCH;
        let digit = r.to_string();
        for k in 0..digits {
            let x = REG_COL_X0 + k as f64 * REG_COL_PITCH;
            out.push(bubble(x, y, BUBBLE_GRAY));
            out.push(ctx.text_centered(col_cx(k), y + REG_DIGIT_DY, &digit, SIZE_BUBBLE, false, NAVY));
        }
    }
}

/// A stroked answer/registration bubble at box top-left (`x`, `y`).
pub(crate) fn bubble(x: f64, y: f64, color: &str) -> Fragment {
    Fragment {
        x,
        y,
        width: BUBBLE_D,
        height: BUBBLE_D,
        kind: FragmentKind::StrokedCircle(StrokedCircle {
            stroke_width: BUBBLE_STROKE,
            color: color.to_owned(),
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Fill-instructions strip + vector example
// ─────────────────────────────────────────────────────────────────────────────

fn layout_strip(spec: &AnswerSheetSpec, ctx: &SheetCtx<'_>, out: &mut Vec<Fragment>) {
    // Background in two halves + top hairline, exactly like the reference.
    let half = (CONTENT_X1 - CONTENT_X0) / 2.0;
    out.push(filled_rect(CONTENT_X0, STRIP_Y0, half, STRIP_H, STRIP_BG));
    out.push(filled_rect(CONTENT_X0 + half, STRIP_Y0, half, STRIP_H, STRIP_BG));
    out.push(filled_rect(CONTENT_X0, STRIP_Y0, half, HAIR_W, STRIP_TOP));
    out.push(filled_rect(CONTENT_X0 + half, STRIP_Y0, half, HAIR_W, STRIP_TOP));

    let text = spec.fill_instructions.as_deref().unwrap_or(DEFAULT_FILL_INSTRUCTIONS);
    ctx.paragraph_justified(
        STRIP_TEXT_X, STRIP_TEXT_TOP, STRIP_TEXT_W, text, SIZE_BODY, BODY_LINE_H, false, out,
    );

    if spec.show_fill_example {
        push_example(ctx, out);
    }
}

/// Vector recreation of the Correto/Errado marking example.
///
/// The reference embeds a raster image here; the vector version is a
/// deliberate divergence (documented in ANALYSIS.md).
fn push_example(ctx: &SheetCtx<'_>, out: &mut Vec<Fragment>) {
    let bubbles_x0 = EXAMPLE_X0 + EXAMPLE_LABEL_W;
    let d = EXAMPLE_BUBBLE_D;

    // Row 1 — "Correto": filled bubble + stroked B C D E.
    out.push(ctx.text(EXAMPLE_X0, EXAMPLE_ROW1_TOP + 1.2, "Correto", SIZE_BUBBLE, true, NAVY));
    out.push(Fragment {
        x: bubbles_x0,
        y: EXAMPLE_ROW1_TOP,
        width: d,
        height: d,
        kind: FragmentKind::FilledCircle(FilledCircle { color: "#000000".to_owned() }),
    });
    for (i, letter) in ["B", "C", "D", "E"].iter().enumerate() {
        let x = bubbles_x0 + (i + 1) as f64 * EXAMPLE_PITCH;
        out.push(example_bubble(x, EXAMPLE_ROW1_TOP));
        out.push(example_letter(ctx, x, EXAMPLE_ROW1_TOP, letter));
    }

    // Row 2 — "Errado": X, blob, check, strike-through, small dot.
    out.push(ctx.text(EXAMPLE_X0, EXAMPLE_ROW2_TOP + 1.2, "Errado", SIZE_BUBBLE, true, NAVY));
    for i in 0..5usize {
        let x = bubbles_x0 + i as f64 * EXAMPLE_PITCH;
        let y = EXAMPLE_ROW2_TOP;
        match i {
            0 => {
                // Circle with an X through it.
                out.push(example_bubble(x, y));
                push_line(x - 0.8, y - 0.8, x + d + 0.8, y + d + 0.8, out);
                push_line(x + d + 0.8, y - 0.8, x - 0.8, y + d + 0.8, out);
            }
            1 => {
                // Partial blob inside the circle.
                out.push(example_bubble(x, y));
                out.push(Fragment {
                    x: x + 1.8, y: y + 2.2, width: d - 3.2, height: d - 3.6,
                    kind: FragmentKind::FilledCircle(FilledCircle { color: "#000000".to_owned() }),
                });
            }
            2 => {
                // Check mark over the circle.
                out.push(example_bubble(x, y));
                push_line(x + 1.2, y + d / 2.0, x + d / 2.0, y + d - 1.0, out);
                push_line(x + d / 2.0, y + d - 1.0, x + d + 1.0, y - 1.0, out);
            }
            3 => {
                // Letter struck through by a horizontal line.
                out.push(example_bubble(x, y));
                out.push(example_letter(ctx, x, y, "D"));
                push_line(x - 1.5, y + d / 2.0, x + d + 1.5, y + d / 2.0, out);
            }
            _ => {
                // Small off-center dot.
                out.push(example_bubble(x, y));
                out.push(Fragment {
                    x: x + 2.6, y: y + 1.6, width: 3.4, height: 3.4,
                    kind: FragmentKind::FilledCircle(FilledCircle { color: "#000000".to_owned() }),
                });
            }
        }
    }
}

fn example_bubble(x: f64, y: f64) -> Fragment {
    Fragment {
        x,
        y,
        width: EXAMPLE_BUBBLE_D,
        height: EXAMPLE_BUBBLE_D,
        kind: FragmentKind::StrokedCircle(StrokedCircle {
            stroke_width: 0.8,
            color: BUBBLE_GRAY.to_owned(),
        }),
    }
}

fn example_letter(ctx: &SheetCtx<'_>, bx: f64, by: f64, letter: &str) -> Fragment {
    ctx.text_centered(bx + EXAMPLE_BUBBLE_D / 2.0, by + 1.6, letter, 4.6, false, NAVY)
}

/// A thin diagonal/horizontal stroke used by the "Errado" examples, emitted
/// as a rotated thin FilledRect approximation via many small rects would be
/// heavy — instead reuse HRule/VRule when axis-aligned, or a thin quad.
fn push_line(x0: f64, y0: f64, x1: f64, y1: f64, out: &mut Vec<Fragment>) {
    use crate::layout::fragment::HRule;
    if (y1 - y0).abs() < 0.01 {
        out.push(Fragment {
            x: x0.min(x1),
            y: y0 - 0.35,
            width: (x1 - x0).abs(),
            height: 0.7,
            kind: FragmentKind::HRule(HRule { stroke_width: 0.7, color: "#000000".to_owned() }),
        });
        return;
    }
    // Diagonal: approximate with short axis-aligned segments (stair-step of
    // filled rects). 8 steps at this scale (<10pt) is visually a clean line.
    const STEPS: usize = 8;
    let sx = (x1 - x0) / STEPS as f64;
    let sy = (y1 - y0) / STEPS as f64;
    let seg_len = (sx * sx + sy * sy).sqrt();
    for i in 0..STEPS {
        out.push(filled_rect(
            x0 + sx * i as f64,
            y0 + sy * i as f64,
            seg_len.max(sx.abs()) * 0.8,
            0.7,
            "#000000",
        ));
    }
}
