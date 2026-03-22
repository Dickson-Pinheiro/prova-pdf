//! Style cascade: PrintConfig → Section → Question → InlineContent.
//!
//! Each layer applies its optional fields over the resolved style from the
//! previous layer.  `all_black: true` in PrintConfig forces color=(0,0,0)
//! everywhere and cannot be overridden by any inner layer.

use crate::color::Color;
use crate::spec::config::PrintConfig;
use crate::spec::style::{FontStyle, FontWeight, ResolvedStyle, Style, TextAlign};

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Build the base `ResolvedStyle` from `PrintConfig` alone.
///
/// This is the root of every cascade chain.  All layers below it are applied
/// via successive `apply_style` calls.
pub fn base_style(cfg: &PrintConfig) -> ResolvedStyle {
    ResolvedStyle {
        font_size:    cfg.font_size,
        font_weight:  FontWeight::Normal,
        font_style:   FontStyle::Normal,
        // "body" is the sentinel default — don't pin it as an explicit override.
        font_family:  if cfg.font_family == "body" {
            None
        } else {
            Some(cfg.font_family.clone())
        },
        color:        (0.0, 0.0, 0.0),
        underline:    false,
        text_align:   TextAlign::Left,
        line_spacing: cfg.line_spacing.multiplier(),
    }
}

/// Apply one `Style` layer over an existing `ResolvedStyle`, returning a new value.
///
/// Fields that are `None` in `style` are inherited unchanged from `parent`.
/// If `all_black` is `true` the colour is always forced to `(0, 0, 0)`,
/// regardless of what the layer sets.
pub fn apply_style(parent: &ResolvedStyle, style: &Style, all_black: bool) -> ResolvedStyle {
    let mut out = parent.clone();

    if let Some(fs) = style.font_size    { out.font_size   = fs; }
    if let Some(fw) = style.font_weight  { out.font_weight = fw; }
    if let Some(fi) = style.font_style   { out.font_style  = fi; }
    if let Some(ref ff) = style.font_family {
        out.font_family = Some(ff.clone());
    }
    if let Some(u) = style.underline     { out.underline   = u; }
    if let Some(ta) = style.text_align   { out.text_align  = ta; }

    // Colour: parse CSS string, then honour all_black.
    if !all_black {
        if let Some(ref css) = style.color {
            if let Ok(c) = Color::from_str(css) {
                let (r, g, b) = c.to_srgb();
                out.color = (r as f32, g as f32, b as f32);
            }
            // If parsing fails we silently inherit the parent colour.
        }
    }

    // all_black overrides any colour set above.
    if all_black {
        out.color = (0.0, 0.0, 0.0);
    }

    out
}

/// Convenience: apply an `Option<&Style>` layer.
/// `None` means "no override" — parent is returned unchanged.
pub fn apply_opt_style(
    parent:    &ResolvedStyle,
    style:     Option<&Style>,
    all_black: bool,
) -> ResolvedStyle {
    match style {
        Some(s) => apply_style(parent, s, all_black),
        None    => {
            if all_black {
                let mut out = parent.clone();
                out.color = (0.0, 0.0, 0.0);
                out
            } else {
                parent.clone()
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::config::PrintConfig;
    use crate::spec::style::{FontWeight, Style, TextAlign};

    fn default_base() -> ResolvedStyle {
        base_style(&PrintConfig::default())
    }

    // ── base_style ────────────────────────────────────────────────────────────

    #[test]
    fn base_style_uses_config_font_size() {
        let cfg = PrintConfig { font_size: 14.0, ..PrintConfig::default() };
        let s   = base_style(&cfg);
        assert_eq!(s.font_size, 14.0);
    }

    #[test]
    fn base_style_default_color_is_black() {
        let s = default_base();
        assert_eq!(s.color, (0.0, 0.0, 0.0));
    }

    #[test]
    fn base_style_body_family_becomes_none() {
        let s = default_base(); // font_family == "body"
        assert!(s.font_family.is_none(),
            "font_family 'body' should resolve to None (follow FontRules)");
    }

    #[test]
    fn base_style_custom_family_is_preserved() {
        let cfg = PrintConfig { font_family: "Helvetica".into(), ..PrintConfig::default() };
        let s   = base_style(&cfg);
        assert_eq!(s.font_family.as_deref(), Some("Helvetica"));
    }

    #[test]
    fn base_style_line_spacing_matches_config() {
        use crate::spec::config::LineSpacing;
        let cfg = PrintConfig { line_spacing: LineSpacing::OneAndHalf, ..PrintConfig::default() };
        let s   = base_style(&cfg);
        assert_eq!(s.line_spacing, 1.5);
    }

    // ── apply_style — field inheritance ──────────────────────────────────────

    #[test]
    fn empty_style_inherits_everything() {
        let base     = default_base();
        let out      = apply_style(&base, &Style::default(), false);
        assert_eq!(out.font_size,   base.font_size);
        assert_eq!(out.font_weight, base.font_weight);
        assert_eq!(out.color,       base.color);
    }

    #[test]
    fn font_size_overrides_parent() {
        let base  = default_base();
        let style = Style { font_size: Some(18.0), ..Style::default() };
        let out   = apply_style(&base, &style, false);
        assert_eq!(out.font_size, 18.0);
    }

    #[test]
    fn font_weight_overrides_parent() {
        let base  = default_base();
        let style = Style { font_weight: Some(FontWeight::Bold), ..Style::default() };
        let out   = apply_style(&base, &style, false);
        assert_eq!(out.font_weight, FontWeight::Bold);
    }

    #[test]
    fn text_align_overrides_parent() {
        let base  = default_base();
        let style = Style { text_align: Some(TextAlign::Center), ..Style::default() };
        let out   = apply_style(&base, &style, false);
        assert_eq!(out.text_align, TextAlign::Center);
    }

    #[test]
    fn font_family_overrides_parent() {
        let base  = default_base();
        let style = Style { font_family: Some("Courier".into()), ..Style::default() };
        let out   = apply_style(&base, &style, false);
        assert_eq!(out.font_family.as_deref(), Some("Courier"));
    }

    #[test]
    fn underline_overrides_parent() {
        let base  = default_base();
        let style = Style { underline: Some(true), ..Style::default() };
        let out   = apply_style(&base, &style, false);
        assert!(out.underline);
    }

    // ── CSS colour parsing ────────────────────────────────────────────────────

    #[test]
    fn hex_color_is_applied() {
        let base  = default_base();
        let style = Style { color: Some("#ff0000".into()), ..Style::default() };
        let out   = apply_style(&base, &style, false);
        // Red channel should be close to 1.0 in sRGB.
        assert!(out.color.0 > 0.9, "red channel expected ≈ 1.0, got {}", out.color.0);
        assert!(out.color.1 < 0.1, "green channel expected ≈ 0.0");
        assert!(out.color.2 < 0.1, "blue channel expected ≈ 0.0");
    }

    #[test]
    fn invalid_color_falls_back_to_parent() {
        let base  = default_base(); // color = black
        let style = Style { color: Some("not-a-color".into()), ..Style::default() };
        let out   = apply_style(&base, &style, false);
        assert_eq!(out.color, (0.0, 0.0, 0.0),
            "invalid colour string should inherit parent colour");
    }

    // ── all_black ─────────────────────────────────────────────────────────────

    #[test]
    fn all_black_overrides_red_color() {
        let base  = default_base();
        let style = Style { color: Some("#ff0000".into()), ..Style::default() };
        let out   = apply_style(&base, &style, /* all_black */ true);
        assert_eq!(out.color, (0.0, 0.0, 0.0),
            "all_black should force color to (0,0,0) even when style sets red");
    }

    #[test]
    fn all_black_on_none_color_stays_black() {
        let base  = default_base();
        let out   = apply_style(&base, &Style::default(), /* all_black */ true);
        assert_eq!(out.color, (0.0, 0.0, 0.0));
    }

    // ── Cascade chain PrintConfig → Section → Question ────────────────────────

    #[test]
    fn three_layer_cascade() {
        let cfg  = PrintConfig { font_size: 12.0, ..PrintConfig::default() };
        let base = base_style(&cfg);

        // Section overrides font size.
        let section_style = Style { font_size: Some(14.0), ..Style::default() };
        let after_section = apply_style(&base, &section_style, false);
        assert_eq!(after_section.font_size, 14.0);

        // Question overrides weight; inherits section font_size.
        let question_style = Style { font_weight: Some(FontWeight::Bold), ..Style::default() };
        let after_question = apply_style(&after_section, &question_style, false);
        assert_eq!(after_question.font_size,   14.0, "font_size should be inherited from section");
        assert_eq!(after_question.font_weight, FontWeight::Bold);
    }

    #[test]
    fn all_black_propagates_through_cascade() {
        let base = default_base();

        // Section sets blue; all_black must override it at every level.
        let section_style  = Style { color: Some("#0000ff".into()), ..Style::default() };
        let after_section  = apply_style(&base, &section_style, /* all_black */ true);
        assert_eq!(after_section.color, (0.0, 0.0, 0.0));

        // Question sets red; all_black must still win.
        let question_style = Style { color: Some("#ff0000".into()), ..Style::default() };
        let after_question = apply_style(&after_section, &question_style, /* all_black */ true);
        assert_eq!(after_question.color, (0.0, 0.0, 0.0));
    }

    // ── apply_opt_style ───────────────────────────────────────────────────────

    #[test]
    fn opt_style_none_inherits_parent() {
        let base = default_base();
        let out  = apply_opt_style(&base, None, false);
        assert_eq!(out.font_size, base.font_size);
        assert_eq!(out.color,     base.color);
    }

    #[test]
    fn opt_style_none_all_black_forces_black() {
        let mut base  = default_base();
        base.color    = (1.0, 0.0, 0.0); // set red first
        let out       = apply_opt_style(&base, None, /* all_black */ true);
        assert_eq!(out.color, (0.0, 0.0, 0.0),
            "apply_opt_style(None, all_black=true) must still force black");
    }
}
