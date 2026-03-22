//! PDF drawing operators for geometric fragment types.
//!
//! Each public `emit_*` function writes operators directly into a [`Content`]
//! stream builder, wrapped in `q`/`Q` (save/restore) so they cannot pollute
//! the surrounding graphics state.
//!
//! # Coordinate conversion
//!
//! The layout engine uses **top-left origin** (y grows down).  PDF uses
//! **bottom-left origin** (y grows up).  All functions receive `ph`
//! (page height in points) and apply the flip:
//!
//! ```text
//! pdf_y_bottom = ph − layout_y − fragment_height
//! ```

use pdf_writer::Content;

use crate::layout::fragment::{FilledCircle, FilledRect, HRule, StrokedRect, VRule};

// ─────────────────────────────────────────────────────────────────────────────
// Color helper
// ─────────────────────────────────────────────────────────────────────────────

/// Parse `"#RRGGBB"` or `"#rrggbb"` into normalised `(r, g, b)` floats in
/// the range `[0.0, 1.0]`.  Falls back to opaque black on any parse error.
pub fn parse_hex_color(s: &str) -> (f32, f32, f32) {
    let s = s.trim_start_matches('#');
    if s.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&s[0..2], 16),
            u8::from_str_radix(&s[2..4], 16),
            u8::from_str_radix(&s[4..6], 16),
        ) {
            return (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
        }
    }
    (0.0, 0.0, 0.0)
}

// ─────────────────────────────────────────────────────────────────────────────
// HRule
// ─────────────────────────────────────────────────────────────────────────────

/// Emit a horizontal line (`HRule`) into the content stream.
///
/// The line is drawn at the vertical centre of the fragment box.  Operators
/// emitted: `q  w  RG  m  l  S  Q`.
pub fn emit_hrule(
    content: &mut Content,
    frag_x:  f64,
    frag_y:  f64,
    frag_w:  f64,
    frag_h:  f64,
    hrule:   &HRule,
    ph:      f64,
) {
    // Centre-line of the fragment in PDF y-up coordinates.
    let y = (ph - frag_y - frag_h / 2.0) as f32;
    let x1 = frag_x as f32;
    let x2 = (frag_x + frag_w) as f32;

    let (r, g, b) = parse_hex_color(&hrule.color);

    content.save_state();
    content.set_line_width(hrule.stroke_width as f32);
    content.set_stroke_rgb(r, g, b);
    content.move_to(x1, y);
    content.line_to(x2, y);
    content.stroke();
    content.restore_state();
}

// ─────────────────────────────────────────────────────────────────────────────
// VRule
// ─────────────────────────────────────────────────────────────────────────────

/// Emit a vertical line (`VRule`) into the content stream.
///
/// The line is drawn at the horizontal centre of the fragment box.
/// Operators emitted: `q  w  RG  m  l  S  Q`.
pub fn emit_vrule(
    content: &mut Content,
    frag_x:  f64,
    frag_y:  f64,
    _frag_w: f64,
    frag_h:  f64,
    vrule:   &VRule,
    ph:      f64,
) {
    let x = frag_x as f32;
    let y1 = (ph - frag_y) as f32;
    let y2 = (ph - frag_y - frag_h) as f32;

    let (r, g, b) = parse_hex_color(&vrule.color);

    content.save_state();
    content.set_line_width(vrule.stroke_width as f32);
    content.set_stroke_rgb(r, g, b);
    content.move_to(x, y1);
    content.line_to(x, y2);
    content.stroke();
    content.restore_state();
}

// ─────────────────────────────────────────────────────────────────────────────
// FilledCircle
// ─────────────────────────────────────────────────────────────────────────────

/// Emit a solid filled circle (`FilledCircle`) inscribed in the fragment box.
///
/// Approximates a circle with 4 cubic Bézier segments (standard PDF technique).
/// κ ≈ 0.5523 is the control point distance for a quarter-circle.
pub fn emit_filled_circle(
    content: &mut Content,
    frag_x:  f64,
    frag_y:  f64,
    frag_w:  f64,
    frag_h:  f64,
    circle:  &FilledCircle,
    ph:      f64,
) {
    let r = (frag_w.min(frag_h)) / 2.0;
    let cx = frag_x + frag_w / 2.0;
    let cy = ph - frag_y - frag_h / 2.0; // PDF y-up

    let (cr, cg, cb) = parse_hex_color(&circle.color);

    // κ for Bézier approximation of a quarter circle
    const KAPPA: f64 = 0.5522847498;
    let k = r * KAPPA;

    let cx = cx as f32;
    let cy = cy as f32;
    let r = r as f32;
    let k = k as f32;

    content.save_state();
    content.set_fill_rgb(cr, cg, cb);

    // Start at top of circle
    content.move_to(cx, cy + r);
    // Top-right quarter
    content.cubic_to(cx + k, cy + r, cx + r, cy + k, cx + r, cy);
    // Bottom-right quarter
    content.cubic_to(cx + r, cy - k, cx + k, cy - r, cx, cy - r);
    // Bottom-left quarter
    content.cubic_to(cx - k, cy - r, cx - r, cy - k, cx - r, cy);
    // Top-left quarter
    content.cubic_to(cx - r, cy + k, cx - k, cy + r, cx, cy + r);

    content.close_path();
    content.fill_nonzero();
    content.restore_state();
}

// ─────────────────────────────────────────────────────────────────────────────
// FilledRect
// ─────────────────────────────────────────────────────────────────────────────

/// Emit a solid filled rectangle (`FilledRect`) into the content stream.
///
/// Operators emitted: `q  rg  re  f  Q`.
pub fn emit_filled_rect(
    content: &mut Content,
    frag_x:  f64,
    frag_y:  f64,
    frag_w:  f64,
    frag_h:  f64,
    rect:    &FilledRect,
    ph:      f64,
) {
    // PDF `re` operator takes bottom-left corner.
    let x = frag_x as f32;
    let y = (ph - frag_y - frag_h) as f32;
    let w = frag_w as f32;
    let h = frag_h as f32;

    let (r, g, b) = parse_hex_color(&rect.color);

    content.save_state();
    content.set_fill_rgb(r, g, b);
    content.rect(x, y, w, h);
    content.fill_nonzero();
    content.restore_state();
}

// ─────────────────────────────────────────────────────────────────────────────
// StrokedRect
// ─────────────────────────────────────────────────────────────────────────────

/// Emit a stroked (outlined) rectangle (`StrokedRect`) into the content stream.
///
/// If `rect.dash` is `Some([on, off])`, a dash pattern `[on off] 0 d` is set
/// before stroking.  Operators emitted: `q  w  [d]  RG  re  S  Q`.
pub fn emit_stroked_rect(
    content: &mut Content,
    frag_x:  f64,
    frag_y:  f64,
    frag_w:  f64,
    frag_h:  f64,
    rect:    &StrokedRect,
    ph:      f64,
) {
    let x = frag_x as f32;
    let y = (ph - frag_y - frag_h) as f32;
    let w = frag_w as f32;
    let h = frag_h as f32;

    let (r, g, b) = parse_hex_color(&rect.color);

    content.save_state();
    content.set_line_width(rect.stroke_width as f32);

    // Apply dash pattern before stroking.
    if let Some([on, off]) = rect.dash {
        content.set_dash_pattern([on as f32, off as f32], 0.0);
    }

    content.set_stroke_rgb(r, g, b);
    content.rect(x, y, w, h);
    content.stroke();
    content.restore_state();
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_hex_color ───────────────────────────────────────────────────────

    #[test]
    fn color_black() {
        assert_eq!(parse_hex_color("#000000"), (0.0, 0.0, 0.0));
    }

    #[test]
    fn color_white() {
        let (r, g, b) = parse_hex_color("#FFFFFF");
        assert!((r - 1.0).abs() < 1e-6);
        assert!((g - 1.0).abs() < 1e-6);
        assert!((b - 1.0).abs() < 1e-6);
    }

    #[test]
    fn color_red_uppercase() {
        let (r, g, b) = parse_hex_color("#FF0000");
        assert!((r - 1.0).abs() < 1e-6);
        assert_eq!(g, 0.0);
        assert_eq!(b, 0.0);
    }

    #[test]
    fn color_red_lowercase() {
        let (r, g, b) = parse_hex_color("#ff0000");
        assert!((r - 1.0).abs() < 1e-6);
    }

    #[test]
    fn color_invalid_returns_black() {
        assert_eq!(parse_hex_color("bad"),     (0.0, 0.0, 0.0));
        assert_eq!(parse_hex_color(""),        (0.0, 0.0, 0.0));
        assert_eq!(parse_hex_color("#12345"),  (0.0, 0.0, 0.0));
    }

    // ── Content stream helpers ────────────────────────────────────────────────

    fn stream_bytes(content: Content) -> Vec<u8> {
        content.finish().into_vec()
    }

    fn make_hrule(color: &str) -> HRule {
        HRule { stroke_width: 0.5, color: color.into() }
    }
    fn make_filled_rect(color: &str) -> FilledRect {
        FilledRect { color: color.into() }
    }
    fn make_stroked_rect(color: &str, dash: Option<[f64; 2]>) -> StrokedRect {
        StrokedRect { stroke_width: 1.0, color: color.into(), dash }
    }

    // ── emit_hrule ────────────────────────────────────────────────────────────

    #[test]
    fn hrule_contains_move_line_stroke() {
        let mut c = Content::new();
        emit_hrule(&mut c, 0.0, 50.0, 400.0, 0.5, &make_hrule("#000000"), 841.89);
        let bytes = stream_bytes(c);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains(" m\n") || s.contains(" m "), "must contain moveto");
        assert!(s.contains(" l\n") || s.contains(" l "), "must contain lineto");
        assert!(s.contains(" S\n") || s.contains('S'),   "must contain stroke");
    }

    #[test]
    fn hrule_wrapped_in_save_restore() {
        let mut c = Content::new();
        emit_hrule(&mut c, 0.0, 50.0, 200.0, 1.0, &make_hrule("#000000"), 841.89);
        let bytes = stream_bytes(c);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.starts_with("q\n"), "must start with save state");
        assert!(s.trim_end().ends_with('Q'), "must end with restore state");
    }

    #[test]
    fn hrule_sets_stroke_color() {
        let mut c = Content::new();
        emit_hrule(&mut c, 0.0, 0.0, 100.0, 0.5, &make_hrule("#FF0000"), 100.0);
        let bytes = stream_bytes(c);
        let s = std::str::from_utf8(&bytes).unwrap();
        // Red: RG with 1.0000 0.0000 0.0000
        assert!(s.contains("1 0 0 RG") || s.contains("1.0") && s.contains("RG"),
            "must set stroke color RG");
    }

    #[test]
    fn hrule_coordinate_flip() {
        // frag_y=100, frag_h=0, ph=841.89 → pdf_y = 841.89 - 100 - 0 = 741.89
        let mut c = Content::new();
        emit_hrule(&mut c, 0.0, 100.0, 400.0, 0.0, &make_hrule("#000000"), 841.89);
        let bytes = stream_bytes(c);
        let s = std::str::from_utf8(&bytes).unwrap();
        // The y coordinate in move_to should be near 741.89
        assert!(s.contains("741"), "y-flip: pdf_y should be near 741");
    }

    // ── emit_filled_rect ──────────────────────────────────────────────────────

    #[test]
    fn filled_rect_contains_re_and_fill() {
        let mut c = Content::new();
        emit_filled_rect(&mut c, 10.0, 20.0, 200.0, 50.0, &make_filled_rect("#CCCCCC"), 841.89);
        let bytes = stream_bytes(c);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains(" re\n") || s.contains(" re "), "must contain re operator");
        // fill_nonzero() emits "f\n" on its own line (preceded by "\n" not " ").
        assert!(s.contains("f\n"), "must contain fill operator f");
    }

    #[test]
    fn filled_rect_wrapped_in_save_restore() {
        let mut c = Content::new();
        emit_filled_rect(&mut c, 0.0, 0.0, 100.0, 50.0, &make_filled_rect("#000000"), 841.89);
        let bytes = stream_bytes(c);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.starts_with("q\n"), "must start with q");
        assert!(s.trim_end().ends_with('Q'), "must end with Q");
    }

    #[test]
    fn filled_rect_sets_fill_color() {
        let mut c = Content::new();
        emit_filled_rect(&mut c, 0.0, 0.0, 100.0, 50.0, &make_filled_rect("#FF0000"), 841.89);
        let bytes = stream_bytes(c);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("rg"), "must set fill color rg");
    }

    #[test]
    fn filled_rect_coordinate_flip() {
        // frag_y=100, frag_h=50, ph=841.89 → pdf_y_bottom = 841.89 - 100 - 50 = 691.89
        let mut c = Content::new();
        emit_filled_rect(&mut c, 0.0, 100.0, 200.0, 50.0, &make_filled_rect("#000000"), 841.89);
        let bytes = stream_bytes(c);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("691"), "y-flip: bottom-left y should be near 691");
    }

    // ── emit_stroked_rect ─────────────────────────────────────────────────────

    #[test]
    fn stroked_rect_contains_re_and_stroke() {
        let mut c = Content::new();
        emit_stroked_rect(&mut c, 10.0, 20.0, 200.0, 50.0, &make_stroked_rect("#000000", None), 841.89);
        let bytes = stream_bytes(c);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains(" re\n") || s.contains(" re "), "must contain re operator");
        assert!(s.contains(" S\n") || s.contains('S'), "must contain stroke operator");
    }

    #[test]
    fn stroked_rect_wrapped_in_save_restore() {
        let mut c = Content::new();
        emit_stroked_rect(&mut c, 0.0, 0.0, 100.0, 50.0, &make_stroked_rect("#000000", None), 841.89);
        let bytes = stream_bytes(c);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.starts_with("q\n"), "must start with q");
        assert!(s.trim_end().ends_with('Q'), "must end with Q");
    }

    #[test]
    fn stroked_rect_no_dash_no_d_operator() {
        let mut c = Content::new();
        emit_stroked_rect(&mut c, 0.0, 0.0, 100.0, 50.0, &make_stroked_rect("#000000", None), 841.89);
        let bytes = stream_bytes(c);
        let s = std::str::from_utf8(&bytes).unwrap();
        // Without dash, no `d` operator should appear.
        assert!(!s.contains(" d\n") && !s.contains("] 0 d"),
            "solid rect must not contain dash operator");
    }

    #[test]
    fn stroked_rect_with_dash_emits_d_operator() {
        let mut c = Content::new();
        emit_stroked_rect(
            &mut c, 0.0, 0.0, 100.0, 50.0,
            &make_stroked_rect("#000000", Some([4.0, 4.0])),
            841.89,
        );
        let bytes = stream_bytes(c);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains(" d\n") || s.contains("] 0 d") || s.contains("d\n"),
            "dashed rect must contain dash operator d");
    }

    #[test]
    fn stroked_rect_dash_values_appear_in_stream() {
        let mut c = Content::new();
        emit_stroked_rect(
            &mut c, 0.0, 0.0, 100.0, 50.0,
            &make_stroked_rect("#000000", Some([4.0, 4.0])),
            841.89,
        );
        let bytes = stream_bytes(c);
        let s = std::str::from_utf8(&bytes).unwrap();
        // The dash array [4 4] should appear in the stream.
        assert!(s.contains("4"), "dash on/off values must appear in stream");
    }

    #[test]
    fn stroked_rect_different_dash_produces_different_stream() {
        let mut c1 = Content::new();
        emit_stroked_rect(&mut c1, 0.0, 0.0, 100.0, 50.0,
            &make_stroked_rect("#000000", Some([4.0, 4.0])), 841.89);

        let mut c2 = Content::new();
        emit_stroked_rect(&mut c2, 0.0, 0.0, 100.0, 50.0,
            &make_stroked_rect("#000000", Some([8.0, 2.0])), 841.89);

        assert_ne!(stream_bytes(c1), stream_bytes(c2),
            "different dash patterns must produce different streams");
    }

    #[test]
    fn stroked_rect_sets_stroke_color() {
        let mut c = Content::new();
        emit_stroked_rect(&mut c, 0.0, 0.0, 100.0, 50.0,
            &make_stroked_rect("#FF0000", None), 841.89);
        let bytes = stream_bytes(c);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("RG"), "must set stroke color with RG");
    }
}
