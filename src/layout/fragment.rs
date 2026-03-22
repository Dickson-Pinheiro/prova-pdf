//! Positioned layout fragments — output of the layout engine.
//!
//! A Fragment is an atomic, positioned rendering unit. The layout engine
//! produces a flat list of Fragments for each page; the PDF emitter
//! translates them into content stream operators.

use std::rc::Rc;

/// A positioned, atomic rendering unit.
#[derive(Debug, Clone)]
pub struct Fragment {
    /// X position in PDF points from left edge of content area.
    pub x: f64,
    /// Y position in PDF points from top of content area (grows downward).
    pub y: f64,
    /// Width of the fragment in points.
    pub width: f64,
    /// Height of the fragment in points.
    pub height: f64,
    /// The content to render.
    pub kind: FragmentKind,
}

/// The content type of a Fragment.
#[derive(Debug, Clone)]
pub enum FragmentKind {
    /// A run of shaped glyphs from a single font+size+color.
    GlyphRun(GlyphRun),
    /// A horizontal rule (line).
    HRule(HRule),
    /// A vertical rule (line).
    VRule(VRule),
    /// A filled rectangle (background, answer box, etc.).
    FilledRect(FilledRect),
    /// A stroked rectangle (border box).
    StrokedRect(StrokedRect),
    /// A solid filled circle (used for question number badges).
    FilledCircle(FilledCircle),
    /// An embedded image (raster).
    Image(ImageFragment),
    /// Vertical whitespace (no content, used for spacing).
    Spacer,
}

/// A run of shaped glyphs to be rendered with a single font face.
///
/// `font_family` and `color` use `Rc<str>` to avoid per-word String
/// allocations in the inline layout hot path — cloning an `Rc<str>`
/// is just a reference-count increment.
#[derive(Debug, Clone)]
pub struct GlyphRun {
    /// Glyph IDs in render order.
    pub glyph_ids: Vec<u16>,
    /// Per-glyph x-advance in font units.
    pub x_advances: Vec<i32>,
    /// Per-glyph x-offset in font units.
    pub x_offsets: Vec<i32>,
    /// Per-glyph y-offset in font units.
    pub y_offsets: Vec<i32>,
    /// Font size in points.
    pub font_size: f64,
    /// Named font family (resolved from FontRegistry).
    pub font_family: Rc<str>,
    /// Font variant index: 0=regular, 1=bold, 2=italic, 3=bold-italic.
    pub variant: u8,
    /// Fill color as CSS string (e.g., "#000000").
    pub color: Rc<str>,
    /// Baseline offset from the top of the fragment box, in points.
    pub baseline_offset: f64,
}

impl GlyphRun {
    /// Build a `GlyphRun` from pre-shaped glyphs.
    ///
    /// This is the canonical way to construct a `GlyphRun` from the output
    /// of `shape_text()`. Using this constructor avoids duplicating the
    /// glyph-field extraction pattern across 20+ call sites.
    pub fn from_shaped(
        glyphs: &[crate::layout::text::ShapedGlyph],
        font_size: f64,
        font_family: Rc<str>,
        variant: u8,
        color: Rc<str>,
        baseline_offset: f64,
    ) -> Self {
        Self {
            glyph_ids:   glyphs.iter().map(|g| g.glyph_id).collect(),
            x_advances:  glyphs.iter().map(|g| g.x_advance).collect(),
            x_offsets:   glyphs.iter().map(|g| g.x_offset).collect(),
            y_offsets:   glyphs.iter().map(|g| g.y_offset).collect(),
            font_size,
            font_family,
            variant,
            color,
            baseline_offset,
        }
    }
}

/// A horizontal line.
#[derive(Debug, Clone)]
pub struct HRule {
    /// Stroke width in points.
    pub stroke_width: f64,
    /// Stroke color as CSS string.
    pub color: String,
}

/// A vertical line (used for column separators, etc.).
#[derive(Debug, Clone)]
pub struct VRule {
    /// Stroke width in points.
    pub stroke_width: f64,
    /// Stroke color as CSS string.
    pub color: String,
}

/// A solid filled rectangle.
#[derive(Debug, Clone)]
pub struct FilledRect {
    /// Fill color as CSS string.
    pub color: String,
}

/// A stroked (outlined) rectangle.
#[derive(Debug, Clone)]
pub struct StrokedRect {
    /// Stroke width in points.
    pub stroke_width: f64,
    /// Stroke color as CSS string.
    pub color: String,
    /// Dash pattern: None = solid, Some([on, off]) = dashed.
    pub dash: Option<[f64; 2]>,
}

/// A solid filled circle (used for question number and alternative badges).
///
/// Rendered as a circle inscribed in the fragment bounding box (radius = width/2).
#[derive(Debug, Clone)]
pub struct FilledCircle {
    /// Fill color as CSS string.
    pub color: String,
}

/// An embedded raster image.
#[derive(Debug, Clone)]
pub struct ImageFragment {
    /// Key into the image store.
    pub key: String,
}
