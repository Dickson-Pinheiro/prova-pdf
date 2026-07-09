//! "Respostas" box: numbered rows of lettered answer bubbles.
//!
//! The template always reserves 5 bubble columns (A–E). When the exam has
//! fewer alternatives, the surplus bubbles and letters are painted in the
//! row background color — invisible but present, exactly like the Chromium
//! reference (see ANALYSIS.md §7).
//!
//! Rows wrap into vertical columns inside the box (`rows_per_column`); if
//! the box overflows, continuation pages repeat the box with only the grid.

use crate::layout::fragment::Fragment;
use crate::spec::answer_sheet::AnswerSheetSpec;

use super::panels::{bubble, BUBBLE_D};
use super::{
    filled_rect, SheetCtx, BORDER_GRAY, BORDER_W, BUBBLE_GRAY, CONTENT_CX, CONTENT_W,
    CONTENT_X0, CONTENT_X1, NAVY, SHADE, SIZE_BUBBLE, SIZE_HEADER, WHITE,
};

// ── Measured geometry ────────────────────────────────────────────────────────

/// Box vertical extent.
const BOX_Y0: f64 = 349.30;
const BOX_Y1: f64 = 811.45;
/// "Respostas" title glyph top.
const TITLE_TOP: f64 = 356.10;
/// First row top edge (shading cell top) and row pitch.
const ROW_Y0: f64 = 373.74;
const ROW_PITCH: f64 = 13.7756;
/// Number cell: left edge and width.
const NUM_CELL_X: f64 = 47.43;
const NUM_CELL_W: f64 = 19.23;
/// Bubble cells: first left edge, cell width, pitch between cells.
const CELL_X0: f64 = 66.66;
const CELL_W: f64 = 11.96;
const CELL_PITCH: f64 = 11.825;
/// Bubble box offsets inside the row/cell.
const BUBBLE_DX: f64 = 1.34;
const BUBBLE_DY: f64 = 2.36;
/// Glyph tops relative to the row top.
const NUM_TOP_DY: f64 = 4.0;
const LETTER_TOP_DY: f64 = 4.51;
/// Reserved bubble columns (template constant).
const RESERVED_COLS: usize = 5;
const LETTERS: [&str; RESERVED_COLS] = ["A", "B", "C", "D", "E"];
/// Horizontal stride between wrapped question columns.
const COLUMN_STRIDE: f64 = CONTENT_W / 4.0;

/// Lay out the answers box onto `page1`; returns all pages (continuation
/// pages are created when the grid exceeds the box capacity).
pub(crate) fn layout_answers(
    spec:  &AnswerSheetSpec,
    ctx:   &SheetCtx<'_>,
    page1: Vec<Fragment>,
) -> Vec<Vec<Fragment>> {
    let mut pages = vec![page1];

    let rows_per_col = spec.answers.rows_per_column.max(1) as usize;
    let max_cols = ((CONTENT_X1 - NUM_CELL_X) / COLUMN_STRIDE).floor() as usize; // 4
    let per_page = rows_per_col * max_cols;

    let count = spec.answers.count as usize;
    let visible = (spec.answers.alternatives as usize).clamp(1, RESERVED_COLS);

    // Box + title on page 1 even when there are zero questions.
    push_box(ctx, &mut pages[0]);

    for q in 0..count {
        let page_idx = q / per_page;
        while pages.len() <= page_idx {
            let mut page = Vec::new();
            push_box(ctx, &mut page);
            super::marks::push_fiducials(&mut page);
            pages.push(page);
        }
        let local = q % per_page;
        let col = local / rows_per_col;
        let row = local % rows_per_col;

        let x_off = col as f64 * COLUMN_STRIDE;
        let row_top = ROW_Y0 + row as f64 * ROW_PITCH;
        let shaded = row % 2 == 0;
        let number = spec.answers.start_number + q as u32;

        let out = &mut pages[page_idx];

        // Row shading: number cell + all reserved bubble cells.
        if shaded {
            out.push(filled_rect(NUM_CELL_X + x_off, row_top, NUM_CELL_W, ROW_PITCH, SHADE));
            for k in 0..RESERVED_COLS {
                out.push(filled_rect(
                    CELL_X0 + x_off + k as f64 * CELL_PITCH, row_top, CELL_W, ROW_PITCH, SHADE,
                ));
            }
        }

        // Question number (bold, centered in the number cell).
        out.push(ctx.text_centered(
            NUM_CELL_X + x_off + NUM_CELL_W / 2.0,
            row_top + NUM_TOP_DY,
            &number.to_string(),
            SIZE_BUBBLE,
            true,
            NAVY,
        ));

        // Bubbles A–E; hidden ones take the row background color.
        let bg = if shaded { SHADE } else { WHITE };
        for (k, letter) in LETTERS.iter().enumerate() {
            let hidden = k >= visible;
            let bx = CELL_X0 + x_off + k as f64 * CELL_PITCH + BUBBLE_DX;
            let by = row_top + BUBBLE_DY;
            let color = if hidden { bg } else { BUBBLE_GRAY };
            let letter_color = if hidden { bg } else { NAVY };
            out.push(bubble(bx, by, color));
            out.push(ctx.text_centered(
                bx + BUBBLE_D / 2.0,
                row_top + LETTER_TOP_DY,
                letter,
                SIZE_BUBBLE,
                false,
                letter_color,
            ));
        }
    }

    pages
}

/// Box borders (filled rects) + centered bold title.
fn push_box(ctx: &SheetCtx<'_>, out: &mut Vec<Fragment>) {
    let h = BOX_Y1 - BOX_Y0;
    // Invisible white interior (present in the Chromium reference).
    out.push(filled_rect(23.52, 349.82, 548.96, 461.11, super::WHITE));
    out.push(filled_rect(CONTENT_X0, BOX_Y0, CONTENT_W, BORDER_W, BORDER_GRAY));
    out.push(filled_rect(CONTENT_X0, BOX_Y1 - BORDER_W, CONTENT_W, BORDER_W, BORDER_GRAY));
    out.push(filled_rect(CONTENT_X0, BOX_Y0, BORDER_W, h, BORDER_GRAY));
    out.push(filled_rect(CONTENT_X1 - BORDER_W, BOX_Y0, BORDER_W, h, BORDER_GRAY));
    out.push(ctx.text_centered(CONTENT_CX, TITLE_TOP, "Respostas", SIZE_HEADER, true, NAVY));
}
