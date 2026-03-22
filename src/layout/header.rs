//! Layout for `InstitutionalHeader` — rendered at the top of page 1.
//!
//! # Coordinate contract
//! All returned fragment coordinates are **relative to the top-left of the header block**
//! (y = 0 at the block top).  The caller (PageComposer) translates them to absolute
//! content-area coordinates via `push_block`.

use std::collections::HashMap;
use std::rc::Rc;

use crate::fonts::resolve::{FontResolver, FontRole};
use crate::layout::fragment::{Fragment, FragmentKind, GlyphRun, HRule, ImageFragment};
use crate::layout::inline::InlineLayoutEngine;
use crate::layout::page::PageGeometry;
use crate::layout::text::{shape_text, shaped_text_width};
use crate::spec::header::InstitutionalHeader;
use crate::spec::inline::{InlineContent, InlineText};
use crate::spec::style::{FontStyle, FontWeight, ResolvedStyle};

const CM_TO_PT: f64 = 28.3465;

/// Body font size in points — matches lize CSS `body { font-size: 0.875rem }`.
/// 0.875rem × 16px = 14px × 0.75 pt/px = 10.5pt.
/// Used for header text, student fields, instructions — NOT for question content.
pub(crate) const BODY_FONT_SIZE_PT: f64 = 10.5;

/// Default logo height when `logo_height_cm` is not specified.
const LOGO_DEFAULT_HEIGHT_CM: f64 = 2.0;
/// Fraction of the content width reserved for the logo column (matches lize CSS w-25).
const LOGO_COL_FRACTION: f64 = 0.25;
/// Padding inside the logo cell (matches lize CSS p-4 ≈ 1.5rem ≈ 18pt).
const LOGO_CELL_PAD_PT: f64 = 18.0;
/// Stroke thickness of table borders (matches lize CSS table-bordered ≈ 1px).
const TABLE_BORDER_PT: f64 = 0.75;
/// Table border color (rgba(72,94,144,0.16) on white ≈ #e1e5ea).
const TABLE_BORDER_COLOR: &str = "#e1e5ea";
/// Stroke thickness of the institutional separator rule.
const HRULE_THICKNESS_PT: f64 = 0.5;
/// Vertical margin above and below the separator rule.
const HRULE_V_MARGIN_PT: f64 = 4.0;
/// Top margin before the instructions block (matches lize mt-3 = 1rem = 16px ≈ 12pt).
const INSTRUCTIONS_TOP_MARGIN_PT: f64 = 12.0;
/// Cell vertical padding for table rows (Bootstrap .table td padding = 8px ≈ 6pt vertical).
const CELL_V_PAD_PT: f64 = 6.0;
/// Cell horizontal padding for table rows (Bootstrap .table td padding = 10px ≈ 7.5pt horizontal).
const CELL_H_PAD_PT: f64 = 7.5;

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Lay out `header` into a flat list of [`Fragment`]s.
///
/// `font_size`    — base font size in points (from `PrintConfig`).
/// `line_spacing` — line-height multiplier (from `PrintConfig`).
///
/// Returns `(fragments, total_height_pt)`.
pub fn layout_header<'a>(
    header:       &InstitutionalHeader,
    resolver:     &'a FontResolver<'a>,
    geometry:     &PageGeometry,
    _images:      &HashMap<String, Vec<u8>>,
    _font_size:   f64,
    line_spacing: f64,
) -> (Vec<Fragment>, f64) {
    let mut fragments: Vec<Fragment> = Vec::new();

    // Header text uses the fixed body font size (10.5pt), NOT the question
    // content font_size.  In lize HTML the header renders at 0.875rem (≈10.5pt)
    // while question content uses the user-configurable {{font_size}}pt.
    let font_size = BODY_FONT_SIZE_PT;
    let blank_cm  = crate::layout::inline::BLANK_DEFAULT_CM;
    let cw        = geometry.content_width_pt;

    // ── Table layout — matches lize HTML table-bordered structure ──────────
    // The logo column is always reserved (w-25) regardless of whether a logo
    // image is provided, so the header layout is consistent across all exams.
    let logo_col_w  = cw * LOGO_COL_FRACTION;
    let text_col_x  = logo_col_w;
    let text_col_w  = (cw - text_col_x).max(1.0);

    let mut cursor_y: f64 = 0.0;

    // ── Row 1: institution name (bold, centered, uppercase) ──────────────
    // Matches lize: font-weight-bold text-center text-uppercase
    let mut inst_row_h = CELL_V_PAD_PT * 2.0 + font_size;
    if let Some(ref institution) = header.institution {
        let inst_upper = institution.to_uppercase();
        let fd = resolver.resolve(FontRole::Heading, FontWeight::Bold, FontStyle::Normal, None);
        let glyphs    = shape_text(fd, &inst_upper);
        let text_w    = shaped_text_width(&glyphs, font_size, fd.units_per_em);
        let ascent_pt = fd.ascender as f64 / fd.units_per_em as f64 * font_size;
        let family    = Rc::from(resolver.resolve_family_name(FontRole::Heading, None));

        // Center text within the text column
        let cell_inner_w = text_col_w - CELL_H_PAD_PT * 2.0;
        let x_offset = ((cell_inner_w - text_w).max(0.0)) / 2.0;

        fragments.push(Fragment {
            x:      text_col_x + CELL_H_PAD_PT + x_offset,
            y:      cursor_y + CELL_V_PAD_PT,
            width:  text_w,
            height: font_size,
            kind:   FragmentKind::GlyphRun(GlyphRun::from_shaped(
                &glyphs, font_size, family, 1, Rc::from("#000000"), ascent_pt,
            )),
        });
        inst_row_h = CELL_V_PAD_PT * 2.0 + font_size;
    }
    push_cell_border(&mut fragments, text_col_x, cursor_y, text_col_w, inst_row_h);
    cursor_y += inst_row_h;

    // ── Row 2: "PROVA: <TITLE>" — label normal weight, title bold ────────
    // Matches lize: "PROVA: " + <span class="font-weight-bold text-uppercase">name</span>
    if let Some(ref title) = header.title {
        let fd_normal = resolver.resolve(FontRole::Body, FontWeight::Normal, FontStyle::Normal, None);
        let fd_bold   = resolver.resolve(FontRole::Body, FontWeight::Bold, FontStyle::Normal, None);
        let family: Rc<str> = Rc::from(resolver.resolve_family_name(FontRole::Body, None));
        let ascent_pt = fd_normal.ascender as f64 / fd_normal.units_per_em as f64 * font_size;

        let label = "PROVA: ";
        let label_glyphs = shape_text(fd_normal, label);
        let label_w      = shaped_text_width(&label_glyphs, font_size, fd_normal.units_per_em);

        let title_upper  = title.to_uppercase();
        let title_glyphs = shape_text(fd_bold, &title_upper);
        let title_w      = shaped_text_width(&title_glyphs, font_size, fd_bold.units_per_em);

        let x_base = text_col_x + CELL_H_PAD_PT;
        let y_base = cursor_y + CELL_V_PAD_PT;

        // "PROVA: " in normal weight
        fragments.push(Fragment {
            x: x_base, y: y_base, width: label_w, height: font_size,
            kind: FragmentKind::GlyphRun(GlyphRun::from_shaped(
                &label_glyphs, font_size, family.clone(), 0, Rc::from("#000000"), ascent_pt,
            )),
        });

        // Title in bold
        fragments.push(Fragment {
            x: x_base + label_w, y: y_base, width: title_w, height: font_size,
            kind: FragmentKind::GlyphRun(GlyphRun::from_shaped(
                &title_glyphs, font_size, family, 1, Rc::from("#000000"), ascent_pt,
            )),
        });

        let row_h = CELL_V_PAD_PT * 2.0 + font_size;
        push_cell_border(&mut fragments, text_col_x, cursor_y, text_col_w, row_h);
        cursor_y += row_h;
    }

    // ── Row: subject · year (if present) ──────────────────────────────────
    let subject_year = build_subject_year(&header.subject, &header.year);
    if !subject_year.is_empty() {
        let body_style = ResolvedStyle {
            font_size,
            line_spacing,
            ..ResolvedStyle::default()
        };
        let engine = InlineLayoutEngine {
            resolver,
            available_width:  text_col_w - CELL_H_PAD_PT * 2.0,
            font_size,
            line_spacing,
            blank_default_cm: blank_cm,
            justify: false,
        };
        let (frags, h) = engine.layout(
            &[text_inline(&subject_year)],
            FontRole::Body,
            &body_style,
            text_col_x + CELL_H_PAD_PT,
            cursor_y + CELL_V_PAD_PT,
        );
        fragments.extend(frags);
        let row_h = CELL_V_PAD_PT * 2.0 + h;
        push_cell_border(&mut fragments, text_col_x, cursor_y, text_col_w, row_h);
        cursor_y += row_h;
    }

    // ── Student fields — bordered table cells (matches lize HTML) ─────────
    // Each field renders as a bordered cell with normal-weight label.
    // Fields with width_cm share a row (equal-width cells).
    // Fields without width_cm get a full-width row.
    // No underlines — table cell borders serve as demarcation.
    if !header.student_fields.is_empty() {
        let fd = resolver.resolve(FontRole::Body, FontWeight::Normal, FontStyle::Normal, None);
        let ascent_pt   = fd.ascender as f64 / fd.units_per_em as f64 * font_size;
        let family_name: Rc<str> = Rc::from(resolver.resolve_family_name(FontRole::Body, None));
        let row_h       = CELL_V_PAD_PT * 2.0 + font_size;

        // Group consecutive fields with width_cm into shared rows.
        // Fields without width_cm get their own full-width row.
        let mut i = 0;
        let fields = &header.student_fields;
        while i < fields.len() {
            if fields[i].width_cm.is_some() {
                // Collect consecutive fields with width_cm
                let group_start = i;
                while i < fields.len() && fields[i].width_cm.is_some() {
                    i += 1;
                }
                let group = &fields[group_start..i];
                let n = group.len() as f64;
                let cell_w = text_col_w / n;

                for (j, field) in group.iter().enumerate() {
                    let cell_x = text_col_x + cell_w * j as f64;
                    let label_text = format!("{}:", field.label);
                    let label_glyphs = shape_text(fd, &label_text);
                    let label_w = shaped_text_width(&label_glyphs, font_size, fd.units_per_em);

                    fragments.push(Fragment {
                        x:      cell_x + CELL_H_PAD_PT,
                        y:      cursor_y + CELL_V_PAD_PT,
                        width:  label_w,
                        height: font_size,
                        kind:   FragmentKind::GlyphRun(GlyphRun::from_shaped(
                            &label_glyphs, font_size, family_name.clone(), 0, Rc::from("#000000"), ascent_pt,
                        )),
                    });
                    push_cell_border(&mut fragments, cell_x, cursor_y, cell_w, row_h);
                }
                cursor_y += row_h;
            } else {
                // Full-width row for this field
                let field = &fields[i];
                let label_text = format!("{}:", field.label);
                let label_glyphs = shape_text(fd, &label_text);
                let label_w = shaped_text_width(&label_glyphs, font_size, fd.units_per_em);

                fragments.push(Fragment {
                    x:      text_col_x + CELL_H_PAD_PT,
                    y:      cursor_y + CELL_V_PAD_PT,
                    width:  label_w,
                    height: font_size,
                    kind:   FragmentKind::GlyphRun(GlyphRun::from_shaped(
                        &label_glyphs, font_size, family_name.clone(), 0, Rc::from("#000000"), ascent_pt,
                    )),
                });
                push_cell_border(&mut fragments, text_col_x, cursor_y, text_col_w, row_h);
                cursor_y += row_h;
                i += 1;
            }
        }
    }

    // ── Logo cell — spans ALL rows (institution + title + student fields) ──
    // Matches lize CSS: w-25 p-4 rowspan="6". The cell border is always drawn
    // (even without a logo) to keep the header layout consistent.
    {
        let logo_h_pt = LOGO_DEFAULT_HEIGHT_CM * CM_TO_PT;
        let min_logo_cell_h = LOGO_CELL_PAD_PT * 2.0 + logo_h_pt;
        if cursor_y < min_logo_cell_h {
            cursor_y = min_logo_cell_h;
        }
        // Render logo image only when a key is provided.
        if let Some(ref logo_key) = header.logo_key {
            let avail_w = (logo_col_w - LOGO_CELL_PAD_PT * 2.0).max(1.0);
            let avail_h = (cursor_y - LOGO_CELL_PAD_PT * 2.0).max(1.0);
            let img_w   = avail_w;
            let img_h   = logo_h_pt.min(avail_h);
            let img_x   = LOGO_CELL_PAD_PT;
            let img_y   = LOGO_CELL_PAD_PT + (avail_h - img_h).max(0.0) / 2.0;
            fragments.push(Fragment {
                x: img_x, y: img_y, width: img_w, height: img_h,
                kind: FragmentKind::Image(ImageFragment { key: logo_key.clone() }),
            });
        }
        // Logo cell border spanning ALL rows — always present.
        push_cell_border(&mut fragments, 0.0, 0.0, logo_col_w, cursor_y);
    }

    // ── Instructions ──────────────────────────────────────────────────────────
    if !header.instructions.is_empty() {
        cursor_y += INSTRUCTIONS_TOP_MARGIN_PT;
        let style = ResolvedStyle {
            font_size,
            line_spacing,
            ..ResolvedStyle::default()
        };
        let engine = InlineLayoutEngine {
            resolver,
            available_width:  cw,
            font_size,
            line_spacing,
            blank_default_cm: blank_cm,
            justify: false,
        };
        let (frags, h) = engine.layout(
            &header.instructions,
            FontRole::Body,
            &style,
            0.0,
            cursor_y,
        );
        fragments.extend(frags);
        cursor_y += h;
    }

    // ── Final separator rule ──────────────────────────────────────────────────
    cursor_y += HRULE_V_MARGIN_PT;
    fragments.push(Fragment {
        x:      0.0,
        y:      cursor_y,
        width:  cw,
        height: HRULE_THICKNESS_PT,
        kind:   FragmentKind::HRule(HRule {
            stroke_width: HRULE_THICKNESS_PT,
            color:        "#000000".to_owned(),
        }),
    });
    cursor_y += HRULE_THICKNESS_PT + HRULE_V_MARGIN_PT;

    (fragments, cursor_y)
}

/// Push a stroked rect border for a table cell.
fn push_cell_border(fragments: &mut Vec<Fragment>, x: f64, y: f64, w: f64, h: f64) {
    fragments.push(Fragment {
        x, y, width: w, height: h,
        kind: FragmentKind::StrokedRect(crate::layout::fragment::StrokedRect {
            stroke_width: TABLE_BORDER_PT,
            color:        TABLE_BORDER_COLOR.to_owned(),
            dash:         None,
        }),
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Private helpers
// ─────────────────────────────────────────────────────────────────────────────

fn text_inline(s: &str) -> InlineContent {
    InlineContent::Text(InlineText { value: s.to_owned(), style: None })
}

fn build_subject_year(subject: &Option<String>, year: &Option<String>) -> String {
    match (subject.as_deref(), year.as_deref()) {
        (Some(s), Some(y)) => format!("{s} · {y}"),
        (Some(s), None)    => s.to_owned(),
        (None,    Some(y)) => y.to_owned(),
        (None,    None)    => String::new(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::fonts::resolve::FontResolver;
    use crate::layout::page::PageGeometry;
    use crate::spec::config::PrintConfig;
    use crate::spec::header::{InstitutionalHeader, StudentField};
    use crate::spec::inline::{InlineText, InlineContent};
    use crate::test_helpers::fixtures::make_resolver_and_rules;

    fn geometry() -> PageGeometry {
        PageGeometry::from_config(&PrintConfig::default())
    }

    fn no_images() -> HashMap<String, Vec<u8>> { HashMap::new() }

    fn call<'a>(
        header:   &InstitutionalHeader,
        resolver: &'a FontResolver<'a>,
    ) -> (Vec<Fragment>, f64) {
        layout_header(header, resolver, &geometry(), &no_images(), 15.0, 1.4)
    }

    // ── Helpers to inspect fragment types ─────────────────────────────────────

    fn count_hrules(frags: &[Fragment]) -> usize {
        frags.iter().filter(|f| matches!(f.kind, FragmentKind::HRule(_))).count()
    }

    fn count_images(frags: &[Fragment]) -> usize {
        frags.iter().filter(|f| matches!(f.kind, FragmentKind::Image(_))).count()
    }

    fn count_stroked_rects(frags: &[Fragment]) -> usize {
        frags.iter().filter(|f| matches!(f.kind, FragmentKind::StrokedRect(_))).count()
    }

    fn count_glyph_runs(frags: &[Fragment]) -> usize {
        frags.iter().filter(|f| matches!(f.kind, FragmentKind::GlyphRun(_))).count()
    }

    // ── Empty header ──────────────────────────────────────────────────────────

    #[test]
    fn empty_header_has_exactly_one_hrule() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let header = InstitutionalHeader::default();
        let (frags, _h) = call(&header, &res);
        assert_eq!(count_hrules(&frags), 1, "empty header should have exactly one HRule");
    }

    #[test]
    fn empty_header_height_is_positive() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let (_, h) = call(&InstitutionalHeader::default(), &res);
        assert!(h > 0.0, "even an empty header has a rule, so height must be > 0");
    }

    // ── Logo ──────────────────────────────────────────────────────────────────

    #[test]
    fn logo_key_produces_image_fragment() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let header = InstitutionalHeader {
            logo_key: Some("logo.png".into()),
            ..Default::default()
        };
        let (frags, _) = call(&header, &res);
        assert_eq!(count_images(&frags), 1, "one logo key → one ImageFragment");
    }

    #[test]
    fn logo_image_fragment_is_inside_logo_cell() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let header = InstitutionalHeader {
            logo_key: Some("logo.png".into()),
            ..Default::default()
        };
        let (frags, _) = call(&header, &res);
        let img = frags.iter().find(|f| matches!(f.kind, FragmentKind::Image(_))).unwrap();
        // Logo is inside a padded cell; x >= LOGO_CELL_PAD_PT
        assert!(img.x >= LOGO_CELL_PAD_PT - 0.001,
            "logo ImageFragment x should be >= LOGO_CELL_PAD_PT, got {}", img.x);
    }

    #[test]
    fn logo_default_height_is_2cm() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let header = InstitutionalHeader {
            logo_key: Some("logo.png".into()),
            ..Default::default()
        };
        let (frags, _) = call(&header, &res);
        let img = frags.iter().find(|f| matches!(f.kind, FragmentKind::Image(_))).unwrap();
        let expected = LOGO_DEFAULT_HEIGHT_CM * CM_TO_PT;
        assert!((img.height - expected).abs() < 0.001,
            "logo height should be {expected:.2} pt, got {:.2}", img.height);
    }

    #[test]
    fn no_logo_key_produces_no_image_fragment() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let (frags, _) = call(&InstitutionalHeader::default(), &res);
        assert_eq!(count_images(&frags), 0);
    }

    // ── Institutional text ────────────────────────────────────────────────────

    #[test]
    fn institution_text_produces_glyph_run() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let header = InstitutionalHeader {
            institution: Some("Escola Estadual".into()),
            ..Default::default()
        };
        let (frags, _) = call(&header, &res);
        assert!(count_glyph_runs(&frags) > 0, "institution text should produce at least one GlyphRun");
    }

    #[test]
    fn hrule_is_below_text_block() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let header = InstitutionalHeader {
            institution: Some("ENEM 2024".into()),
            ..Default::default()
        };
        let (frags, _) = call(&header, &res);
        let rule_y = frags.iter()
            .filter(|f| matches!(f.kind, FragmentKind::HRule(_)))
            .map(|f| f.y)
            .next()
            .unwrap();
        let text_max_y = frags.iter()
            .filter(|f| matches!(f.kind, FragmentKind::GlyphRun(_)))
            .map(|f| f.y + f.height)
            .fold(0.0_f64, f64::max);
        assert!(rule_y >= text_max_y,
            "HRule y ({rule_y:.2}) should be at or below text bottom ({text_max_y:.2})");
    }

    #[test]
    fn subject_year_joined_with_separator() {
        assert_eq!(build_subject_year(&Some("Matemática".into()), &Some("2024".into())),
                   "Matemática · 2024");
        assert_eq!(build_subject_year(&Some("Física".into()), &None), "Física");
        assert_eq!(build_subject_year(&None, &Some("2024".into())), "2024");
        assert_eq!(build_subject_year(&None, &None), "");
    }

    // ── Student fields ────────────────────────────────────────────────────────

    #[test]
    fn student_field_produces_glyph_run_and_bordered_cell() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let header = InstitutionalHeader {
            student_fields: vec![
                StudentField { label: "Nome".into(), width_cm: None },
            ],
            ..Default::default()
        };
        let (frags, _) = call(&header, &res);
        assert!(count_glyph_runs(&frags) > 0, "should have label GlyphRun");
        // Should have bordered cell (StrokedRect) instead of underline (FilledRect)
        assert!(count_stroked_rects(&frags) > 0, "should have bordered cell StrokedRect");
    }

    #[test]
    fn student_field_none_width_produces_full_width_cell() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let header = InstitutionalHeader {
            student_fields: vec![
                StudentField { label: "Nome".into(), width_cm: None },
            ],
            ..Default::default()
        };
        let (frags, _) = call(&header, &res);
        // The label GlyphRun should be within the content area
        let label = frags.iter()
            .filter(|f| matches!(f.kind, FragmentKind::GlyphRun(_)))
            .last()
            .unwrap();
        let g = geometry();
        assert!(label.x + label.width <= g.content_width_pt + 0.5,
            "field label should not exceed content width");
    }

    #[test]
    fn multiple_student_fields_with_widths_share_a_row() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let header = InstitutionalHeader {
            student_fields: vec![
                StudentField { label: "Nº".into(), width_cm: Some(3.0) },
                StudentField { label: "Turma".into(), width_cm: Some(3.0) },
            ],
            ..Default::default()
        };
        let (frags, _) = call(&header, &res);
        // Both labels should be on the same row (same y)
        let labels: Vec<&Fragment> = frags.iter()
            .filter(|f| matches!(f.kind, FragmentKind::GlyphRun(_)))
            .collect();
        assert!(labels.len() >= 2, "should have at least 2 label GlyphRuns");
        let last_two = &labels[labels.len()-2..];
        assert!((last_two[0].y - last_two[1].y).abs() < 0.001,
            "both fields should share the same row y");
    }

    // ── Instructions ──────────────────────────────────────────────────────────

    #[test]
    fn instructions_appear_below_student_fields() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let header = InstitutionalHeader {
            student_fields: vec![
                StudentField { label: "Nome".into(), width_cm: None },
            ],
            instructions: vec![
                InlineContent::Text(InlineText { value: "Leia com atenção.".into(), style: None }),
            ],
            ..Default::default()
        };
        let (frags, _) = call(&header, &res);
        // Should have at least 2 GlyphRuns (field label + instruction text)
        let runs: Vec<&Fragment> = frags.iter()
            .filter(|f| matches!(f.kind, FragmentKind::GlyphRun(_)))
            .collect();
        assert!(runs.len() >= 2, "should have field label + instruction GlyphRuns");
        // Instruction (last run) should be below the field label (first run)
        let field_run = runs[0];
        let instr_run = runs.last().unwrap();
        assert!(instr_run.y > field_run.y,
            "instruction y ({:.2}) should be below field y ({:.2})", instr_run.y, field_run.y);
    }

    // ── Full-header fixture ───────────────────────────────────────────────────

    #[test]
    fn full_header_produces_all_element_types() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);
        let header = InstitutionalHeader {
            institution:    Some("Secretaria de Educação".into()),
            title:          Some("Avaliação Bimestral".into()),
            subject:        Some("Matemática".into()),
            year:           Some("2024".into()),
            logo_key:       Some("logo.png".into()),
            student_fields: vec![
                StudentField { label: "Nome".into(), width_cm: None },
                StudentField { label: "Turma".into(), width_cm: Some(3.0) },
                StudentField { label: "Data".into(),  width_cm: Some(3.0) },
                StudentField { label: "Nota".into(),  width_cm: Some(2.5) },
            ],
            instructions:   vec![
                InlineContent::Text(InlineText {
                    value: "Responda com caneta azul ou preta.".into(),
                    style: None,
                }),
            ],
            ..Default::default()
        };

        let (frags, total_h) = call(&header, &res);

        assert!(total_h > 0.0,            "full header height should be positive");
        assert!(count_hrules(&frags)  == 1, "exactly one separator rule");
        assert!(count_images(&frags)  == 1, "exactly one logo");
        assert!(count_glyph_runs(&frags)  > 0, "should have text runs");
        assert!(count_stroked_rects(&frags) > 0, "should have bordered cells");

        // No fragment should exceed the content width
        let g = geometry();
        for f in &frags {
            // HRule spans full content width — skip exact check
            if matches!(f.kind, FragmentKind::HRule(_)) { continue; }
            assert!(f.x + f.width <= g.content_width_pt + 1.0,
                "fragment at x={:.2} w={:.2} exceeds content_width={:.2}",
                f.x, f.width, g.content_width_pt);
        }

        // All fragment y coordinates are non-negative
        for f in &frags {
            assert!(f.y >= 0.0, "fragment y should be >= 0, got {:.2}", f.y);
        }
    }

    #[test]
    fn total_height_grows_with_content() {
        let (reg, rules) = make_resolver_and_rules();
        let res = FontResolver::new(&reg, &rules);

        // Minimal header — only the logo cell minimum enforces its height.
        let (_, h_minimal) = call(&InstitutionalHeader::default(), &res);

        // Rich header — institution + exam title + several rows of student fields,
        // enough to exceed the logo-cell minimum height.
        let many_fields: Vec<StudentField> = (0..8)
            .map(|i| StudentField { label: format!("Campo {i}"), width_cm: None })
            .collect();
        let (_, h_rich) = call(&InstitutionalHeader {
            institution:    Some("Escola Estadual Exemplo".into()),
            title:          Some("Avaliação de Matemática — 2º Bimestre".into()),
            student_fields: many_fields,
            ..Default::default()
        }, &res);

        assert!(h_rich > h_minimal,
            "rich header ({h_rich:.2}) should be taller than minimal header ({h_minimal:.2})");
    }
}
