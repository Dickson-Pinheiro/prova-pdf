//! Middle panels of the answer sheet:
//! - "Orientações" bullet list + signature line (full content width, centered)
//! - grey fill-instructions strip with the vector Correto/Errado example

use crate::layout::fragment::{FilledCircle, Fragment, FragmentKind, StrokedCircle};
use crate::spec::answer_sheet::{
    AnswerSheetSpec, DEFAULT_FILL_INSTRUCTIONS, DEFAULT_SIGNATURE_LABEL,
};

use super::{
    filled_rect, SheetCtx, BODY_LINE_H, BUBBLE_GRAY, CONTENT_CX, CONTENT_X0, CONTENT_X1, NAVY,
    SIZE_BODY, SIZE_BUBBLE, SIZE_HEADER, STRIP_BG, STRIP_TOP, HAIR_W, WHITE,
};

// ── Orientations panel (full content width, x 23 → 573) ─────────────────────
// The matrícula grid was removed, so the panel spans the whole content width.
// The title and signature label are centered on CONTENT_CX; the bullet block
// keeps its left inset mirrored on the right, so it too is centered.

/// "Orientações" title glyph top.
const ORIENT_TITLE_TOP: f64 = 127.37;
/// First bullet line glyph top.
const BULLETS_TOP: f64 = 146.86;
/// Bullet text left edge and wrap width. The left inset (`BULLET_TEXT_X -
/// CONTENT_X0`) is mirrored on the right so the block is centered.
const BULLET_TEXT_X: f64 = 49.51;
const BULLET_TEXT_W: f64 = CONTENT_X1 - BULLET_TEXT_X - (BULLET_TEXT_X - CONTENT_X0);
/// Bullet dot: x, diameter, offset from the line's glyph top.
const BULLET_DOT_X: f64 = 42.23;
const BULLET_DOT_D: f64 = 2.08;
const BULLET_DOT_DY: f64 = 3.34;

// ── Signature (centered on the full content width) ───────────────────────────

const SIG_LINE_W: f64 = 271.36;
const SIG_LINE_X: f64 = CONTENT_CX - SIG_LINE_W / 2.0;
const SIG_LINE_Y: f64 = 280.16;
const SIG_LABEL_TOP: f64 = 282.54;

// ── Shared answer bubble metrics ─────────────────────────────────────────────

/// Bubble diameter and outline width (1px CSS). Used by the answers grid.
pub const BUBBLE_D: f64 = 9.36;
pub const BUBBLE_STROKE: f64 = 0.52;

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

/// Invisible white rects of the panel table (single full-width cell: borders +
/// background). White-on-white, kept for structural parity with the template.
const PANEL_WHITES: [(f64, f64, f64, f64); 4] = [
    (23.0, 116.93, 0.52, 178.83),    // left border
    (23.0, 116.93, 549.48, 0.52),    // top border (full width, 23 → 572.48)
    (572.48, 116.93, 0.52, 178.83),  // right border
    (23.52, 117.45, 549.48, 178.31), // cell background (full width)
];

pub(crate) fn layout_panels(spec: &AnswerSheetSpec, ctx: &SheetCtx<'_>, out: &mut Vec<Fragment>) {
    for (x, y, w, h) in PANEL_WHITES {
        out.push(filled_rect(x, y, w, h, WHITE));
    }
    layout_orientations(spec, ctx, out);
    layout_strip(spec, ctx, out);
}

// ─────────────────────────────────────────────────────────────────────────────
// Orientations + signature
// ─────────────────────────────────────────────────────────────────────────────

fn layout_orientations(spec: &AnswerSheetSpec, ctx: &SheetCtx<'_>, out: &mut Vec<Fragment>) {
    // Orientations are optional: when omitted the title and bullets are left
    // blank, but the reserved vertical space (and everything below) is kept.
    if !spec.orientations.is_empty() {
        out.push(ctx.text_centered(CONTENT_CX, ORIENT_TITLE_TOP, "Orientações", SIZE_HEADER, true, NAVY));

        // Bullets flow continuously (line pitch only, no inter-item spacing).
        let mut top = BULLETS_TOP;
        for item in &spec.orientations {
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
    }

    // Signature line + label — always drawn, centered across the full width.
    out.push(filled_rect(SIG_LINE_X, SIG_LINE_Y, SIG_LINE_W, HAIR_W, NAVY));
    let label = spec.signature_label.as_deref().unwrap_or(DEFAULT_SIGNATURE_LABEL);
    out.push(ctx.text_centered(CONTENT_CX, SIG_LABEL_TOP, label, SIZE_BODY, false, NAVY));
}

/// A stroked answer bubble at box top-left (`x`, `y`).
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
