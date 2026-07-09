//! Answer-sheet institutional header: a bordered table with the logo on the
//! left, institution / fields / exam title in the middle, and the QR code on
//! the right.
//!
//! Reuses [`InstitutionalHeader`] from the exam spec.  Field mapping:
//! `student_fields[..n-1]` share the row above "PROVA:" (side by side) and
//! the last field gets the full-width bottom row — with
//! `[UNIDADE, TURMA, ALUNO]` this reproduces the lize reference exactly.

use std::collections::HashMap;

use crate::layout::fragment::{Fragment, FragmentKind, ImageFragment};
use crate::spec::answer_sheet::AnswerSheetSpec;

use super::{filled_rect, qr, SheetCtx, BORDER_GRAY, BORDER_W, NAVY, SIZE_HEADER};

// ── Measured geometry ────────────────────────────────────────────────────────

/// Table top edge.
pub const TABLE_Y0: f64 = 26.48;
/// Table bottom edge (bottom border spans TABLE_Y1 − BORDER_W → TABLE_Y1).
pub const TABLE_Y1: f64 = 108.61;
/// Logo cell: CONTENT_X0 → LOGO_X1.
const LOGO_X1: f64 = 130.61;
/// Middle column left edge (after the logo separator border).
const MID_X0: f64 = 131.65;
/// QR cell left edge (border at QR_X0 − BORDER_W → QR_X0).
const QR_X0: f64 = 496.06;
/// Split of the fields row (UNIDADE | TURMA).
const FIELD_SPLIT_X: f64 = 329.19;
/// Left padding of text inside middle cells.
const CELL_PAD: f64 = 5.18;
/// Glyph tops of the four text rows.
const ROW_TOPS: [f64; 4] = [33.28, 53.55, 73.82, 94.10];
/// Logo box: fixed height (40px CSS), aspect-fit, left edge at LOGO_X.
const LOGO_MAX_H: f64 = 20.79;
const LOGO_X: f64 = 28.2;
const LOGO_PAD: f64 = 4.16;

/// QR center, measured from the reference (1px left of the cell center).
const QR_CX: f64 = 534.015;
const QR_CY: f64 = 67.55;

/// Header-table border segments, verbatim from the reference snapshot
/// (x, y, w, h).  Chromium draws per-cell borders with subpixel snapping;
/// reproducing the exact segments gives byte-level snapshot parity.
const BORDER_SEGMENTS: [(f64, f64, f64, f64); 33] = [
    (23.0, 26.48, 1.04, 20.79),
    (23.0, 26.48, 108.13, 1.04),
    (130.61, 26.48, 1.04, 21.31),
    (131.13, 26.48, 0.52, 1.04),
    (131.65, 26.48, 198.06, 1.04),
    (329.71, 26.48, 166.35, 1.04),
    (495.02, 26.48, 1.04, 21.31),
    (496.06, 26.48, 76.94, 1.04),
    (571.96, 26.48, 1.04, 20.79),
    (130.61, 46.75, 1.04, 21.31),
    (131.65, 46.75, 198.58, 1.04),
    (329.19, 46.75, 166.87, 1.04),
    (23.0, 47.27, 1.04, 20.27),
    (571.96, 47.27, 1.04, 20.27),
    (329.19, 47.79, 1.04, 20.27),
    (495.02, 47.79, 1.04, 20.27),
    (130.61, 67.02, 1.04, 21.31),
    (131.65, 67.02, 198.58, 1.04),
    (330.23, 67.02, 164.79, 1.04),
    (495.02, 67.02, 1.04, 21.31),
    (23.0, 67.54, 1.04, 20.27),
    (571.96, 67.54, 1.04, 20.27),
    (130.61, 87.3, 1.04, 21.31),
    (131.65, 87.3, 198.06, 1.04),
    (329.71, 87.3, 165.31, 1.04),
    (495.02, 87.3, 1.04, 21.31),
    (23.0, 87.82, 1.04, 20.79),
    (571.96, 87.82, 1.04, 20.79),
    (23.0, 107.57, 108.13, 1.04),
    (131.13, 107.57, 0.52, 1.04),
    (131.65, 107.57, 198.06, 1.04),
    (329.71, 107.57, 165.31, 1.04),
    (495.02, 107.57, 77.98, 1.04),
];

pub(crate) fn layout_sheet_header(
    spec:   &AnswerSheetSpec,
    ctx:    &SheetCtx<'_>,
    images: &HashMap<String, Vec<u8>>,
    out:    &mut Vec<Fragment>,
) {
    let header = &spec.header;
    let table_h = TABLE_Y1 - TABLE_Y0;

    // ── Borders ────────────────────────────────────────────────────────────
    // Chromium rasterises each cell's CSS border as its own filled rect,
    // with per-cell subpixel snapping (segment lengths differ by ±0.52 and
    // tiny corner stubs appear).  The exact segmentation is replicated from
    // the reference snapshot — see tests/answer_sheet/ANALYSIS.md.
    for (x, y, w, h) in BORDER_SEGMENTS {
        out.push(filled_rect(x, y, w, h, BORDER_GRAY));
    }

    // ── Row 1: institution (bold, centered, uppercase) ────────────────────
    if let Some(ref inst) = header.institution {
        let cx = (MID_X0 + QR_X0 - BORDER_W) / 2.0;
        out.push(ctx.text_centered(cx, ROW_TOPS[0], &inst.to_uppercase(), SIZE_HEADER, true, NAVY));
    }

    // ── Row 2: side-by-side fields (all but the last student field) ───────
    let fields = &header.student_fields;
    let side_fields: &[crate::spec::header::StudentField] =
        if fields.len() >= 2 { &fields[..fields.len() - 1] } else { &fields[..] };
    if let Some(first) = side_fields.first() {
        out.push(ctx.text(
            MID_X0 + CELL_PAD, ROW_TOPS[1],
            &format!("{}:", first.label.to_uppercase()),
            SIZE_HEADER, false, NAVY,
        ));
    }
    if let Some(second) = side_fields.get(1) {
        out.push(ctx.text(
            FIELD_SPLIT_X + BORDER_W + CELL_PAD, ROW_TOPS[1],
            &format!("{}:", second.label.to_uppercase()),
            SIZE_HEADER, false, NAVY,
        ));
    }

    // ── Row 3: "PROVA: " + bold title ──────────────────────────────────────
    if let Some(ref title) = header.title {
        let label = "PROVA: ";
        let label_w = ctx.width(label, SIZE_HEADER, false);
        out.push(ctx.text(MID_X0 + CELL_PAD, ROW_TOPS[2], label, SIZE_HEADER, false, NAVY));
        out.push(ctx.text(MID_X0 + CELL_PAD + label_w, ROW_TOPS[2], &title.to_uppercase(), SIZE_HEADER, true, NAVY));
    }

    // ── Row 4: full-width last field (ALUNO) ───────────────────────────────
    if fields.len() >= 2 {
        let last = &fields[fields.len() - 1];
        out.push(ctx.text(
            MID_X0 + CELL_PAD, ROW_TOPS[3],
            &format!("{}:", last.label.to_uppercase()),
            SIZE_HEADER, false, NAVY,
        ));
    }

    // ── Logo: fixed height (40px), aspect-fit width, at the reference's
    // left padding, vertically centered in the cell ───────────────────────
    if let Some(ref logo_key) = header.logo_key {
        let max_w = LOGO_X1 - LOGO_X - LOGO_PAD;
        let (mut w, mut h) = (max_w, LOGO_MAX_H);
        if let Some((iw, ih)) = images.get(logo_key).and_then(|d| probe_dims(d)) {
            let aspect = iw as f64 / ih as f64;
            w = (LOGO_MAX_H * aspect).min(max_w);
            h = w / aspect;
        }
        let y = TABLE_Y0 + (table_h - h) / 2.0;
        out.push(Fragment {
            x: LOGO_X, y, width: w, height: h,
            kind: FragmentKind::Image(ImageFragment { key: logo_key.clone() }),
        });
    }

    // ── QR code, centered in the right cell ───────────────────────────────
    if let Some(payload) = spec.qr_payload() {
        qr::push_qr(&payload, QR_CX, QR_CY, out);
    }
}

/// Probe raster dimensions from image bytes (PNG/JPEG) for aspect-fitting.
#[cfg(feature = "images")]
fn probe_dims(data: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

#[cfg(not(feature = "images"))]
fn probe_dims(_data: &[u8]) -> Option<(u32, u32)> {
    None
}
