use std::rc::Rc;

use crate::fonts::resolve::{FontResolver, FontRole};
use crate::layout::fragment::{FilledCircle, FilledRect, Fragment, FragmentKind, GlyphRun, HRule};
use crate::layout::inline::InlineLayoutEngine;
use crate::layout::text::{shape_text, shaped_text_width};
use crate::spec::answer::{AlternativeLayout, ChoiceAnswer};
use crate::spec::config::{LetterCase, PrintConfig};
use crate::spec::inline::{InlineContent, InlineText};
use crate::spec::style::{FontStyle, FontWeight, ResolvedStyle};

use super::{ColumnGeometry, ALT_BADGE_GAP_PT, ALT_BADGE_SCALE};

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Number of columns used when `AlternativeLayout::Horizontal` (grid) is requested.
pub(super) const GRID_COLUMNS: usize = 2;

/// Stripe background color for odd-indexed alternatives (matches lize CSS table-striped).
const ALT_STRIPE_COLOR: &str = "#F3F4F7";
/// Border-bottom color between alternatives.
const ALT_BORDER_COLOR: &str = "#C2C2C2";
/// Border-bottom stroke width (matches lize CSS: 1.5px ≈ 1.125pt).
const ALT_BORDER_STROKE_PT: f64 = 1.125;

/// Muted color for the "Alternativas da questão N" label (matches lize CSS text-muted).
const ALT_LABEL_COLOR: &str = "#6c757d";
/// Scale factor for the alternatives label font size relative to question font_size.
const ALT_LABEL_FONT_SCALE: f64 = 0.75;

// ─────────────────────────────────────────────────────────────────────────────
// Functions
// ─────────────────────────────────────────────────────────────────────────────

pub(super) fn layout_choice<'a>(
    choice:          &ChoiceAnswer,
    number:          u32,
    resolver:        &'a FontResolver<'a>,
    geometry:        &ColumnGeometry,
    font_size:       f64,
    line_spacing:    f64,
    blank_default_cm: f64,
    origin_y:        f64,
    config:          &PrintConfig,
    spc:             f64,
) -> (Vec<Fragment>, f64) {
    let mut frags   = Vec::new();
    let mut local_y = origin_y;

    // ── "Alternativas da questão N" label ────────────────────────────────────
    {
        let label_text = format!("Alternativas da questão {}", number);
        let label_size = font_size * ALT_LABEL_FONT_SCALE;
        let fd     = resolver.resolve(FontRole::Body, FontWeight::Normal, FontStyle::Normal, None);
        let glyphs = shape_text(fd, &label_text);
        let text_w = shaped_text_width(&glyphs, label_size, fd.units_per_em);
        let ascent = fd.ascender as f64 / fd.units_per_em as f64 * label_size;
        let family = Rc::from(resolver.resolve_family_name(FontRole::Body, None));

        frags.push(Fragment {
            x:      0.0,
            y:      local_y,
            width:  text_w,
            height: label_size,
            kind:   FragmentKind::GlyphRun(GlyphRun::from_shaped(
                &glyphs, label_size, family, 0, Rc::from(ALT_LABEL_COLOR), ascent,
            )),
        });
        local_y += label_size * line_spacing + 4.0 * spc;
    }

    let alt_spacing_pt = config.alternative_spacing_cm * 28.3465 * spc;
    let style = ResolvedStyle { font_size, line_spacing, ..ResolvedStyle::default() };
    let use_badges = !config.remove_color_alternatives;

    // Badge diameter: same ratio as question badge (matches lize CSS question-alternative)
    let badge_diam   = (font_size * ALT_BADGE_SCALE).max(16.0);
    let badge_margin = 3.0 * spc;

    match choice.layout {
        AlternativeLayout::Vertical => {
            for (idx, alt) in choice.alternatives.iter().enumerate() {
                let letter = format_alt_letter(&alt.label, idx, config.letter_case);

                if !use_badges {
                    // Plain text mode: "a) " prefix
                    let prefix  = format!("{letter}) ");
                    let content = build_alt_content(&prefix, &alt.content);
                    let engine  = InlineLayoutEngine {
                        resolver,
                        available_width: geometry.column_width_pt,
                        font_size, line_spacing, blank_default_cm,
            justify: false,
                    };
                    let (f, h) = engine.layout(&content, FontRole::Body, &style, 0.0, local_y);
                    frags.extend(f);
                    local_y += h + alt_spacing_pt;
                    continue;
                }

                // ── Badge mode ──────────────────────────────────────────────
                let row_start = local_y;
                let row_pad   = alt_spacing_pt / 2.0;

                // Pre-compute text to know row height
                let content_x  = badge_margin + badge_diam + ALT_BADGE_GAP_PT;
                let text_width = (geometry.column_width_pt - content_x).max(1.0);
                let engine = InlineLayoutEngine {
                    resolver,
                    available_width: text_width,
                    font_size, line_spacing, blank_default_cm,
            justify: false,
                };
                let text_y = row_start + row_pad + (badge_diam - font_size * line_spacing).max(0.0) / 2.0;
                let (text_frags, text_h) = engine.layout(&alt.content, FontRole::Body, &style, content_x, text_y);
                let row_h = (badge_diam + row_pad * 2.0).max(text_h + row_pad * 2.0);

                // Striped background (pushed first → drawn behind)
                if idx % 2 == 0 {
                    frags.push(Fragment {
                        x: 0.0, y: row_start, width: geometry.column_width_pt, height: row_h,
                        kind: FragmentKind::FilledRect(FilledRect {
                            color: ALT_STRIPE_COLOR.to_owned(),
                        }),
                    });
                }

                // Badge circle
                let bx = badge_margin;
                let by = row_start + (row_h - badge_diam) / 2.0;
                frags.push(Fragment {
                    x: bx, y: by, width: badge_diam, height: badge_diam,
                    kind: FragmentKind::FilledCircle(FilledCircle {
                        color: "#000000".to_owned(),
                    }),
                });

                // White letter inside badge
                let fd     = resolver.resolve(FontRole::Body, FontWeight::Bold, FontStyle::Normal, None);
                let glyphs = shape_text(fd, &letter);
                let tw     = shaped_text_width(&glyphs, font_size, fd.units_per_em);
                let ascent = fd.ascender as f64 / fd.units_per_em as f64 * font_size;
                let family = Rc::from(resolver.resolve_family_name(FontRole::Body, None));
                frags.push(Fragment {
                    x:      bx + (badge_diam - tw) / 2.0,
                    y:      by + (badge_diam - font_size) / 2.0,
                    width:  tw,
                    height: font_size,
                    kind:   FragmentKind::GlyphRun(GlyphRun::from_shaped(
                        &glyphs, font_size, family, 1, Rc::from("#ffffff"), ascent,
                    )),
                });

                // Alternative text
                frags.extend(text_frags);

                // Border-bottom
                frags.push(Fragment {
                    x: 0.0, y: row_start + row_h,
                    width: geometry.column_width_pt, height: ALT_BORDER_STROKE_PT,
                    kind: FragmentKind::HRule(HRule {
                        stroke_width: ALT_BORDER_STROKE_PT,
                        color: ALT_BORDER_COLOR.to_owned(),
                    }),
                });

                local_y += row_h;
            }
        }

        AlternativeLayout::Horizontal => {
            // Grid layout: GRID_COLUMNS alternatives per row.
            let col_width = geometry.column_width_pt / GRID_COLUMNS as f64;

            let mut global_idx = 0usize;
            for row in choice.alternatives.chunks(GRID_COLUMNS) {
                let mut row_height = 0.0_f64;

                for (col_idx, alt) in row.iter().enumerate() {
                    let letter = format_alt_letter(&alt.label, global_idx, config.letter_case);
                    let prefix = format!("{letter}) ");
                    let content = build_alt_content(&prefix, &alt.content);
                    let origin_x = col_idx as f64 * col_width;
                    let engine  = InlineLayoutEngine {
                        resolver,
                        available_width:  col_width,
                        font_size, line_spacing, blank_default_cm,
            justify: false,
                    };
                    let (f, h) = engine.layout(&content, FontRole::Body, &style, origin_x, local_y);
                    frags.extend(f);
                    row_height = row_height.max(h);
                    global_idx += 1;
                }

                local_y += row_height + alt_spacing_pt;
            }
        }
    }

    let total_h = local_y - origin_y;
    (frags, total_h)
}

/// Build the full inline content for one alternative: `"A) "` prefix + body content.
pub(super) fn build_alt_content(prefix: &str, body: &[InlineContent]) -> Vec<InlineContent> {
    let mut content = Vec::with_capacity(body.len() + 1);
    content.push(InlineContent::Text(InlineText {
        value: prefix.to_owned(),
        style: None,
    }));
    content.extend_from_slice(body);
    content
}

/// Return just the letter for an alternative (e.g. "A", "b").
///
/// Uses `alt.label` when non-empty; otherwise falls back to `idx` (0-based).
pub(super) fn format_alt_letter(label: &str, idx: usize, case: LetterCase) -> String {
    if !label.is_empty() {
        match case {
            LetterCase::Upper => label.to_uppercase(),
            LetterCase::Lower => label.to_lowercase(),
        }
    } else {
        let base = b'A' + (idx % 26) as u8;
        let ch = char::from(base);
        match case {
            LetterCase::Upper => ch.to_string(),
            LetterCase::Lower => ch.to_lowercase().to_string(),
        }
    }
}

/// Produce the label prefix for an alternative (e.g. "A) ").
pub(super) fn format_alt_label(label: &str, idx: usize, case: LetterCase) -> String {
    format!("{}) ", format_alt_letter(label, idx, case))
}
