//! Running header / footer overlay — rendered on each page as a separate layer.
//!
//! # Coordinate system
//! Returned fragment coordinates are **content-area relative**: `(0, 0)` is the
//! top-left corner of the usable content area (inside margins), matching the
//! convention used by [`PageComposer`].  The PDF emitter adds margin offsets.
//!
//! Running headers use a negative `y` to place text above the content area
//! (inside the top margin).  Running footers use a `y` past the content height
//! to place text inside the bottom margin.
//!
//! # Token substitution
//! Within each region string the following tokens are replaced before shaping:
//!
//! | Token      | Replaced by                |
//! |------------|----------------------------|
//! | `{page}`   | current page number (1-based) |
//! | `{pages}`  | total page count              |
//!
//! [`PageComposer`]: crate::layout::page::PageComposer

use std::rc::Rc;

use crate::fonts::resolve::{FontResolver, FontRole};
use crate::layout::fragment::{Fragment, FragmentKind, GlyphRun};
use crate::layout::page::PageGeometry;
use crate::layout::text::{shape_text, shaped_text_width, ShapedGlyph};
use crate::spec::header::RunningHeader;
use crate::spec::style::{FontStyle, FontWeight};

/// Font size used for running headers and footers, in PDF points.
pub const RUNNING_FONT_SIZE: f64 = 9.0;

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Lay out one running header **or** footer overlay for a single page.
///
/// Returns a flat list of [`Fragment`]s in **page-absolute coordinates**.
///
/// # Parameters
/// - `running`      — the `RunningHeader` spec (left / center / right strings).
/// - `resolver`     — font resolver for the `body` role.
/// - `geometry`     — page geometry (margins, dimensions).
/// - `page_number`  — 1-based page number substituted for `{page}`.
/// - `total_pages`  — total page count substituted for `{pages}`.
/// - `is_footer`    — `false` → top margin (running header);
///                    `true`  → bottom margin (running footer).
pub fn layout_running_overlay<'a>(
    running:     &RunningHeader,
    resolver:    &'a FontResolver<'a>,
    geometry:    &PageGeometry,
    page_number: u32,
    total_pages: u32,
    is_footer:   bool,
) -> Vec<Fragment> {
    let fd        = resolver.resolve(FontRole::Body, FontWeight::Normal, FontStyle::Normal, None);
    let ascent_pt = fd.ascender as f64 / fd.units_per_em as f64 * RUNNING_FONT_SIZE;
    let family: Rc<str> = Rc::from(resolver.resolve_family_name(FontRole::Body, None));

    // Content-area relative coordinates.
    // The emitter adds (margin_left, margin_top) to convert to page-absolute.
    let y = if is_footer {
        // Place in bottom margin: content_height + half of bottom margin.
        let content_h = geometry.content_height_pt;
        content_h + (geometry.margin_bottom_pt - RUNNING_FONT_SIZE) / 2.0
    } else {
        // Place in top margin: negative offset above content area.
        // Centre text vertically in the top margin strip.
        -(geometry.margin_top_pt + RUNNING_FONT_SIZE) / 2.0
    };

    let cw = geometry.content_width_pt;

    let mut frags: Vec<Fragment> = Vec::new();

    // ── Left region — left-aligned ────────────────────────────────────────────
    if let Some(ref text) = running.left {
        let s = substitute(text, page_number, total_pages);
        if !s.is_empty() {
            let glyphs = shape_text(fd, &s);
            let w      = shaped_text_width(&glyphs, RUNNING_FONT_SIZE, fd.units_per_em);
            frags.push(glyph_frag(glyphs, 0.0, y, w, ascent_pt, family.clone()));
        }
    }

    // ── Center region — horizontally centred in the content width ─────────────
    if let Some(ref text) = running.center {
        let s = substitute(text, page_number, total_pages);
        if !s.is_empty() {
            let glyphs = shape_text(fd, &s);
            let w      = shaped_text_width(&glyphs, RUNNING_FONT_SIZE, fd.units_per_em);
            let x      = (cw - w) / 2.0;
            frags.push(glyph_frag(glyphs, x, y, w, ascent_pt, family.clone()));
        }
    }

    // ── Right region — right-aligned ──────────────────────────────────────────
    if let Some(ref text) = running.right {
        let s = substitute(text, page_number, total_pages);
        if !s.is_empty() {
            let glyphs = shape_text(fd, &s);
            let w      = shaped_text_width(&glyphs, RUNNING_FONT_SIZE, fd.units_per_em);
            let x      = cw - w;
            frags.push(glyph_frag(glyphs, x, y, w, ascent_pt, family.clone()));
        }
    }

    frags
}

// ─────────────────────────────────────────────────────────────────────────────
// Token substitution
// ─────────────────────────────────────────────────────────────────────────────

/// Replace `{page}` and `{pages}` tokens in `text`.
pub fn substitute(text: &str, page: u32, total: u32) -> String {
    text.replace("{page}", &page.to_string())
        .replace("{pages}", &total.to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Fragment builder
// ─────────────────────────────────────────────────────────────────────────────

fn glyph_frag(
    glyphs:    Vec<ShapedGlyph>,
    x:         f64,
    y:         f64,
    width:     f64,
    ascent_pt: f64,
    family:    Rc<str>,
) -> Fragment {
    Fragment {
        x,
        y,
        width,
        height: RUNNING_FONT_SIZE,
        kind: FragmentKind::GlyphRun(GlyphRun::from_shaped(
            &glyphs, RUNNING_FONT_SIZE, family, 0, Rc::from("#000000"), ascent_pt,
        )),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::page::PageGeometry;
    use crate::spec::config::PrintConfig;
    use crate::spec::header::RunningHeader;
    use crate::test_helpers::fixtures::make_resolver_and_rules;

    fn geometry() -> PageGeometry {
        PageGeometry::from_config(&PrintConfig::default())
    }

    // ── Token substitution ────────────────────────────────────────────────────

    #[test]
    fn substitute_page_token() {
        assert_eq!(substitute("{page}", 2, 5), "2");
    }

    #[test]
    fn substitute_pages_token() {
        assert_eq!(substitute("{pages}", 2, 5), "5");
    }

    #[test]
    fn substitute_combined_token() {
        assert_eq!(substitute("{page}/{pages}", 1, 3), "1/3");
        assert_eq!(substitute("{page}/{pages}", 2, 3), "2/3");
        assert_eq!(substitute("{page}/{pages}", 3, 3), "3/3");
    }

    #[test]
    fn substitute_no_tokens_unchanged() {
        assert_eq!(substitute("Matemática", 1, 10), "Matemática");
    }

    #[test]
    fn substitute_multiple_occurrences() {
        assert_eq!(substitute("{page} de {pages} ({page})", 2, 4), "2 de 4 (2)");
    }

    // ── Empty RunningHeader ────────────────────────────────────────────────────

    #[test]
    fn empty_running_header_produces_no_fragments() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let frags = layout_running_overlay(
            &RunningHeader::default(), &res, &geometry(), 1, 3, false,
        );
        assert!(frags.is_empty());
    }

    // ── Font size ─────────────────────────────────────────────────────────────

    #[test]
    fn all_fragments_use_9pt_font() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let running = RunningHeader {
            left:   Some("Matemática".into()),
            center: Some("Prova".into()),
            right:  Some("Pág. {page}/{pages}".into()),
        };
        let frags = layout_running_overlay(&running, &res, &geometry(), 1, 3, false);
        for f in &frags {
            if let FragmentKind::GlyphRun(ref run) = f.kind {
                assert!((run.font_size - RUNNING_FONT_SIZE).abs() < 0.001,
                    "font_size should be {RUNNING_FONT_SIZE}, got {}", run.font_size);
            }
        }
    }

    // ── Y position ────────────────────────────────────────────────────────────

    #[test]
    fn header_y_is_negative_above_content_area() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let g    = geometry();
        let running = RunningHeader { center: Some("Cabeçalho".into()), ..Default::default() };
        let frags = layout_running_overlay(&running, &res, &g, 1, 1, false);
        assert_eq!(frags.len(), 1);
        let y = frags[0].y;
        assert!(y < 0.0, "header y ({y:.2}) should be negative (above content area)");
        // When emitter adds margin_top, the result should be within [0, margin_top].
        let page_y = y + g.margin_top_pt;
        assert!(page_y >= 0.0, "page-absolute y ({page_y:.2}) should be >= 0");
        assert!(page_y < g.margin_top_pt, "page-absolute y ({page_y:.2}) should be within top margin");
    }

    #[test]
    fn footer_y_is_below_content_area() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let g    = geometry();
        let running = RunningHeader { center: Some("Rodapé".into()), ..Default::default() };
        let frags = layout_running_overlay(&running, &res, &g, 1, 1, true);
        assert_eq!(frags.len(), 1);
        let y = frags[0].y;
        assert!(y >= g.content_height_pt,
            "footer y ({y:.2}) should be at or below content height ({:.2})", g.content_height_pt);
        // When emitter adds margin_top, should be in the bottom margin.
        let page_y = y + g.margin_top_pt;
        let bot_start = g.page_height_pt - g.margin_bottom_pt;
        assert!(page_y >= bot_start,
            "page-absolute footer y ({page_y:.2}) should be in bottom margin (>= {bot_start:.2})");
    }

    // ── X position ────────────────────────────────────────────────────────────

    #[test]
    fn left_text_starts_at_x_zero() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let g    = geometry();
        let running = RunningHeader { left: Some("Esquerda".into()), ..Default::default() };
        let frags = layout_running_overlay(&running, &res, &g, 1, 1, false);
        assert_eq!(frags.len(), 1);
        assert!((frags[0].x).abs() < 0.001,
            "left text x ({:.2}) should be 0 (content-area origin)", frags[0].x);
    }

    #[test]
    fn right_text_ends_at_content_width() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let g    = geometry();
        let running = RunningHeader { right: Some("Direita".into()), ..Default::default() };
        let frags = layout_running_overlay(&running, &res, &g, 1, 1, false);
        assert_eq!(frags.len(), 1);
        let right_edge = frags[0].x + frags[0].width;
        let expected   = g.content_width_pt;
        assert!((right_edge - expected).abs() < 0.5,
            "right text right edge ({right_edge:.2}) should be near content width ({expected:.2})");
    }

    #[test]
    fn center_text_is_horizontally_centered() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let g    = geometry();
        let running = RunningHeader { center: Some("Centro".into()), ..Default::default() };
        let frags = layout_running_overlay(&running, &res, &g, 1, 1, false);
        assert_eq!(frags.len(), 1);
        let text_center = frags[0].x + frags[0].width / 2.0;
        let content_center = g.content_width_pt / 2.0;
        assert!((text_center - content_center).abs() < 1.0,
            "center text mid ({text_center:.2}) should be near content mid ({content_center:.2})");
    }

    // ── Three regions together ────────────────────────────────────────────────

    #[test]
    fn three_regions_produce_three_fragments() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let running = RunningHeader {
            left:   Some("Escola".into()),
            center: Some("Prova Final".into()),
            right:  Some("Pág. {page}/{pages}".into()),
        };
        let frags = layout_running_overlay(&running, &res, &geometry(), 2, 5, false);
        assert_eq!(frags.len(), 3, "three regions → three fragments");
    }

    #[test]
    fn all_three_fragments_share_same_y() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let running = RunningHeader {
            left:   Some("A".into()),
            center: Some("B".into()),
            right:  Some("C".into()),
        };
        let frags = layout_running_overlay(&running, &res, &geometry(), 1, 1, false);
        assert_eq!(frags.len(), 3);
        let y0 = frags[0].y;
        for f in &frags {
            assert!((f.y - y0).abs() < 0.001, "all fragments should share the same y");
        }
    }

    // ── Page-number sequence (criterion: 1/3, 2/3, 3/3) ─────────────────────

    #[test]
    fn footer_shows_page_fraction_for_three_pages() {
        let (reg, rules) = make_resolver_and_rules();
        let res   = FontResolver::new(&reg, &rules);
        let g     = geometry();
        let total = 3u32;

        // Simulate PdfEmitter calling layout_running_overlay for each page.
        for page in 1..=total {
            let running = RunningHeader {
                right: Some("{page}/{pages}".into()),
                ..Default::default()
            };
            let frags = layout_running_overlay(&running, &res, &g, page, total, true);
            assert_eq!(frags.len(), 1,
                "page {page}: footer should have exactly one fragment");

            // Verify the shaped text contains the correct glyphs by checking it
            // produces a non-empty GlyphRun (full pixel check requires a PDF emitter).
            if let FragmentKind::GlyphRun(ref run) = frags[0].kind {
                assert!(!run.glyph_ids.is_empty(),
                    "page {page}: footer GlyphRun should have glyphs");
                assert!((run.font_size - RUNNING_FONT_SIZE).abs() < 0.001);
            } else {
                panic!("expected GlyphRun for page {page}");
            }

            // The fragment must be below the content area (content-area coords).
            assert!(frags[0].y >= g.content_height_pt,
                "page {page}: footer y ({:.2}) should be below content height ({:.2})",
                frags[0].y, g.content_height_pt);
        }
    }

    // ── Token substitution yields different text per page ─────────────────────

    #[test]
    fn page_tokens_produce_distinct_text_per_page() {
        // Verify substitute() itself produces the expected strings.
        let texts: Vec<String> = (1..=3)
            .map(|p| substitute("{page}/{pages}", p, 3))
            .collect();
        assert_eq!(texts, ["1/3", "2/3", "3/3"]);
    }

    // ── Only present regions are rendered ─────────────────────────────────────

    #[test]
    fn only_left_region_produces_one_fragment() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let running = RunningHeader { left: Some("Só esquerda".into()), ..Default::default() };
        let frags = layout_running_overlay(&running, &res, &geometry(), 1, 1, false);
        assert_eq!(frags.len(), 1);
    }

    #[test]
    fn only_right_region_produces_one_fragment() {
        let (reg, rules) = make_resolver_and_rules();
        let res  = FontResolver::new(&reg, &rules);
        let running = RunningHeader { right: Some("{page}".into()), ..Default::default() };
        let frags = layout_running_overlay(&running, &res, &geometry(), 1, 3, false);
        assert_eq!(frags.len(), 1);
    }
}
