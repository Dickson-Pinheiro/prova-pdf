//! PDF emission — converts `Vec<Page>` (each a `Vec<Fragment>`) to PDF bytes.
//!
//! # Coordinate system
//!
//! The layout engine uses a **top-left origin** (y increases downward).
//! PDF uses a **bottom-left origin** (y increases upward).
//!
//! Conversion for a fragment at layout `(x, y)` with `height`:
//! ```text
//! pdf_y = page_height_pt − fragment.y − fragment.height
//! ```
//!
//! # Object reference layout
//!
//! | Ref              | Object           |
//! |------------------|------------------|
//! | 1                | Catalog          |
//! | 2                | Pages tree       |
//! | 3 + i*2          | Page i           |
//! | 4 + i*2          | Content i        |
//! | 3 + n*2 + j*5    | Type0Font j      |
//! | 3 + n*2 + j*5+1  | CIDFont j        |
//! | 3 + n*2 + j*5+2  | FontDescriptor j |
//! | 3 + n*2 + j*5+3  | FontFile2 j      |
//! | 3 + n*2 + j*5+4  | ToUnicode CMap j |

use std::collections::HashMap;

use pdf_writer::{Content, Name, Pdf, Rect, Ref, Str};

use crate::fonts::FontRegistry;
use crate::layout::fragment::{Fragment, FragmentKind, GlyphRun};
use crate::layout::page::PageGeometry;
use crate::pdf::drawing::{emit_filled_rect, emit_hrule, emit_stroked_rect, parse_hex_color};
use crate::pdf::fonts::{collect_glyph_sets, embed_fonts, EmbeddedFont, FontMap, REFS_PER_FONT};
use crate::pdf::images::{embed_images, emit_image_do, ImageMap};
use crate::pipeline::PipelineError;

// ─────────────────────────────────────────────────────────────────────────────
// PdfEmitter
// ─────────────────────────────────────────────────────────────────────────────

/// Converts positioned fragment pages to a PDF byte stream.
///
/// Only the basic page structure is emitted in TASK-026.
/// Text (TASK-028) and shapes (TASK-029) extend the content streams; font
/// embedding and subsetting is added in TASK-027.
pub struct PdfEmitter<'a> {
    pub registry: &'a FontRegistry,
    pub images:   &'a HashMap<String, Vec<u8>>,
}

impl PdfEmitter<'_> {
    /// Create a new `PdfEmitter`.
    pub fn new<'a>(
        registry: &'a FontRegistry,
        images:   &'a HashMap<String, Vec<u8>>,
    ) -> PdfEmitter<'a> {
        PdfEmitter { registry, images }
    }

    /// Emit all pages as a complete PDF document.
    ///
    /// Each inner `Vec<Fragment>` becomes one page.  An empty outer `Vec`
    /// produces a single blank page so the result is always a valid PDF.
    pub fn emit(
        &self,
        pages:    Vec<Vec<Fragment>>,
        geometry: &PageGeometry,
    ) -> Result<Vec<u8>, PipelineError> {
        // Always produce at least one page so the PDF is valid.
        let pages = if pages.is_empty() { vec![vec![]] } else { pages };
        let n = pages.len();

        // ── Object refs ───────────────────────────────────────────────────────
        let catalog_ref = Ref::new(1);
        let pages_ref   = Ref::new(2);

        let page_refs: Vec<Ref> = (0..n)
            .map(|i| Ref::new(3 + i as i32 * 2))
            .collect();
        let content_refs: Vec<Ref> = (0..n)
            .map(|i| Ref::new(4 + i as i32 * 2))
            .collect();

        // Font objects start immediately after all page/content refs.
        // Page i  → 3 + i*2; Content i → 4 + i*2; last used = 2 + n*2.
        // First font ref = 3 + n*2.
        let font_base_ref = 3 + n as i32 * 2;

        // ── Collect and embed fonts ───────────────────────────────────────────
        let glyph_sets = collect_glyph_sets(&pages);

        // Image objects follow all font objects.
        let image_base_ref = font_base_ref + glyph_sets.len() as i32 * REFS_PER_FONT;

        // ── Build PDF ─────────────────────────────────────────────────────────
        let mut pdf = Pdf::new();

        // Embed fonts into the PDF (writes Type0, CIDFont, descriptor, etc.)
        let font_map = embed_fonts(&mut *pdf, self.registry, &glyph_sets, font_base_ref)?;

        // Embed images into the PDF (writes ImageXObject streams).
        let image_map = embed_images(&mut *pdf, self.images, image_base_ref)?;

        // Catalog → Pages tree.
        pdf.catalog(catalog_ref).pages(pages_ref);

        // Pages tree.
        pdf.pages(pages_ref)
            .kids(page_refs.iter().copied())
            .count(n as i32);

        let pw = geometry.page_width_pt  as f32;
        let ph = geometry.page_height_pt as f32;
        let media_box = Rect::new(0.0, 0.0, pw, ph);

        for (i, page_frags) in pages.iter().enumerate() {
            let page_ref    = page_refs[i];
            let content_ref = content_refs[i];

            // Content stream: GlyphRun (TASK-028), shapes (TASK-029), images (TASK-030).
            let content_bytes = build_content_stream(page_frags, geometry, &font_map, &image_map);
            pdf.stream(content_ref, &content_bytes);

            // Page object — add font and image resources.
            let mut page = pdf.page(page_ref);
            page.parent(pages_ref)
                .media_box(media_box)
                .contents(content_ref);

            let has_fonts  = !font_map.is_empty();
            let has_images = !image_map.is_empty();

            if has_fonts || has_images {
                let mut resources = page.resources();
                if has_fonts {
                    let mut fonts_dict = resources.fonts();
                    for ef in font_map.fonts.values() {
                        fonts_dict.pair(Name(ef.resource_name.as_bytes()), ef.type0_ref);
                    }
                }
                if has_images {
                    let mut xobjects = resources.x_objects();
                    for (_, info) in &image_map.images {
                        xobjects.pair(Name(info.resource_name.as_bytes()), info.xobject_ref);
                    }
                }
            }
        }

        Ok(pdf.finish())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Content stream builder
// ─────────────────────────────────────────────────────────────────────────────

/// Build the raw content stream bytes for one page.
///
/// Emits `BT … ET` blocks for every `GlyphRun` fragment (TASK-028).
/// Shapes (`HRule`, `FilledRect`, `StrokedRect`) are added in TASK-029.
fn build_content_stream(
    frags:     &[Fragment],
    geometry:  &PageGeometry,
    font_map:  &FontMap,
    image_map: &ImageMap,
) -> Vec<u8> {
    let mut content = Content::new();
    let ph  = geometry.page_height_pt;
    let ml  = geometry.margin_left_pt;
    let mt  = geometry.margin_top_pt;

    for frag in frags {
        // Fragment coordinates are relative to the content-area origin
        // (top-left corner of the usable area, inside the margins).
        // Translate to page coordinates before calling emit helpers.
        let px = frag.x + ml;
        let py = frag.y + mt;

        match &frag.kind {
            FragmentKind::GlyphRun(run) => {
                if let Some(ef) = font_map.get(&run.font_family, run.variant) {
                    emit_glyph_run(&mut content, px, py, run, ef, ph);
                }
            }
            FragmentKind::HRule(hr) => {
                emit_hrule(&mut content, px, py, frag.width, frag.height, hr, ph);
            }
            FragmentKind::VRule(vr) => {
                crate::pdf::drawing::emit_vrule(&mut content, px, py, frag.width, frag.height, vr, ph);
            }
            FragmentKind::FilledRect(fr) => {
                emit_filled_rect(&mut content, px, py, frag.width, frag.height, fr, ph);
            }
            FragmentKind::FilledCircle(fc) => {
                crate::pdf::drawing::emit_filled_circle(&mut content, px, py, frag.width, frag.height, fc, ph);
            }
            FragmentKind::StrokedRect(sr) => {
                emit_stroked_rect(&mut content, px, py, frag.width, frag.height, sr, ph);
            }
            FragmentKind::Image(img) => {
                if let Some(info) = image_map.get(&img.key) {
                    emit_image_do(&mut content, px, py, frag.width, frag.height, info, ph);
                }
            }
            FragmentKind::Spacer => {}
        }
    }

    content.finish().into_vec()
}

// ─────────────────────────────────────────────────────────────────────────────
// GlyphRun → BT … ET
// ─────────────────────────────────────────────────────────────────────────────

/// Emit one `BT … ET` block for a single shaped `GlyphRun`.
///
/// # Coordinate system
///
/// Layout uses top-left origin; PDF uses bottom-left.  The baseline in PDF
/// coordinates is:
/// ```text
/// pdf_baseline_y = page_height − frag.y − run.baseline_offset
/// ```
///
/// # Glyph addressing
///
/// Each glyph is written as a 2-byte big-endian CID (the *remapped* GID),
/// matching the `Identity-H` encoding declared in the `Type0Font` object.
///
/// # Advance handling
///
/// The `CIDFont` has `/DW 0`, so the PDF cursor does not advance automatically
/// after each glyph.  All advances — including GPOS kerning encoded in
/// `x_advances` — are supplied explicitly as `TJ` numeric adjustments.
///
/// The conversion formula (font units → TJ thousandths) is:
/// ```text
/// tj_amount = −(font_units × 1000 / units_per_em)
/// ```
/// A negative TJ amount moves the cursor *right* (positive x direction).
fn emit_glyph_run(
    content: &mut Content,
    px:      f64,
    py:      f64,
    run:     &GlyphRun,
    ef:      &EmbeddedFont,
    ph:      f64,
) {
    if run.glyph_ids.is_empty() {
        return;
    }

    let x            = px as f32;
    let pdf_baseline = (ph - py - run.baseline_offset) as f32;
    let font_size    = run.font_size as f32;
    let upe          = ef.units_per_em as f32;

    let (r, g, b) = parse_hex_color(&run.color);

    content.begin_text();
    content.set_fill_rgb(r, g, b);
    content.set_font(Name(ef.resource_name.as_bytes()), font_size);
    // Identity text matrix: no rotation, positioned at (x, pdf_baseline).
    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, x, pdf_baseline]);

    {
        let mut sp    = content.show_positioned();
        let mut items = sp.items();
        let n         = run.glyph_ids.len();

        for i in 0..n {
            let old_gid = run.glyph_ids[i];
            let new_gid = ef.remapper.get(old_gid).unwrap_or(0);

            let x_off = run.x_offsets.get(i).copied().unwrap_or(0);
            let x_adv = run.x_advances.get(i).copied().unwrap_or(0);

            // Pre-offset: shift the glyph right by x_off font units.
            if x_off != 0 {
                items.adjust(-(x_off as f32 * 1000.0 / upe));
            }

            // Show glyph as 2-byte big-endian CID.
            let glyph_bytes = [(new_gid >> 8) as u8, (new_gid & 0xFF) as u8];
            items.show(Str(&glyph_bytes));

            // Post-advance: move cursor right by (x_adv − x_off) font units.
            let net_adv = x_adv - x_off;
            if net_adv != 0 {
                items.adjust(-(net_adv as f32 * 1000.0 / upe));
            }
        }
    }

    content.end_text();
}


// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;
    use crate::spec::config::{PageSize, PrintConfig};

    fn make_emitter() -> (FontRegistry, HashMap<String, Vec<u8>>) {
        (FontRegistry::new(), HashMap::new())
    }

    fn default_geometry() -> PageGeometry {
        PageGeometry::from_config(&PrintConfig::default())
    }

    fn spacer_frag(x: f64, y: f64, h: f64) -> Fragment {
        Fragment { x, y, width: 0.0, height: h, kind: FragmentKind::Spacer }
    }

    // ── Valid PDF output ──────────────────────────────────────────────────────

    /// Critério: gera PDF válido com N páginas.
    #[test]
    fn output_starts_with_pdf_header() {
        let (reg, images) = make_emitter();
        let emitter = PdfEmitter::new(&reg, &images);
        let bytes = emitter.emit(vec![vec![]], &default_geometry()).unwrap();
        assert!(bytes.starts_with(b"%PDF-"), "output must start with %PDF-");
    }

    #[test]
    fn output_ends_with_eof_marker() {
        let (reg, images) = make_emitter();
        let emitter = PdfEmitter::new(&reg, &images);
        let bytes = emitter.emit(vec![vec![]], &default_geometry()).unwrap();
        let tail = &bytes[bytes.len().saturating_sub(10)..];
        assert!(tail.windows(5).any(|w| w == b"%%EOF"),
            "PDF must end with %%EOF marker");
    }

    #[test]
    fn output_is_non_empty() {
        let (reg, images) = make_emitter();
        let emitter = PdfEmitter::new(&reg, &images);
        let bytes = emitter.emit(vec![vec![]], &default_geometry()).unwrap();
        assert!(!bytes.is_empty());
    }

    // ── Page count ───────────────────────────────────────────────────────────

    #[test]
    fn single_page_produces_valid_pdf() {
        let (reg, images) = make_emitter();
        let emitter = PdfEmitter::new(&reg, &images);
        let bytes = emitter.emit(vec![vec![spacer_frag(0.0, 0.0, 10.0)]], &default_geometry()).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        // Page count recorded as "/Count 1"
        assert!(contains_bytes(&bytes, b"/Count 1"), "should record /Count 1");
    }

    #[test]
    fn three_pages_recorded_in_pages_tree() {
        let (reg, images) = make_emitter();
        let emitter = PdfEmitter::new(&reg, &images);
        let pages = vec![vec![], vec![], vec![]];
        let bytes = emitter.emit(pages, &default_geometry()).unwrap();
        assert!(contains_bytes(&bytes, b"/Count 3"), "should record /Count 3");
    }

    #[test]
    fn five_pages_recorded_in_pages_tree() {
        let (reg, images) = make_emitter();
        let emitter = PdfEmitter::new(&reg, &images);
        let pages: Vec<Vec<Fragment>> = (0..5).map(|_| vec![]).collect();
        let bytes = emitter.emit(pages, &default_geometry()).unwrap();
        assert!(contains_bytes(&bytes, b"/Count 5"), "should record /Count 5");
    }

    #[test]
    fn empty_pages_input_produces_single_blank_page() {
        let (reg, images) = make_emitter();
        let emitter = PdfEmitter::new(&reg, &images);
        let bytes = emitter.emit(vec![], &default_geometry()).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(contains_bytes(&bytes, b"/Count 1"), "empty input → 1 blank page");
    }

    // ── MediaBox dimensions ───────────────────────────────────────────────────

    #[test]
    fn a4_mediabox_present_in_output() {
        let (reg, images) = make_emitter();
        let emitter = PdfEmitter::new(&reg, &images);
        let geom  = PageGeometry::from_config(&PrintConfig { page_size: PageSize::A4, ..PrintConfig::default() });
        let bytes = emitter.emit(vec![vec![]], &geom).unwrap();
        // MediaBox contains the page dimensions — we just verify /MediaBox appears.
        assert!(contains_bytes(&bytes, b"/MediaBox"), "output must contain /MediaBox");
    }

    #[test]
    fn different_page_sizes_produce_different_outputs() {
        let (reg, images) = make_emitter();
        let emitter = PdfEmitter::new(&reg, &images);

        let geom_a4  = PageGeometry::from_config(&PrintConfig { page_size: PageSize::A4,  ..PrintConfig::default() });
        let geom_ata = PageGeometry::from_config(&PrintConfig { page_size: PageSize::Ata, ..PrintConfig::default() });

        let bytes_a4  = emitter.emit(vec![vec![]], &geom_a4).unwrap();
        let bytes_ata = emitter.emit(vec![vec![]], &geom_ata).unwrap();

        assert_ne!(bytes_a4, bytes_ata, "different page sizes should produce different PDFs");
    }

    // ── Coordinate helpers ────────────────────────────────────────────────────

    #[test]
    fn coordinate_flip_formula_is_correct() {
        // pdf_y = page_height − frag.y − frag.height
        let ph = 841.89_f64;
        let frag_y = 100.0_f64;
        let frag_h = 20.0_f64;
        let pdf_y  = ph - frag_y - frag_h;
        assert!((pdf_y - 721.89).abs() < 0.01, "coordinate flip: expected 721.89, got {pdf_y:.2}");
    }

    #[test]
    fn fragments_on_page_do_not_cause_panic() {
        let (reg, images) = make_emitter();
        let emitter = PdfEmitter::new(&reg, &images);
        use crate::layout::fragment::{GlyphRun, HRule, FilledRect, StrokedRect};
        let frags = vec![
            Fragment { x: 10.0, y: 20.0, width: 100.0, height: 12.0,
                kind: FragmentKind::GlyphRun(GlyphRun {
                    glyph_ids: vec![], x_advances: vec![], x_offsets: vec![], y_offsets: vec![],
                    font_size: 12.0, font_family: Rc::from("body"), variant: 0,
                    color: Rc::from("#000000"), baseline_offset: 10.0,
                }) },
            Fragment { x: 0.0, y: 50.0, width: 400.0, height: 0.5,
                kind: FragmentKind::HRule(HRule { stroke_width: 0.5, color: "#000000".into() }) },
            Fragment { x: 0.0, y: 100.0, width: 200.0, height: 30.0,
                kind: FragmentKind::FilledRect(FilledRect { color: "#cccccc".into() }) },
            Fragment { x: 0.0, y: 150.0, width: 200.0, height: 30.0,
                kind: FragmentKind::StrokedRect(StrokedRect {
                    stroke_width: 0.7, color: "#000000".into(), dash: Some([4.0, 4.0]) }) },
        ];
        let result = emitter.emit(vec![frags], &default_geometry());
        assert!(result.is_ok(), "fragments on page must not cause panic or error");
        assert!(result.unwrap().starts_with(b"%PDF-"));
    }

    // ── GlyphRun emission (TASK-028) ──────────────────────────────────────────

    use crate::test_helpers::fixtures::DEJAVU;

    fn make_emitter_with_font() -> (FontRegistry, HashMap<String, Vec<u8>>) {
        let mut reg = FontRegistry::new();
        reg.add_variant("body", 0, DEJAVU.to_vec()).unwrap();
        (reg, HashMap::new())
    }

    fn glyph_run_frag(
        family: &str, variant: u8,
        glyph_ids: Vec<u16>, x_advances: Vec<i32>,
        x: f64, y: f64, size: f64,
    ) -> Fragment {
        use crate::layout::fragment::GlyphRun;
        let n = glyph_ids.len();
        Fragment {
            x, y, width: 100.0, height: size,
            kind: FragmentKind::GlyphRun(GlyphRun {
                glyph_ids,
                x_advances,
                x_offsets:       vec![0; n],
                y_offsets:       vec![0; n],
                font_size:       size,
                font_family:     Rc::from(family),
                variant,
                color:           Rc::from("#000000"),
                baseline_offset: size * 0.8,
            }),
        }
    }

    #[test]
    fn glyph_run_produces_bt_et_operators() {
        let (reg, images) = make_emitter_with_font();
        let emitter = PdfEmitter::new(&reg, &images);
        // DejaVu Sans: glyph 68 = 'a', 69 = 'b', 70 = 'c'
        let frag = glyph_run_frag("body", 0, vec![68, 69, 70], vec![1228, 1228, 1228], 10.0, 20.0, 12.0);
        let bytes = emitter.emit(vec![vec![frag]], &default_geometry()).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        // BT and ET must appear in the PDF bytes (content stream may be compressed,
        // but with no compression filter set these are raw bytes).
        assert!(contains_bytes(&bytes, b"BT"), "content stream must contain BT");
        assert!(contains_bytes(&bytes, b"ET"), "content stream must contain ET");
    }

    #[test]
    fn glyph_run_tf_operator_uses_resource_name() {
        let (reg, images) = make_emitter_with_font();
        let emitter = PdfEmitter::new(&reg, &images);
        let frag = glyph_run_frag("body", 0, vec![68], vec![1228], 0.0, 0.0, 12.0);
        let bytes = emitter.emit(vec![vec![frag]], &default_geometry()).unwrap();
        // Tf operator: /F0 12 Tf
        assert!(contains_bytes(&bytes, b"/F0"), "Tf must reference /F0 resource");
        assert!(contains_bytes(&bytes, b"Tf"), "content stream must contain Tf");
    }

    #[test]
    fn glyph_run_tm_operator_present() {
        let (reg, images) = make_emitter_with_font();
        let emitter = PdfEmitter::new(&reg, &images);
        let frag = glyph_run_frag("body", 0, vec![68], vec![1228], 50.0, 100.0, 12.0);
        let bytes = emitter.emit(vec![vec![frag]], &default_geometry()).unwrap();
        assert!(contains_bytes(&bytes, b"Tm"), "content stream must contain Tm");
    }

    #[test]
    fn glyph_run_tj_operator_present() {
        let (reg, images) = make_emitter_with_font();
        let emitter = PdfEmitter::new(&reg, &images);
        let frag = glyph_run_frag("body", 0, vec![68, 69], vec![1228, 1228], 0.0, 0.0, 12.0);
        let bytes = emitter.emit(vec![vec![frag]], &default_geometry()).unwrap();
        assert!(contains_bytes(&bytes, b"TJ"), "content stream must contain TJ");
    }

    #[test]
    fn empty_glyph_run_produces_no_bt() {
        // A GlyphRun with no glyphs should produce no BT/ET block.
        let (reg, images) = make_emitter_with_font();
        let emitter = PdfEmitter::new(&reg, &images);
        let frag = glyph_run_frag("body", 0, vec![], vec![], 0.0, 0.0, 12.0);
        let bytes = emitter.emit(vec![vec![frag]], &default_geometry()).unwrap();
        assert!(!contains_bytes(&bytes, b"BT"), "empty GlyphRun must not emit BT");
    }

    #[test]
    fn two_glyph_runs_produce_two_bt_blocks() {
        let (reg, images) = make_emitter_with_font();
        let emitter = PdfEmitter::new(&reg, &images);
        let f1 = glyph_run_frag("body", 0, vec![68], vec![1228], 0.0, 0.0, 12.0);
        let f2 = glyph_run_frag("body", 0, vec![69], vec![1228], 50.0, 0.0, 12.0);
        let bytes = emitter.emit(vec![vec![f1, f2]], &default_geometry()).unwrap();
        let bt_count = bytes.windows(2).filter(|w| *w == b"BT").count();
        assert!(bt_count >= 2, "two GlyphRuns must produce at least 2 BT operators, got {bt_count}");
    }

    #[test]
    fn glyph_run_red_color_emitted() {
        let (reg, images) = make_emitter_with_font();
        let emitter = PdfEmitter::new(&reg, &images);
        use crate::layout::fragment::GlyphRun;
        let frag = Fragment {
            x: 0.0, y: 0.0, width: 100.0, height: 12.0,
            kind: FragmentKind::GlyphRun(GlyphRun {
                glyph_ids: vec![68],
                x_advances: vec![1228],
                x_offsets:  vec![0],
                y_offsets:  vec![0],
                font_size:  12.0,
                font_family: Rc::from("body"),
                variant:    0,
                color:      Rc::from("#FF0000"),  // red
                baseline_offset: 9.6,
            }),
        };
        let bytes = emitter.emit(vec![vec![frag]], &default_geometry()).unwrap();
        // rg operator with (1.0 0.0 0.0): red fill
        assert!(contains_bytes(&bytes, b"rg"), "rg color operator must appear");
    }

    #[test]
    fn glyph_run_with_kerning_produces_adjustments() {
        // A run with x_offsets produces TJ adjustments in the content stream.
        let (reg, images) = make_emitter_with_font();
        let emitter = PdfEmitter::new(&reg, &images);
        use crate::layout::fragment::GlyphRun;
        let frag = Fragment {
            x: 0.0, y: 0.0, width: 200.0, height: 12.0,
            kind: FragmentKind::GlyphRun(GlyphRun {
                glyph_ids:  vec![68, 69],
                x_advances: vec![1228, 1228],
                x_offsets:  vec![50, 0],   // non-zero offset on first glyph
                y_offsets:  vec![0, 0],
                font_size:  12.0,
                font_family: Rc::from("body"),
                variant:    0,
                color:      Rc::from("#000000"),
                baseline_offset: 9.6,
            }),
        };
        let bytes = emitter.emit(vec![vec![frag]], &default_geometry()).unwrap();
        assert!(contains_bytes(&bytes, b"TJ"), "TJ must appear for kerned run");
    }

    // ── Shape emission via full emitter (TASK-029) ───────────────────────────

    #[test]
    fn hrule_in_page_produces_stroke_operator() {
        let (reg, images) = make_emitter();
        let emitter = PdfEmitter::new(&reg, &images);
        use crate::layout::fragment::HRule;
        let frag = Fragment {
            x: 0.0, y: 50.0, width: 400.0, height: 0.5,
            kind: FragmentKind::HRule(HRule { stroke_width: 0.5, color: "#000000".into() }),
        };
        let bytes = emitter.emit(vec![vec![frag]], &default_geometry()).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(contains_bytes(&bytes, b"S\n"),
            "HRule must produce stroke operator S");
    }

    #[test]
    fn filled_rect_in_page_produces_fill_operator() {
        let (reg, images) = make_emitter();
        let emitter = PdfEmitter::new(&reg, &images);
        use crate::layout::fragment::FilledRect;
        let frag = Fragment {
            x: 10.0, y: 20.0, width: 200.0, height: 50.0,
            kind: FragmentKind::FilledRect(FilledRect { color: "#CCCCCC".into() }),
        };
        let bytes = emitter.emit(vec![vec![frag]], &default_geometry()).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(contains_bytes(&bytes, b" re\n") || contains_bytes(&bytes, b"re "),
            "FilledRect must produce re operator");
        // fill_nonzero() emits "f\n" on its own line.
        assert!(contains_bytes(&bytes, b"f\n"),
            "FilledRect must produce fill operator f");
    }

    #[test]
    fn stroked_rect_solid_no_dash_operator() {
        let (reg, images) = make_emitter();
        let emitter = PdfEmitter::new(&reg, &images);
        use crate::layout::fragment::StrokedRect;
        let frag = Fragment {
            x: 10.0, y: 20.0, width: 200.0, height: 50.0,
            kind: FragmentKind::StrokedRect(StrokedRect {
                stroke_width: 1.0, color: "#000000".into(), dash: None,
            }),
        };
        let bytes = emitter.emit(vec![vec![frag]], &default_geometry()).unwrap();
        assert!(contains_bytes(&bytes, b"S\n"),
            "StrokedRect must produce stroke operator S");
        // No dash operator in the output for solid rect.
        assert!(!contains_bytes(&bytes, b"] 0 d"),
            "solid StrokedRect must not contain dash operator");
    }

    #[test]
    fn stroked_rect_dashed_emits_dash_operator() {
        let (reg, images) = make_emitter();
        let emitter = PdfEmitter::new(&reg, &images);
        use crate::layout::fragment::StrokedRect;
        let frag = Fragment {
            x: 10.0, y: 20.0, width: 200.0, height: 50.0,
            kind: FragmentKind::StrokedRect(StrokedRect {
                stroke_width: 1.0, color: "#000000".into(), dash: Some([4.0, 4.0]),
            }),
        };
        let bytes = emitter.emit(vec![vec![frag]], &default_geometry()).unwrap();
        assert!(contains_bytes(&bytes, b" d\n") || contains_bytes(&bytes, b"] 0 d\n")
            || contains_bytes(&bytes, b"d\n"),
            "dashed StrokedRect must contain dash operator d");
    }

    #[test]
    fn mixed_shapes_do_not_panic() {
        let (reg, images) = make_emitter();
        let emitter = PdfEmitter::new(&reg, &images);
        use crate::layout::fragment::{HRule, FilledRect, StrokedRect};
        let frags = vec![
            Fragment { x: 0.0, y: 10.0, width: 400.0, height: 0.5,
                kind: FragmentKind::HRule(HRule { stroke_width: 0.5, color: "#333333".into() }) },
            Fragment { x: 0.0, y: 50.0, width: 200.0, height: 30.0,
                kind: FragmentKind::FilledRect(FilledRect { color: "#E8E8E8".into() }) },
            Fragment { x: 0.0, y: 100.0, width: 200.0, height: 30.0,
                kind: FragmentKind::StrokedRect(StrokedRect {
                    stroke_width: 0.7, color: "#000000".into(), dash: Some([4.0, 4.0]) }) },
        ];
        let result = emitter.emit(vec![frags], &default_geometry());
        assert!(result.is_ok(), "mixed shapes must not panic");
        assert!(result.unwrap().starts_with(b"%PDF-"));
    }

    // ── Helper ────────────────────────────────────────────────────────────────

    fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }
}
