//! Positioned layout fragments — output of the layout engine.
//!
//! A Fragment is an atomic, positioned rendering unit. The layout engine
//! produces a flat list of Fragments for each page; the PDF emitter
//! translates them into content stream operators.

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
    /// A filled rectangle (background, answer box, etc.).
    FilledRect(FilledRect),
    /// A stroked rectangle (border box).
    StrokedRect(StrokedRect),
    /// An embedded image (raster).
    Image(ImageFragment),
    /// Vertical whitespace (no content, used for spacing).
    Spacer,
}

/// A run of shaped glyphs to be rendered with a single font face.
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
    pub font_family: String,
    /// Font variant index: 0=regular, 1=bold, 2=italic, 3=bold-italic.
    pub variant: u8,
    /// Fill color as CSS string (e.g., "#000000").
    pub color: String,
    /// Baseline offset from the top of the fragment box, in points.
    pub baseline_offset: f64,
}

/// A horizontal line.
#[derive(Debug, Clone)]
pub struct HRule {
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

/// An embedded raster image.
#[derive(Debug, Clone)]
pub struct ImageFragment {
    /// Key into the image store.
    pub key: String,
}
