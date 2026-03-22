//! Inline layout engine — converts Vec<InlineContent> → Vec<Fragment>.
//!
//! Coordinate contract:
//!   origin_x / origin_y — absolute position in the content area.
//!   All output Fragment coordinates are also absolute.
//!
//! Algorithm:
//!   1. Flatten InlineContent items into InlineAtoms (word-level units).
//!   2. Greedy line-fill: accumulate atoms until available_width overflows.
//!   3. Render each line: translate atom-local coords to absolute page coords.

use std::rc::Rc;

use unicode_linebreak::{linebreaks, BreakOpportunity};

use crate::fonts::data::FontData;
use crate::fonts::resolve::{FontResolver, FontRole};
use crate::layout::fragment::{FilledRect, Fragment, FragmentKind, GlyphRun, HRule, VRule, ImageFragment};
use crate::layout::text::{shape_text, shaped_text_width, ShapedGlyph};
use crate::spec::inline::InlineContent;
use crate::spec::style::{FontStyle, FontWeight, ResolvedStyle, Style};
#[cfg(feature = "math")]
use crate::math::{parse_latex, layout_math, MathContext, MathLayoutResult, MathDrawCommand};

const CM_TO_PT: f64 = 28.3465;
pub const BLANK_DEFAULT_CM: f64 = 3.5;
/// Fraction of font_size used as the height of a blank underline stroke.
const BLANK_HEIGHT_FACTOR: f64 = 0.08;
/// Sub/Sup font size scale relative to parent.
const SUB_SUP_SCALE: f64 = 0.65;
/// Baseline offset for sub (downward) and sup (upward), in em units.
const SUB_DELTA_EM: f64 = 0.35;
const SUP_DELTA_EM: f64 = 0.35;

// ─────────────────────────────────────────────────────────────────────────────
// Internal atom types
// ─────────────────────────────────────────────────────────────────────────────

/// Smallest inline unit for line-filling.
struct InlineAtom {
    width_pt:  f64,
    ascent_pt: f64,
    descent_pt: f64,
    /// May trigger a line break before this atom when the line overflows.
    soft_break: bool,
    /// Must start a new line after this atom (e.g., mandatory break from \n).
    hard_break: bool,
    kind: AtomKind,
}

enum AtomKind {
    Text {
        glyphs:      Vec<ShapedGlyph>,
        font_family: Rc<str>,
        variant:     u8,
        font_size:   f64,
        units_per_em: u16,
        color:       Rc<str>,
    },
    Blank {
        width_pt: f64,
        color:    String,
    },
    Image {
        key:       String,
        height_pt: f64,
    },
    /// Pre-laid-out sub/superscript block (coordinates relative to origin (0,0)).
    SubSup {
        frags:             Vec<Fragment>,
        width_pt:          f64,
        /// Positive = subscript (downward from parent baseline).
        /// Negative = superscript (upward from parent baseline).
        baseline_delta_pt: f64,
        /// Ascent of the sub/sup content from its own origin (y=0).
        sub_ascent_pt:     f64,
    },
    /// Pre-laid-out math expression (coordinates in math coord system: y=0 baseline, +y up).
    #[cfg(feature = "math")]
    Math {
        result:      MathLayoutResult,
        font_family: Rc<str>,
        display:     bool,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// Public engine
// ─────────────────────────────────────────────────────────────────────────────

pub struct InlineLayoutEngine<'a> {
    pub resolver:        &'a FontResolver<'a>,
    pub available_width: f64,
    pub font_size:       f64,
    /// Multiplier: line height = font_size × line_spacing.
    pub line_spacing:    f64,
    /// Default width (cm) for `InlineContent::Blank` with no explicit width.
    /// Normal = 3.5cm; economy mode = 2.5cm.
    pub blank_default_cm: f64,
    /// When true, distribute extra horizontal space between words on non-last lines.
    pub justify:         bool,
}

impl<'a> InlineLayoutEngine<'a> {
    /// Layout `content` starting at `(origin_x, origin_y)` in content-area coords.
    ///
    /// Returns `(fragments, total_height)`.  All fragments have absolute coordinates.
    pub fn layout(
        &self,
        content: &[InlineContent],
        role: FontRole,
        style: &ResolvedStyle,
        origin_x: f64,
        origin_y: f64,
    ) -> (Vec<Fragment>, f64) {
        if content.is_empty() {
            return (vec![], 0.0);
        }

        let atoms = self.collect_atoms(content, role, style);
        if atoms.is_empty() {
            return (vec![], 0.0);
        }

        let line_height_pt = self.font_size * self.line_spacing;

        // ── Greedy line fill ──────────────────────────────────────────────────
        // Each line is a Vec of atom indices.
        let mut lines: Vec<Vec<usize>> = vec![vec![]];
        let mut cur_width = 0.0_f64;

        for (i, atom) in atoms.iter().enumerate() {
            if atom.hard_break {
                // Include this atom in the current line, then close it.
                lines.last_mut().unwrap().push(i);
                lines.push(vec![]);
                cur_width = 0.0;
            } else if atom.soft_break
                && cur_width + atom.width_pt > self.available_width
                && !lines.last().unwrap().is_empty()
            {
                // Overflow: start a new line with this atom.
                lines.push(vec![i]);
                cur_width = atom.width_pt;
            } else {
                lines.last_mut().unwrap().push(i);
                cur_width += atom.width_pt;
            }
        }

        // Drop trailing empty line (can appear after a final hard_break).
        while lines.last().map_or(false, |l| l.is_empty()) {
            lines.pop();
        }

        // ── Render lines ──────────────────────────────────────────────────────
        let mut all_frags = Vec::new();
        let total_lines = lines.len();

        for (line_idx, indices) in lines.iter().enumerate() {
            if indices.is_empty() {
                continue;
            }

            let line_y      = origin_y + line_idx as f64 * line_height_pt;
            let line_ascent = indices.iter()
                .map(|&i| atoms[i].ascent_pt)
                .fold(0.0_f64, f64::max);
            let baseline_y  = line_y + line_ascent;
            let mut cursor_x = origin_x;

            // ── Justify: compute per-gap extra space ─────────────────────
            let is_last_line = line_idx + 1 == total_lines;
            // Also treat lines ending with a hard break as "last" (no justify).
            let ends_with_hard = indices.last()
                .map(|&i| atoms[i].hard_break)
                .unwrap_or(false);
            let extra_per_gap = if self.justify && !is_last_line && !ends_with_hard {
                let line_width: f64 = indices.iter().map(|&i| atoms[i].width_pt).sum();
                let slack = self.available_width - line_width;
                // Count soft-break points (gaps between words).
                let gap_count = indices.iter()
                    .filter(|&&i| atoms[i].soft_break)
                    .count();
                if gap_count > 0 && slack > 0.0 {
                    slack / gap_count as f64
                } else {
                    0.0
                }
            } else {
                0.0
            };

            for &i in indices {
                let atom = &atoms[i];
                // Add justify gap before soft-break atoms (word starts).
                if atom.soft_break && extra_per_gap > 0.0 {
                    cursor_x += extra_per_gap;
                }
                let frags = self.render_atom(atom, cursor_x, line_y, line_ascent, baseline_y);
                all_frags.extend(frags);
                cursor_x += atom.width_pt;
            }
        }

        let total_height = lines.len() as f64 * line_height_pt;
        (all_frags, total_height)
    }

    // ── Atom collection ───────────────────────────────────────────────────────

    fn collect_atoms(
        &self,
        content: &[InlineContent],
        role: FontRole,
        style: &ResolvedStyle,
    ) -> Vec<InlineAtom> {
        let mut atoms: Vec<InlineAtom> = Vec::new();
        // `is_first` tracks whether we've emitted any atom yet; the very first
        // atom must have soft_break=false (can't break before the beginning).
        let mut is_first = true;

        for item in content {
            match item {
                InlineContent::Text(t) => {
                    // Merge per-inline style overrides onto the base ResolvedStyle.
                    let merged = merge_inline_style(style, t.style.as_ref());
                    let font_data = self.resolver.resolve(
                        role,
                        merged.font_weight,
                        merged.font_style,
                        merged.font_family.as_deref(),
                    );
                    let family: Rc<str> = Rc::from(self.resolver
                        .resolve_family_name(role, merged.font_family.as_deref()));
                    let variant = style_to_variant(merged.font_weight, merged.font_style);
                    let color: Rc<str>   = Rc::from(color_to_css(merged.color));
                    let font_size = merged.font_size.unwrap_or(self.font_size);

                    self.text_to_atoms(
                        &t.value, font_data, family, variant,
                        font_size, color,
                        &mut atoms, &mut is_first,
                    );
                }

                InlineContent::Blank(b) => {
                    let width_pt = b.width_cm.unwrap_or(self.blank_default_cm) * CM_TO_PT;
                    let font_size = self.font_size;
                    let color     = color_to_css(style.color);
                    atoms.push(InlineAtom {
                        width_pt,
                        ascent_pt:  font_size * 0.8,
                        descent_pt: font_size * 0.2,
                        soft_break: !is_first,
                        hard_break: false,
                        kind: AtomKind::Blank { width_pt, color },
                    });
                    is_first = false;
                }

                InlineContent::Sub(s) => {
                    let sub_fs = self.font_size * SUB_SUP_SCALE;
                    let delta  = SUB_DELTA_EM * self.font_size; // positive = down
                    let sub_ascent = self.font_ascent(role, style, sub_fs);
                    let (frags, _) = self.layout_sub_sup(&s.content, role, style, sub_fs);
                    let w = frags_extent(&frags);
                    atoms.push(InlineAtom {
                        width_pt:   w,
                        ascent_pt:  self.font_size * 0.8,
                        descent_pt: sub_ascent + delta,
                        soft_break: false, // sub/sup sticks to the preceding run
                        hard_break: false,
                        kind: AtomKind::SubSup {
                            frags, width_pt: w,
                            baseline_delta_pt: delta,
                            sub_ascent_pt: sub_ascent,
                        },
                    });
                    is_first = false;
                }

                InlineContent::Sup(s) => {
                    let sup_fs = self.font_size * SUB_SUP_SCALE;
                    let delta  = -(SUP_DELTA_EM * self.font_size); // negative = up
                    let sub_ascent = self.font_ascent(role, style, sup_fs);
                    let (frags, _) = self.layout_sub_sup(&s.content, role, style, sup_fs);
                    let w = frags_extent(&frags);
                    // The sup raises content above the baseline → increases ascent.
                    let extra_ascent = SUP_DELTA_EM * self.font_size + sub_ascent;
                    atoms.push(InlineAtom {
                        width_pt:   w,
                        ascent_pt:  extra_ascent.max(self.font_size * 0.8),
                        descent_pt: self.font_size * 0.2,
                        soft_break: false,
                        hard_break: false,
                        kind: AtomKind::SubSup {
                            frags, width_pt: w,
                            baseline_delta_pt: delta,
                            sub_ascent_pt: sub_ascent,
                        },
                    });
                    is_first = false;
                }

                InlineContent::Image(img) => {
                    let height_pt = img.height_cm.unwrap_or(2.0) * CM_TO_PT;
                    let width_pt  = img.width_cm
                        .map(|w| w * CM_TO_PT)
                        .unwrap_or(height_pt);
                    atoms.push(InlineAtom {
                        width_pt,
                        ascent_pt:  height_pt,
                        descent_pt: 0.0,
                        soft_break: !is_first,
                        hard_break: false,
                        kind: AtomKind::Image { key: img.key.clone(), height_pt },
                    });
                    is_first = false;
                }

                #[cfg(feature = "math")]
                InlineContent::Math(m) => {
                    let font_data = self.resolver.resolve(
                        FontRole::Math, FontWeight::Normal, FontStyle::Normal, None,
                    );
                    let font_family: Rc<str> = Rc::from(self.resolver.resolve_family_name(FontRole::Math, None));
                    let ctx = MathContext::new(font_data, self.font_size, m.display);
                    if let Ok(node) = parse_latex(&m.latex) {
                        let result = layout_math(&node, &ctx);
                        let w = result.width;
                        let h = result.height;
                        let d = result.depth;
                        atoms.push(InlineAtom {
                            width_pt:   if m.display { self.available_width } else { w },
                            ascent_pt:  h,
                            descent_pt: d,
                            soft_break: !is_first,
                            hard_break: false,
                            kind: AtomKind::Math { result, font_family, display: m.display },
                        });
                        is_first = false;
                    }
                }
                #[cfg(not(feature = "math"))]
                InlineContent::Math(_) => {}
            }
        }

        atoms
    }

    /// Split `text` into word-level atoms using Unicode line-break opportunities.
    fn text_to_atoms(
        &self,
        text:        &str,
        font_data:   &FontData,
        font_family: Rc<str>,
        variant:     u8,
        font_size:   f64,
        color:       Rc<str>,
        atoms:       &mut Vec<InlineAtom>,
        is_first:    &mut bool,
    ) {
        if text.is_empty() {
            return;
        }

        let ascent_pt  = font_data.ascender  as f64 / font_data.units_per_em as f64 * font_size;
        let descent_pt = (-font_data.descender as f64) / font_data.units_per_em as f64 * font_size;
        let upm        = font_data.units_per_em;
        let mut seg_start  = 0usize;
        let mut first_seg  = true;

        for (pos, opp) in linebreaks(text) {
            if pos <= seg_start {
                continue;
            }
            let seg    = &text[seg_start..pos];
            let glyphs = shape_text(font_data, seg);
            let w      = shaped_text_width(&glyphs, font_size, upm);

            atoms.push(InlineAtom {
                width_pt:   w,
                ascent_pt,
                descent_pt,
                soft_break: !(*is_first && first_seg),
                // unicode-linebreak always emits Mandatory at pos == text.len()
                // (end-of-string). Only treat as hard_break when the break is
                // caused by an embedded newline character *inside* the text.
                hard_break: matches!(opp, BreakOpportunity::Mandatory) && pos < text.len(),
                kind: AtomKind::Text {
                    glyphs,
                    font_family: font_family.clone(),
                    variant,
                    font_size,
                    units_per_em: upm,
                    color: color.clone(),
                },
            });

            first_seg = false;
            seg_start = pos;
        }

        // Leftover text (safety net; unicode-linebreak usually covers the whole string)
        if seg_start < text.len() {
            let seg    = &text[seg_start..];
            let glyphs = shape_text(font_data, seg);
            let w      = shaped_text_width(&glyphs, font_size, upm);
            atoms.push(InlineAtom {
                width_pt:   w,
                ascent_pt,
                descent_pt,
                soft_break: !(*is_first && first_seg),
                hard_break: false,
                kind: AtomKind::Text {
                    glyphs,
                    font_family,
                    variant,
                    font_size,
                    units_per_em: upm,
                    color,
                },
            });
        }

        if !first_seg {
            *is_first = false;
        }
    }

    // ── Atom rendering ────────────────────────────────────────────────────────

    fn render_atom(
        &self,
        atom:       &InlineAtom,
        x:          f64,
        line_y:     f64,
        line_ascent: f64,
        baseline_y: f64,
    ) -> Vec<Fragment> {
        match &atom.kind {
            AtomKind::Text { glyphs, font_family, variant, font_size, units_per_em: _, color } => {
                if glyphs.is_empty() {
                    return vec![];
                }
                let run = GlyphRun {
                    glyph_ids:       glyphs.iter().map(|g| g.glyph_id).collect(),
                    x_advances:      glyphs.iter().map(|g| g.x_advance).collect(),
                    x_offsets:       glyphs.iter().map(|g| g.x_offset).collect(),
                    y_offsets:       glyphs.iter().map(|g| g.y_offset).collect(),
                    font_size:       *font_size,
                    font_family:     font_family.clone(),
                    variant:         *variant,
                    color:           color.clone(),
                    baseline_offset: line_ascent,
                };
                vec![Fragment {
                    x, y: line_y,
                    width:  atom.width_pt,
                    height: atom.ascent_pt + atom.descent_pt,
                    kind:   FragmentKind::GlyphRun(run),
                }]
            }

            AtomKind::Blank { width_pt, color } => {
                // Thin filled rect at the text baseline (underline style).
                let h = (self.font_size * BLANK_HEIGHT_FACTOR).max(0.5);
                vec![Fragment {
                    x, y: baseline_y - h,
                    width:  *width_pt,
                    height: h,
                    kind:   FragmentKind::FilledRect(FilledRect { color: color.clone() }),
                }]
            }

            AtomKind::Image { key, height_pt } => {
                vec![Fragment {
                    x, y: line_y,
                    width:  atom.width_pt,
                    height: *height_pt,
                    kind:   FragmentKind::Image(ImageFragment { key: key.clone() }),
                }]
            }

            AtomKind::SubSup { frags, baseline_delta_pt, sub_ascent_pt, .. } => {
                // target sub/sup top = baseline_y + baseline_delta_pt - sub_ascent_pt
                let sub_top = baseline_y + baseline_delta_pt - sub_ascent_pt;
                frags.iter().map(|f| Fragment {
                    x:      f.x + x,
                    y:      f.y + sub_top,
                    width:  f.width,
                    height: f.height,
                    kind:   f.kind.clone(),
                }).collect()
            }

            #[cfg(feature = "math")]
            AtomKind::Math { result, font_family, display } => {
                math_result_to_fragments(result, font_family, *display, x, baseline_y, atom.width_pt)
            }
        }
    }

    // ── Sub/Sup helpers ───────────────────────────────────────────────────────

    fn layout_sub_sup(
        &self,
        content:   &[InlineContent],
        role:      FontRole,
        style:     &ResolvedStyle,
        font_size: f64,
    ) -> (Vec<Fragment>, f64) {
        let child = InlineLayoutEngine {
            resolver:         self.resolver,
            available_width:  self.available_width,
            font_size,
            line_spacing:     self.line_spacing,
            blank_default_cm: self.blank_default_cm,
            justify: false,
        };
        child.layout(content, role, style, 0.0, 0.0)
    }

    /// Ascent in points for the given font at `font_size`.
    fn font_ascent(&self, role: FontRole, style: &ResolvedStyle, font_size: f64) -> f64 {
        let fd = self.resolver.resolve(
            role, style.font_weight, style.font_style,
            style.font_family.as_deref(),
        );
        fd.ascender as f64 / fd.units_per_em as f64 * font_size
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Free helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Convert FontWeight + FontStyle to a variant index (0–3).
fn style_to_variant(weight: FontWeight, style: FontStyle) -> u8 {
    match (weight, style) {
        (FontWeight::Bold, FontStyle::Italic) => 3,
        (FontWeight::Bold,   _)              => 1,
        (_,   FontStyle::Italic)             => 2,
        _                                    => 0,
    }
}

/// Encode an (r, g, b) ∈ [0, 1] colour as a CSS hex string.
pub fn color_to_css(color: (f32, f32, f32)) -> String {
    let r = (color.0 * 255.0).round() as u8;
    let g = (color.1 * 255.0).round() as u8;
    let b = (color.2 * 255.0).round() as u8;
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// A partially-merged style: carries the ResolvedStyle fields with an optional
/// font_size override (Some = inline override, None = use engine font_size).
struct MergedStyle {
    font_size:   Option<f64>,
    font_weight: FontWeight,
    font_style:  FontStyle,
    font_family: Option<String>,
    color:       (f32, f32, f32),
}

/// Merge an optional per-inline `Style` onto a base `ResolvedStyle`.
/// Fields present in `inline` override those in `base`.
fn merge_inline_style(base: &ResolvedStyle, inline: Option<&Style>) -> MergedStyle {
    match inline {
        None => MergedStyle {
            font_size:   None,
            font_weight: base.font_weight,
            font_style:  base.font_style,
            font_family: base.font_family.clone(),
            color:       base.color,
        },
        Some(s) => MergedStyle {
            font_size:   s.font_size,
            font_weight: s.font_weight.unwrap_or(base.font_weight),
            font_style:  s.font_style.unwrap_or(base.font_style),
            font_family: s.font_family.clone().or_else(|| base.font_family.clone()),
            color:       s.color.as_deref()
                .and_then(parse_css_color)
                .unwrap_or(base.color),
        },
    }
}

/// Parse a CSS hex color string (#rgb or #rrggbb) to normalized (r, g, b).
fn parse_css_color(s: &str) -> Option<(f32, f32, f32)> {
    let s = s.strip_prefix('#')?;
    match s.len() {
        6 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()? as f32 / 255.0;
            let g = u8::from_str_radix(&s[2..4], 16).ok()? as f32 / 255.0;
            let b = u8::from_str_radix(&s[4..6], 16).ok()? as f32 / 255.0;
            Some((r, g, b))
        }
        3 => {
            let r = u8::from_str_radix(&s[0..1], 16).ok()? as f32 / 15.0;
            let g = u8::from_str_radix(&s[1..2], 16).ok()? as f32 / 15.0;
            let b = u8::from_str_radix(&s[2..3], 16).ok()? as f32 / 15.0;
            Some((r, g, b))
        }
        _ => None,
    }
}

/// Horizontal extent of a set of fragments (max x+width − min x).
fn frags_extent(frags: &[Fragment]) -> f64 {
    if frags.is_empty() { return 0.0; }
    let min_x = frags.iter().map(|f| f.x).fold(f64::INFINITY,     f64::min);
    let max_x = frags.iter().map(|f| f.x + f.width).fold(f64::NEG_INFINITY, f64::max);
    (max_x - min_x).max(0.0)
}

// ─────────────────────────────────────────────────────────────────────────────
// Math → Fragment conversion
// ─────────────────────────────────────────────────────────────────────────────

/// Convert a `MathLayoutResult` into positioned `Fragment`s.
///
/// Math coordinate system: y=0 is baseline, positive = up.
/// Fragment coordinate system: y grows downward from top of content area.
///
/// Transformation: frag.y = baseline_y - glyph.y  (inverts Y axis)
#[cfg(feature = "math")]
fn math_result_to_fragments(
    result:      &MathLayoutResult,
    font_family: &str,
    display:     bool,
    origin_x:    f64,
    baseline_y:  f64,
    atom_width:  f64,
) -> Vec<Fragment> {
    let mut frags = Vec::new();

    // For display math, center horizontally.
    let x_offset = if display {
        (atom_width - result.width) / 2.0
    } else {
        0.0
    };
    let base_x = origin_x + x_offset;

    // Group consecutive glyphs with the same font_size into GlyphRuns.
    // MathLayoutResult glyphs are individually positioned, so we convert each
    // glyph into its own GlyphRun for simplicity and correctness.
    for g in &result.glyphs {
        let frag_x = base_x + g.x;
        let frag_y = baseline_y - g.y - g.size;  // top of glyph box

        let run = GlyphRun {
            glyph_ids:       vec![g.glyph_id],
            x_advances:      vec![0],   // single glyph, no advance needed
            x_offsets:       vec![0],
            y_offsets:       vec![0],
            font_size:       g.size,
            font_family:     Rc::from(font_family),
            variant:         0,  // math uses regular variant
            color:           Rc::from("#000000"),
            baseline_offset: g.size,  // baseline at bottom of the glyph box
        };
        frags.push(Fragment {
            x: frag_x,
            y: frag_y,
            width:  g.size * 0.6, // approximate glyph width
            height: g.size,
            kind: FragmentKind::GlyphRun(run),
        });
    }

    // Convert draw commands to fragments.
    for cmd in &result.rules {
        match cmd {
            MathDrawCommand::HRule { x: rx, y: ry, width: rw, thickness } => {
                frags.push(Fragment {
                    x:      base_x + rx,
                    y:      baseline_y - ry - thickness / 2.0,
                    width:  *rw,
                    height: *thickness,
                    kind:   FragmentKind::HRule(HRule {
                        stroke_width: *thickness,
                        color: "#000000".to_string(),
                    }),
                });
            }
            MathDrawCommand::VBar { x: vx, y_center, half_height, thickness } => {
                frags.push(Fragment {
                    x:      base_x + vx - thickness / 2.0,
                    y:      baseline_y - y_center - half_height,
                    width:  *thickness,
                    height: 2.0 * half_height,
                    kind:   FragmentKind::VRule(VRule {
                        stroke_width: *thickness,
                        color: "#000000".to_string(),
                    }),
                });
            }
        }
    }

    frags
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::registry::{FontRegistry, FontRules};
    use crate::spec::inline::{InlineBlank, InlineSubSup, InlineText};
    use crate::test_helpers::fixtures::make_resolver_and_rules;

    // ── Test helpers ──────────────────────────────────────────────────────────

    fn make_registry() -> FontRegistry {
        let (reg, _) = make_resolver_and_rules();
        reg
    }

    fn make_engine<'a>(
        resolver:        &'a FontResolver<'a>,
        available_width: f64,
        font_size:       f64,
    ) -> InlineLayoutEngine<'a> {
        InlineLayoutEngine {
            resolver,
            available_width,
            font_size,
            line_spacing:     1.4,
            blank_default_cm: BLANK_DEFAULT_CM,
            justify:          false,
        }
    }

    fn text_item(s: &str) -> InlineContent {
        InlineContent::Text(InlineText { value: s.to_owned(), style: None })
    }

    fn blank_item(width_cm: Option<f64>) -> InlineContent {
        InlineContent::Blank(InlineBlank { width_cm })
    }

    fn sub_item(text: &str) -> InlineContent {
        InlineContent::Sub(InlineSubSup { content: vec![text_item(text)] })
    }

    fn sup_item(text: &str) -> InlineContent {
        InlineContent::Sup(InlineSubSup { content: vec![text_item(text)] })
    }

    // ── Empty input ───────────────────────────────────────────────────────────

    #[test]
    fn empty_content_produces_no_fragments() {
        let reg   = make_registry();
        let rules = FontRules::default();
        let res   = FontResolver::new(&reg, &rules);
        let eng   = make_engine(&res, 500.0, 12.0);
        let style = ResolvedStyle::default();

        let (frags, h) = eng.layout(&[], FontRole::Body, &style, 0.0, 0.0);
        assert!(frags.is_empty());
        assert_eq!(h, 0.0);
    }

    // ── Text wrapping ─────────────────────────────────────────────────────────

    #[test]
    fn short_text_fits_one_line() {
        let reg   = make_registry();
        let rules = FontRules::default();
        let res   = FontResolver::new(&reg, &rules);
        let eng   = make_engine(&res, 500.0, 12.0);
        let style = ResolvedStyle::default();

        let content = vec![text_item("Hello")];
        let (frags, h) = eng.layout(&content, FontRole::Body, &style, 0.0, 0.0);

        assert!(!frags.is_empty(), "should produce at least one fragment");
        assert!(h > 0.0, "height should be positive");

        // All fragments on the first line → same y.
        let first_y = frags[0].y;
        for f in &frags {
            assert!((f.y - first_y).abs() < 0.001, "all fragments on same line");
        }
    }

    #[test]
    fn long_text_wraps_into_multiple_lines() {
        let reg   = make_registry();
        let rules = FontRules::default();
        let res   = FontResolver::new(&reg, &rules);
        // Very narrow width to force wrapping.
        let eng   = make_engine(&res, 60.0, 12.0);
        let style = ResolvedStyle::default();

        let long = "Lorem ipsum dolor sit amet consectetur adipiscing elit";
        let content = vec![text_item(long)];
        let (frags, h) = eng.layout(&content, FontRole::Body, &style, 0.0, 0.0);

        // Multiple lines → fragments at different y positions.
        let mut ys: Vec<f64> = frags.iter().map(|f| f.y).collect();
        ys.dedup_by(|a, b| (*a - *b).abs() < 0.001);
        assert!(ys.len() > 1, "long text should produce multiple line y-positions");
        assert!(h > 12.0 * 1.4, "total height should exceed one line");
    }

    #[test]
    fn total_height_is_multiple_of_line_height() {
        let reg   = make_registry();
        let rules = FontRules::default();
        let res   = FontResolver::new(&reg, &rules);
        let eng   = make_engine(&res, 60.0, 12.0);
        let style = ResolvedStyle::default();

        let content = vec![text_item("Hello world foo bar baz qux")];
        let (_, h) = eng.layout(&content, FontRole::Body, &style, 0.0, 0.0);
        let line_h = 12.0 * 1.4;
        let lines  = (h / line_h).round() as u32;
        assert!((h - lines as f64 * line_h).abs() < 0.001,
            "height {h} should be a multiple of line_height {line_h}");
    }

    // ── Blank ─────────────────────────────────────────────────────────────────

    #[test]
    fn blank_default_width_is_3_5cm() {
        let reg   = make_registry();
        let rules = FontRules::default();
        let res   = FontResolver::new(&reg, &rules);
        let eng   = make_engine(&res, 500.0, 12.0);
        let style = ResolvedStyle::default();

        let content = vec![blank_item(None)];
        let (frags, _) = eng.layout(&content, FontRole::Body, &style, 0.0, 0.0);

        assert_eq!(frags.len(), 1);
        let expected = BLANK_DEFAULT_CM * CM_TO_PT;
        assert!((frags[0].width - expected).abs() < 0.001,
            "blank width should be {expected:.2} pt, got {:.2}", frags[0].width);
    }

    #[test]
    fn blank_custom_width() {
        let reg   = make_registry();
        let rules = FontRules::default();
        let res   = FontResolver::new(&reg, &rules);
        let eng   = make_engine(&res, 500.0, 12.0);
        let style = ResolvedStyle::default();

        let content = vec![blank_item(Some(5.0))];
        let (frags, _) = eng.layout(&content, FontRole::Body, &style, 0.0, 0.0);

        assert_eq!(frags.len(), 1);
        let expected = 5.0 * CM_TO_PT;
        assert!((frags[0].width - expected).abs() < 0.001,
            "blank width should be {expected:.2} pt, got {:.2}", frags[0].width);
    }

    #[test]
    fn blank_produces_filled_rect() {
        let reg   = make_registry();
        let rules = FontRules::default();
        let res   = FontResolver::new(&reg, &rules);
        let eng   = make_engine(&res, 500.0, 12.0);
        let style = ResolvedStyle::default();

        let content = vec![blank_item(Some(4.0))];
        let (frags, _) = eng.layout(&content, FontRole::Body, &style, 0.0, 0.0);

        assert!(matches!(frags[0].kind, FragmentKind::FilledRect(_)),
            "blank should produce a FilledRect fragment");
    }

    // ── Subscript ─────────────────────────────────────────────────────────────

    #[test]
    fn subscript_baseline_is_below_parent() {
        let reg   = make_registry();
        let rules = FontRules::default();
        let res   = FontResolver::new(&reg, &rules);
        let eng   = make_engine(&res, 500.0, 12.0);
        let style = ResolvedStyle::default();

        // "X" followed by subscript "2".
        let content = vec![text_item("X"), sub_item("2")];
        let (frags, _) = eng.layout(&content, FontRole::Body, &style, 0.0, 0.0);

        // All frags should be on the same line (y between line_y and line_y+height).
        assert!(frags.len() >= 2);

        // The baseline_offset of the parent GlyphRun (index 0).
        let parent_frag = &frags[0];
        let parent_baseline = parent_frag.y + if let FragmentKind::GlyphRun(ref r) = parent_frag.kind {
            r.baseline_offset
        } else { panic!("expected GlyphRun") };

        // The subscript fragment is the last one; its baseline should be below the parent's.
        let sub_frag = frags.last().unwrap();
        let sub_baseline = sub_frag.y + if let FragmentKind::GlyphRun(ref r) = sub_frag.kind {
            r.baseline_offset
        } else { panic!("sub should also be GlyphRun") };

        assert!(sub_baseline > parent_baseline,
            "sub baseline ({sub_baseline:.2}) should be below parent baseline ({parent_baseline:.2})");
    }

    // ── Superscript ───────────────────────────────────────────────────────────

    #[test]
    fn superscript_baseline_is_above_parent() {
        let reg   = make_registry();
        let rules = FontRules::default();
        let res   = FontResolver::new(&reg, &rules);
        let eng   = make_engine(&res, 500.0, 12.0);
        let style = ResolvedStyle::default();

        // "X" followed by superscript "2".
        let content = vec![text_item("X"), sup_item("2")];
        let (frags, _) = eng.layout(&content, FontRole::Body, &style, 0.0, 0.0);

        assert!(frags.len() >= 2);

        let parent_frag     = &frags[0];
        let parent_baseline = parent_frag.y + if let FragmentKind::GlyphRun(ref r) = parent_frag.kind {
            r.baseline_offset
        } else { panic!("expected GlyphRun") };

        let sup_frag     = frags.last().unwrap();
        let sup_baseline = sup_frag.y + if let FragmentKind::GlyphRun(ref r) = sup_frag.kind {
            r.baseline_offset
        } else { panic!("sup should also be GlyphRun") };

        assert!(sup_baseline < parent_baseline,
            "sup baseline ({sup_baseline:.2}) should be above parent baseline ({parent_baseline:.2})");
    }

    // ── Sub/Sup font size ─────────────────────────────────────────────────────

    #[test]
    fn subscript_font_size_is_scaled() {
        let reg   = make_registry();
        let rules = FontRules::default();
        let res   = FontResolver::new(&reg, &rules);
        let eng   = make_engine(&res, 500.0, 12.0);
        let style = ResolvedStyle::default();

        let content = vec![sub_item("2")];
        let (frags, _) = eng.layout(&content, FontRole::Body, &style, 0.0, 0.0);

        assert!(!frags.is_empty());
        if let FragmentKind::GlyphRun(ref r) = frags[0].kind {
            let expected = 12.0 * SUB_SUP_SCALE;
            assert!((r.font_size - expected).abs() < 0.001,
                "sub font_size should be {expected}, got {}", r.font_size);
        } else {
            panic!("expected GlyphRun");
        }
    }

    // ── Origin translation ────────────────────────────────────────────────────

    #[test]
    fn origin_x_y_offsets_fragments() {
        let reg   = make_registry();
        let rules = FontRules::default();
        let res   = FontResolver::new(&reg, &rules);
        let eng   = make_engine(&res, 500.0, 12.0);
        let style = ResolvedStyle::default();

        let content = vec![text_item("Hi")];
        let (frags_at_0, _) = eng.layout(&content, FontRole::Body, &style, 0.0,  0.0);
        let (frags_offset, _) = eng.layout(&content, FontRole::Body, &style, 10.0, 20.0);

        assert_eq!(frags_at_0.len(), frags_offset.len());
        for (a, b) in frags_at_0.iter().zip(frags_offset.iter()) {
            assert!((b.x - a.x - 10.0).abs() < 0.001, "x should shift by origin_x");
            assert!((b.y - a.y - 20.0).abs() < 0.001, "y should shift by origin_y");
        }
    }

    #[test]
    fn inline_bold_style_produces_variant_1() {
        let reg = make_registry();
        let rules = FontRules::default();
        let resolver = FontResolver::new(&reg, &rules);
        let eng = make_engine(&resolver, 400.0, 12.0);
        let style = ResolvedStyle::default();

        let content = vec![
            InlineContent::Text(InlineText {
                value: "normal ".into(),
                style: None,
            }),
            InlineContent::Text(InlineText {
                value: "bold".into(),
                style: Some(crate::spec::style::Style {
                    font_weight: Some(FontWeight::Bold),
                    ..Default::default()
                }),
            }),
        ];
        let (frags, _) = eng.layout(&content, FontRole::Body, &style, 0.0, 0.0);
        let runs: Vec<_> = frags.iter().filter_map(|f| {
            if let FragmentKind::GlyphRun(r) = &f.kind { Some(r) } else { None }
        }).collect();
        assert!(runs.len() >= 2, "should have at least 2 glyph runs, got {}", runs.len());
        assert_eq!(runs[0].variant, 0, "first run should be normal (variant=0)");
        // Find a run with variant=1
        let has_bold = runs.iter().any(|r| r.variant == 1);
        assert!(has_bold, "should have a bold run (variant=1), variants: {:?}", runs.iter().map(|r| r.variant).collect::<Vec<_>>());
    }

    // ── Math inline ──────────────────────────────────────────────────────────

    #[cfg(feature = "math")]
    #[test]
    fn math_inline_produces_glyph_fragments() {
        use crate::spec::inline::InlineMath;

        let reg   = make_registry();
        let rules = FontRules::default();
        let res   = FontResolver::new(&reg, &rules);
        let eng   = make_engine(&res, 500.0, 12.0);
        let style = ResolvedStyle::default();

        let content = vec![
            text_item("Solve "),
            InlineContent::Math(InlineMath { latex: "x^2".to_string(), display: false }),
        ];
        let (frags, h) = eng.layout(&content, FontRole::Body, &style, 0.0, 0.0);

        assert!(h > 0.0, "height should be positive");
        // Should have text glyph runs + math glyph runs.
        let glyph_runs: Vec<_> = frags.iter().filter(|f| matches!(&f.kind, FragmentKind::GlyphRun(_))).collect();
        assert!(glyph_runs.len() >= 2, "should have text + math glyph runs, got {}", glyph_runs.len());
    }

    #[cfg(feature = "math")]
    #[test]
    fn math_fraction_produces_hrule() {
        use crate::spec::inline::InlineMath;

        let reg   = make_registry();
        let rules = FontRules::default();
        let res   = FontResolver::new(&reg, &rules);
        let eng   = make_engine(&res, 500.0, 12.0);
        let style = ResolvedStyle::default();

        let content = vec![
            InlineContent::Math(InlineMath { latex: r"\frac{1}{2}".to_string(), display: false }),
        ];
        let (frags, _) = eng.layout(&content, FontRole::Body, &style, 0.0, 0.0);

        let has_hrule = frags.iter().any(|f| matches!(&f.kind, FragmentKind::HRule(_)));
        assert!(has_hrule, "fraction should produce an HRule fragment");
    }

    #[cfg(feature = "math")]
    #[test]
    fn math_display_mode_centers_content() {
        use crate::spec::inline::InlineMath;

        let reg   = make_registry();
        let rules = FontRules::default();
        let res   = FontResolver::new(&reg, &rules);
        let eng   = make_engine(&res, 500.0, 12.0);
        let style = ResolvedStyle::default();

        let content = vec![
            InlineContent::Math(InlineMath { latex: "x".to_string(), display: true }),
        ];
        let (frags, _) = eng.layout(&content, FontRole::Body, &style, 0.0, 0.0);

        assert!(!frags.is_empty());
        // Display math should be centered: x > 0 (shifted right from origin).
        let first_x = frags[0].x;
        assert!(first_x > 100.0, "display math should be centered, x={first_x:.1}");
    }
}
