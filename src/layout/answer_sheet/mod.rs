//! Layout engine for the OMR answer sheet (folha de respostas / gabarito).
//!
//! Reproduces the lize "Folha de Respostas Avulsa" Chromium output.  Every
//! constant below was measured from the reference PDF snapshot — see
//! `tests/answer_sheet/ANALYSIS.md` for the extraction methodology.
//!
//! # Coordinate contract
//! Unlike the exam layout (block-flow via `PageComposer`), the answer sheet is
//! a **fixed template**: fragments are positioned in page-absolute points
//! (top-left origin) and emitted through a zero-margin [`PageGeometry`].

pub mod answers;
pub mod header;
pub mod marks;
pub mod panels;
pub mod qr;

use std::collections::HashMap;
use std::rc::Rc;

use crate::fonts::data::FontData;
use crate::fonts::resolve::{FontResolver, FontRole};
use crate::layout::fragment::{FilledRect, Fragment, FragmentKind, GlyphRun};
use crate::layout::text::shape_text;
use crate::spec::answer_sheet::AnswerSheetSpec;
use crate::spec::style::{FontStyle, FontWeight};

// ─────────────────────────────────────────────────────────────────────────────
// Palette (measured from the reference)
// ─────────────────────────────────────────────────────────────────────────────

/// Text color everywhere — lize navy.
pub const NAVY: &str = "#001737";
/// Table / box borders.
pub const BORDER_GRAY: &str = "#999999";
/// Bubble outline.
pub const BUBBLE_GRAY: &str = "#464646";
/// Alternating cell shading.
pub const SHADE: &str = "#eaedf3";
/// Fill-instructions strip background.
pub const STRIP_BG: &str = "#dedede";
/// Fill-instructions strip top hairline.
pub const STRIP_TOP: &str = "#485e90";
pub const WHITE: &str = "#ffffff";
pub const BLACK: &str = "#000000";

// ─────────────────────────────────────────────────────────────────────────────
// Global geometry (pt, page-absolute, top-left origin)
// ─────────────────────────────────────────────────────────────────────────────

/// Left edge of the content area.
pub const CONTENT_X0: f64 = 23.0;
/// Right edge of the content area.
pub const CONTENT_X1: f64 = 573.0;
/// Horizontal center of the content area.
pub const CONTENT_CX: f64 = (CONTENT_X0 + CONTENT_X1) / 2.0;
/// Content width.
pub const CONTENT_W: f64 = CONTENT_X1 - CONTENT_X0;

/// Thickness of table/box borders (2px CSS × 0.52).
pub const BORDER_W: f64 = 1.04;
/// Thickness of hairlines (1px CSS × 0.52).
pub const HAIR_W: f64 = 0.52;

// Font sizes (px CSS × 0.52).
pub const SIZE_TRACKING: f64 = 8.32;
pub const SIZE_HEADER: f64 = 7.28;
pub const SIZE_BODY: f64 = 7.62;
pub const SIZE_BUBBLE: f64 = 5.54;

/// Line height of body text (orientations).
pub const BODY_LINE_H: f64 = 11.4357;

/// Baseline-top of the tracking code line.
pub const TRACKING_TOP: f64 = 15.83;
/// Letter-spacing of the tracking code, in CSS px (0.05em at 16px).
const TRACKING_LETTER_SPACING_PX: f64 = 0.8;

/// Footer text top.
pub const FOOTER_TOP: f64 = 813.05;

/// One CSS pixel in points. Chromium lays the sheet out on a CSS-pixel grid
/// and quantizes glyph positions to it; all sizes/borders are px multiples.
pub const PX: f64 = 0.52;

// ─────────────────────────────────────────────────────────────────────────────
// Shared text helper
// ─────────────────────────────────────────────────────────────────────────────

/// Shared shaping context: resolves the body family once and shapes plain
/// strings into positioned `GlyphRun` fragments.
pub(crate) struct SheetCtx<'a> {
    pub resolver: &'a FontResolver<'a>,
    pub family:   Rc<str>,
}

impl<'a> SheetCtx<'a> {
    pub fn new(resolver: &'a FontResolver<'a>) -> Self {
        let family = Rc::from(resolver.resolve_family_name(FontRole::Body, None));
        Self { resolver, family }
    }

    pub fn font(&self, bold: bool) -> &'a FontData {
        let weight = if bold { FontWeight::Bold } else { FontWeight::Normal };
        self.resolver.resolve(FontRole::Body, weight, FontStyle::Normal, None)
    }

    /// Shape `text` with Chromium-print glyph metrics.
    ///
    /// Chromium lays text out with FreeType-hinted integer-pixel advances at
    /// an integer ppem: the CSS px size is rounded to the nearest whole px
    /// (7.62pt body = 14.65px → 15ppem) and every glyph's BASE advance
    /// becomes `round(hmtx × ppem / upem)` whole pixels; kerning adjustments
    /// are rounded to pixels separately, so typical small kerns vanish.
    /// This was reverse-derived from the reference sheet, where it reproduces
    /// every char x-position exactly (see ANALYSIS.md).
    ///
    /// `letter_spacing_px` is added to every advance (tracking-code line);
    /// `space_extra_px` is added to every space glyph (justification).
    /// Returns the adjusted glyph vectors and the total width in points.
    fn shape_quantized(
        &self,
        text: &str,
        size: f64,
        bold: bool,
        letter_spacing_px: f64,
        space_extra_px: f64,
    ) -> (QuantizedRun, f64) {
        let fd = self.font(bold);
        let glyphs = shape_text(fd, text);
        let upem = fd.units_per_em as f64;
        let ppem = (size / PX).round();
        let bytes = text.as_bytes();
        let face = fd.face();

        let mut run = QuantizedRun::with_capacity(glyphs.len());
        let mut total_px = 0.0_f64;

        for g in &glyphs {
            let ch = text[g.cluster as usize..].chars().next().unwrap_or('\u{0}');
            let is_space = bytes.get(g.cluster as usize) == Some(&b' ');
            // Split the shaped advance into base (hmtx) + kerning adjustment;
            // FreeType hints them to whole pixels independently (small kerns
            // round to zero).
            let base = face
                .as_ref()
                .and_then(|f| f.glyph_hor_advance(ttf_parser::GlyphId(g.glyph_id)))
                .map(|a| a as f64)
                .unwrap_or(g.x_advance as f64);
            let kern = g.x_advance as f64 - base;
            let mut hinted = (base * ppem / upem).round() + (kern * ppem / upem).round();
            for &(ov_bold, ov_ppem, ov_ch, ov_px) in &HINT_OVERRIDES {
                if bold == ov_bold && ppem == ov_ppem as f64 && ch == ov_ch {
                    hinted = ov_px;
                }
            }
            let adv_px = hinted
                + letter_spacing_px
                + if is_space { space_extra_px } else { 0.0 };
            total_px += adv_px;

            run.ids.push(g.glyph_id);
            run.advances.push((adv_px * PX / size * upem).round() as i32);
            run.x_offsets.push(g.x_offset);
            run.y_offsets.push(g.y_offset);
        }
        (run, total_px * PX)
    }

    /// Width of `text` at `size` pt after pixel-grid quantization.
    pub fn width(&self, text: &str, size: f64, bold: bool) -> f64 {
        self.shape_quantized(text, size, bold, 0.0, 0.0).1
    }

    fn run_fragment(
        &self,
        run: QuantizedRun,
        x: f64,
        top: f64,
        width: f64,
        size: f64,
        bold: bool,
        color: &str,
    ) -> Fragment {
        let fd = self.font(bold);
        let ascent_pt = fd.ascender as f64 / fd.units_per_em as f64 * size;
        Fragment {
            x,
            y: top,
            width,
            height: size,
            kind: FragmentKind::GlyphRun(GlyphRun {
                glyph_ids:       run.ids,
                x_advances:      run.advances,
                x_offsets:       run.x_offsets,
                y_offsets:       run.y_offsets,
                font_size:       size,
                font_family:     self.family.clone(),
                variant:         if bold { 1 } else { 0 },
                color:           Rc::from(color),
                baseline_offset: ascent_pt,
            }),
        }
    }

    /// Build a text fragment whose glyph tops sit at `top` (pdfplumber `top`).
    ///
    /// The baseline offset uses the font ascender, matching how pdfminer
    /// derives a char's top from the embedded font descriptor — so a
    /// fragment placed at `top` diff-matches a reference run at the same y.
    pub fn text(&self, x: f64, top: f64, text: &str, size: f64, bold: bool, color: &str) -> Fragment {
        let (run, w) = self.shape_quantized(text, size, bold, 0.0, 0.0);
        self.run_fragment(run, x, top, w, size, bold, color)
    }

    /// Text horizontally centered on `cx`.
    pub fn text_centered(&self, cx: f64, top: f64, text: &str, size: f64, bold: bool, color: &str) -> Fragment {
        let (run, w) = self.shape_quantized(text, size, bold, 0.0, 0.0);
        self.run_fragment(run, cx - w / 2.0, top, w, size, bold, color)
    }

    /// Tracking-code line: centered, with the template's 0.8px letter-spacing.
    pub fn text_tracking(&self, cx: f64, top: f64, text: &str, size: f64, color: &str) -> Fragment {
        let (run, w) = self.shape_quantized(text, size, false, TRACKING_LETTER_SPACING_PX, 0.0);
        self.run_fragment(run, cx - w / 2.0, top, w, size, false, color)
    }

    /// Greedy word-wrap of `text` into lines of at most `width` pt, with
    /// full justification (stretched spaces) on every line except the last —
    /// matching the reference's `text-align: justify`.  Each line is emitted
    /// as a single `GlyphRun` including real space glyphs, so PDF text
    /// extractors recover the words.  Returns the number of lines; the first
    /// line's glyph tops sit at `top`, advancing by `line_h` per line.
    pub fn paragraph_justified(
        &self,
        x: f64,
        top: f64,
        width: f64,
        text: &str,
        size: f64,
        line_h: f64,
        justify: bool,
        out: &mut Vec<Fragment>,
    ) -> usize {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            return 0;
        }

        // Greedy line breaking on quantized widths.
        let mut lines: Vec<String> = Vec::new();
        let mut current = words[0].to_owned();
        for word in &words[1..] {
            let candidate = format!("{current} {word}");
            if self.width(&candidate, size, false) > width {
                lines.push(current);
                current = (*word).to_owned();
            } else {
                current = candidate;
            }
        }
        lines.push(current);

        let n_lines = lines.len();
        for (li, line) in lines.into_iter().enumerate() {
            let ly = top + li as f64 * line_h;
            let is_last = li == n_lines - 1;
            let n_spaces = line.matches(' ').count();
            let mut extra_px = 0.0;
            if justify && !is_last && n_spaces > 0 {
                let nat_w = self.width(&line, size, false);
                extra_px = (width - nat_w) / PX / n_spaces as f64;
            }
            let (run, w) = self.shape_quantized(&line, size, false, 0.0, extra_px);
            out.push(self.run_fragment(run, x, ly, w, size, false, NAVY));
        }
        n_lines
    }
}

/// Hinted-advance overrides where TrueType instruction grid-fitting deviates
/// from linear rounding, measured from the Chromium reference:
/// regular 'V' at 14ppem renders 8px (linear = 8.526 → 9) and bold 'E' at
/// 14ppem renders 9px (linear = 8.498 → 8).  Format: (bold, ppem, char, px).
const HINT_OVERRIDES: [(bool, u32, char, f64); 2] = [
    (false, 14, 'V', 8.0),
    (true,  14, 'E', 9.0),
];

/// Glyph vectors of a pixel-grid-quantized run.
struct QuantizedRun {
    ids:       Vec<u16>,
    advances:  Vec<i32>,
    x_offsets: Vec<i32>,
    y_offsets: Vec<i32>,
}

impl QuantizedRun {
    fn with_capacity(n: usize) -> Self {
        Self {
            ids:       Vec::with_capacity(n),
            advances:  Vec::with_capacity(n),
            x_offsets: Vec::with_capacity(n),
            y_offsets: Vec::with_capacity(n),
        }
    }
}

/// A filled rectangle fragment (borders are drawn as filled rects, matching
/// the Chromium reference where CSS borders rasterise as fills, not strokes).
pub(crate) fn filled_rect(x: f64, y: f64, w: f64, h: f64, color: &str) -> Fragment {
    Fragment {
        x,
        y,
        width: w,
        height: h,
        kind: FragmentKind::FilledRect(FilledRect { color: color.to_owned() }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Lay out a complete answer sheet into pages of page-absolute fragments.
///
/// Page 1 carries the full template (tracking code, header, fiducial marks,
/// orientations, fill instructions, answers, footer).
/// If the answer grid overflows its column capacity, continuation pages
/// carry only the answers box (plus fiducials and footer).
pub fn layout_answer_sheet(
    spec:     &AnswerSheetSpec,
    resolver: &FontResolver<'_>,
    images:   &HashMap<String, Vec<u8>>,
) -> Vec<Vec<Fragment>> {
    let ctx = SheetCtx::new(resolver);
    let mut page1: Vec<Fragment> = Vec::new();

    // ── Page background (invisible; present in the Chromium reference) ────
    page1.push(filled_rect(23.0, 14.0, 550.0, 827.6, WHITE));

    // ── Tracking code ─────────────────────────────────────────────────────
    if let Some(ref code) = spec.tracking_code {
        page1.push(ctx.text_tracking(CONTENT_CX, TRACKING_TOP, code, SIZE_TRACKING, NAVY));
    }

    // ── Institutional header table (logo | fields | QR) ───────────────────
    header::layout_sheet_header(spec, &ctx, images, &mut page1);

    // ── Orientations + signature + fill-instructions strip ────────────────
    panels::layout_panels(spec, &ctx, &mut page1);

    // ── Answers box (may spill onto continuation pages) ───────────────────
    let mut pages = answers::layout_answers(spec, &ctx, page1);

    // ── Fiducial corner marks + footer on every page ──────────────────────
    // The marks are drawn last, on top of everything: the answers box paints a
    // white interior down to y≈811, which would otherwise cover the two bottom
    // marks (y≈793) and leave only the two top marks visible.
    for page in pages.iter_mut() {
        marks::push_fiducials(page);
        if let Some(ref footer) = spec.footer_text {
            page.push(ctx.text_centered(CONTENT_CX, FOOTER_TOP, footer, SIZE_HEADER, false, NAVY));
        }
    }

    pages
}
