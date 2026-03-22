use crate::layout::fragment::Fragment;
use crate::spec::config::PrintConfig;

const CM_TO_PT: f64 = 28.3465;
/// 35 CSS-px × 0.75 pt/px = 26.25pt (matches lize CSS `column-gap: 35px`).
const COLUMN_GAP_PT: f64 = 26.25;

/// Computed page geometry in PDF points (1 pt = 1/72 inch).
///
/// All values are derived from `PrintConfig` once and shared across layout phases.
#[derive(Debug, Clone)]
pub struct PageGeometry {
    pub page_width_pt:     f64,
    pub page_height_pt:    f64,
    pub margin_top_pt:     f64,
    pub margin_bottom_pt:  f64,
    pub margin_left_pt:    f64,
    pub margin_right_pt:   f64,
    /// Usable width between left and right margins.
    pub content_width_pt:  f64,
    /// Usable height between top and bottom margins.
    pub content_height_pt: f64,
    pub columns:           u8,
    pub column_gap_pt:     f64,
    /// Width of a single column (= content_width when columns == 1).
    pub column_width_pt:   f64,
}

impl PageGeometry {
    pub fn from_config(cfg: &PrintConfig) -> Self {
        let page_width_pt  = cfg.page_size.width_pt();
        let page_height_pt = cfg.page_size.height_pt();

        let margin_top_pt    = cfg.margins.top    * CM_TO_PT;
        let margin_bottom_pt = cfg.margins.bottom * CM_TO_PT;
        let margin_left_pt   = cfg.margins.left   * CM_TO_PT;
        let margin_right_pt  = cfg.margins.right  * CM_TO_PT;

        let content_width_pt  = page_width_pt  - margin_left_pt  - margin_right_pt;
        let content_height_pt = page_height_pt - margin_top_pt   - margin_bottom_pt;

        let columns = cfg.columns.max(1);
        let column_gap_pt = if columns > 1 { COLUMN_GAP_PT } else { 0.0 };
        let column_width_pt = if columns > 1 {
            (content_width_pt - column_gap_pt * (columns as f64 - 1.0)) / columns as f64
        } else {
            content_width_pt
        };

        Self {
            page_width_pt,
            page_height_pt,
            margin_top_pt,
            margin_bottom_pt,
            margin_left_pt,
            margin_right_pt,
            content_width_pt,
            content_height_pt,
            columns,
            column_gap_pt,
            column_width_pt,
        }
    }

    /// X offset (from left margin) where column `col` starts (0-indexed).
    pub fn column_x(&self, col: u8) -> f64 {
        col as f64 * (self.column_width_pt + self.column_gap_pt)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PageComposer
// ─────────────────────────────────────────────────────────────────────────────

/// Accumulates pre-laid-out blocks into pages, handling pagination and
/// two-column balancing.
///
/// Coordinates contract for `push_block`:
///   - Fragment.x — from the left edge of the column (0 = column start)
///   - Fragment.y — from the top of the block (0 = block start)
///
/// `push_block` translates both axes to absolute content-area coordinates
/// before appending to the current page.
pub struct PageComposer {
    geometry:      PageGeometry,
    cursor_y:      f64,
    /// Minimum y from which column content may start on the current page.
    /// Updated whenever a full-width block (e.g. the institutional header) is
    /// placed, so that column 1 never overlaps content already at the top.
    column_top_y:  f64,
    /// cursor_y recorded when leaving column 0 for column 1.
    /// Used by `push_block_full_width` to place full-width blocks below
    /// *all* column content, not just the current column's cursor.
    col0_exit_y:   f64,
    current_col:   u8,
    current_page:  Vec<Fragment>,
    pages:         Vec<Vec<Fragment>>,
    /// Y ranges `(start, end)` of full-width blocks on the current page.
    /// Used to split the column separator so it doesn't cross full-width content.
    fw_ranges:         Vec<(f64, f64)>,
    /// Collected full-width ranges per finished page.
    fw_ranges_per_page: Vec<Vec<(f64, f64)>>,
}

impl PageComposer {
    pub fn new(geometry: PageGeometry) -> Self {
        Self {
            geometry,
            cursor_y:     0.0,
            column_top_y: 0.0,
            col0_exit_y:  0.0,
            current_col:  0,
            current_page: Vec::new(),
            pages:        Vec::new(),
            fw_ranges:         Vec::new(),
            fw_ranges_per_page: Vec::new(),
        }
    }

    /// Current column width (same as `geometry.column_width_pt`).
    pub fn column_width(&self) -> f64 {
        self.geometry.column_width_pt
    }

    /// Effective column width for a question block.
    ///
    /// - `full_width: true`  → `content_width_pt` (spans both columns / full content area).
    /// - `full_width: false` → `column_width_pt` (normal column width).
    pub fn effective_column_width(&self, full_width: bool) -> f64 {
        if full_width {
            self.geometry.content_width_pt
        } else {
            self.geometry.column_width_pt
        }
    }

    /// Return a `ColumnGeometry` suitable for laying out a question.
    ///
    /// Equivalent to `ColumnGeometry { column_width_pt: self.effective_column_width(full_width) }`.
    pub fn column_geom_for(&self, full_width: bool) -> crate::layout::question::ColumnGeometry {
        crate::layout::question::ColumnGeometry {
            column_width_pt: self.effective_column_width(full_width),
        }
    }

    /// Height still available in the current column before overflow.
    pub fn available_height(&self) -> f64 {
        self.geometry.content_height_pt - self.cursor_y
    }

    /// Absolute X offset of the current column inside the content area.
    fn col_x_offset(&self) -> f64 {
        self.geometry.column_x(self.current_col)
    }

    /// Flush the current column/page and start a fresh one.
    /// Pages with no fragments are not recorded (they carry no content).
    pub fn new_page(&mut self) {
        let page = std::mem::take(&mut self.current_page);
        let ranges = std::mem::take(&mut self.fw_ranges);
        if !page.is_empty() {
            self.pages.push(page);
            self.fw_ranges_per_page.push(ranges);
        }
        self.cursor_y     = 0.0;
        self.column_top_y = 0.0;
        self.col0_exit_y  = 0.0;
        self.current_col  = 0;
    }

    /// Advance to the next column; starts a new page if already in the last column.
    ///
    /// When switching columns, `cursor_y` resets to `column_top_y` (not 0) so
    /// that column content never overlaps full-width blocks (e.g. the header)
    /// already placed at the top of the page.
    fn next_column(&mut self) {
        if self.current_col + 1 < self.geometry.columns {
            // Save col 0's exit position so full-width blocks can start below it.
            if self.current_col == 0 {
                self.col0_exit_y = self.cursor_y;
            }
            self.current_col += 1;
            self.cursor_y     = self.column_top_y;
        } else {
            self.new_page();
        }
    }

    /// Append a block of pre-positioned fragments to the current page/column.
    ///
    /// `height`    — total height of the block in points.
    /// `fragments` — fragments with x relative to column origin and y relative
    ///               to block top. Both axes are translated to content-area
    ///               absolute coordinates before insertion.
    ///
    /// Overflow behaviour:
    ///   - Multi-column, not last column → advance to next column first.
    ///   - Otherwise → start a new page.
    ///
    /// Column balancing (2-column mode): after placing a block, if cursor_y
    /// has reached the halfway point of the content height and we are still in
    /// column 0, the composer advances to column 1 automatically.
    pub fn push_block(&mut self, height: f64, mut fragments: Vec<Fragment>) {
        // Handle overflow: try next column before starting a new page.
        if self.cursor_y + height > self.geometry.content_height_pt {
            self.next_column();
        }

        let x_off = self.col_x_offset();
        let y_off = self.cursor_y;

        for f in &mut fragments {
            f.x += x_off;
            f.y += y_off;
        }

        self.current_page.extend(fragments);
        self.cursor_y += height;

        // Two-column balancing: move to col 1 once we pass the halfway point of
        // the space available *below* any full-width content (column_top_y).
        let balance_threshold = self.column_top_y
            + (self.geometry.content_height_pt - self.column_top_y) * 0.7;

        if self.geometry.columns > 1
            && self.current_col == 0
            && self.cursor_y >= balance_threshold
        {
            self.next_column();
        }
    }

    /// Append a full-width block that spans the entire content area.
    ///
    /// Unlike [`push_block`], no column x-offset is applied — the caller must
    /// have laid out the block using [`effective_column_width(true)`] so that
    /// fragment x coordinates are already relative to the content-area left edge.
    ///
    /// After placing a full-width block the composer resets to column 0 so that
    /// subsequent normal blocks start from the left column again.
    ///
    /// `LeftOfQuestion` / `RightOfQuestion` base texts produce full-width blocks
    /// via [`crate::layout::base_text::layout_side_by_side`] and must use this method.
    pub fn push_block_full_width(&mut self, height: f64, mut fragments: Vec<Fragment>) {
        // A full-width block must start below ALL columns' content, not just
        // the current column's cursor.  When in column 1, col0_exit_y holds
        // how far column 0 reached; take the max of both.
        let start_y = self.cursor_y.max(self.col0_exit_y);

        // Overflow check against the true starting point.
        if start_y + height > self.geometry.content_height_pt {
            self.new_page();
            // After new_page all cursors are reset to 0; start fresh.
            let y_off = self.cursor_y; // == column_top_y == 0 after new_page
            for f in &mut fragments {
                f.y += y_off;
            }
            self.current_page.extend(fragments);
            self.cursor_y     = y_off + height;
        } else {
            for f in &mut fragments {
                f.y += start_y;
            }
            self.current_page.extend(fragments);
            self.cursor_y = start_y + height;
        }

        // Record the Y range of this full-width block so the column rule
        // can be split around it.
        let fw_start = self.cursor_y - height;
        self.fw_ranges.push((fw_start, self.cursor_y));

        // A full-width block always resets layout to column 0, and advances
        // column_top_y so that subsequent column content starts below it.
        self.column_top_y = self.cursor_y;
        self.col0_exit_y  = self.cursor_y;
        self.current_col  = 0;
    }

    /// Flush the last page and return all collected pages.
    /// Each inner `Vec<Fragment>` represents one page in content-area coordinates.
    /// Flush the last page and return all collected pages plus per-page
    /// full-width Y ranges (used to split the column separator rule).
    pub fn finalize(mut self) -> (Vec<Vec<Fragment>>, Vec<Vec<(f64, f64)>>) {
        if !self.current_page.is_empty() {
            self.pages.push(self.current_page);
            self.fw_ranges_per_page.push(self.fw_ranges);
        }
        (self.pages, self.fw_ranges_per_page)
    }

    // ── Convenience helpers for the layout pipeline ───────────────────────────

    /// Force a page break before the next block (used by `force_page_break`
    /// on `Question` and `break_all_questions` in `PrintConfig`).
    pub fn force_break(&mut self) {
        // Only break if there is already content on the current page.
        if !self.current_page.is_empty() || self.cursor_y > 0.0 {
            self.new_page();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::config::{Margins, PageSize, PrintConfig};

    fn config_with_page(page_size: PageSize) -> PrintConfig {
        PrintConfig { page_size, ..PrintConfig::default() }
    }

    // ── A4 ────────────────────────────────────────────────────────────────────

    #[test]
    fn a4_page_dimensions() {
        let g = PageGeometry::from_config(&config_with_page(PageSize::A4));
        assert!((g.page_width_pt  - 595.276).abs() < 0.01, "A4 width");
        assert!((g.page_height_pt - 841.890).abs() < 0.01, "A4 height");
    }

    #[test]
    fn a4_content_width_default_margins() {
        let g = PageGeometry::from_config(&config_with_page(PageSize::A4));
        // Default margins: left=1.5, right=1.5 → 3.0 cm total horizontal
        let expected_content = 595.276 - 2.0 * 1.5 * CM_TO_PT;
        assert!((g.content_width_pt - expected_content).abs() < 0.01);
    }

    #[test]
    fn a4_content_height_default_margins() {
        let g = PageGeometry::from_config(&config_with_page(PageSize::A4));
        // Default margins: top=0.6, bottom=0.6 → 1.2 cm total vertical
        let expected_content = 841.890 - 2.0 * 0.6 * CM_TO_PT;
        assert!((g.content_height_pt - expected_content).abs() < 0.01);
    }

    // ── ATA ───────────────────────────────────────────────────────────────────

    #[test]
    fn ata_page_dimensions() {
        let g = PageGeometry::from_config(&config_with_page(PageSize::Ata));
        assert!((g.page_width_pt  - 566.929).abs() < 0.01, "ATA width");
        assert!((g.page_height_pt - 754.016).abs() < 0.01, "ATA height");
    }

    // ── Custom ────────────────────────────────────────────────────────────────

    #[test]
    fn custom_page_dimensions() {
        let g = PageGeometry::from_config(&config_with_page(
            PageSize::Custom { width_mm: 100.0, height_mm: 200.0 },
        ));
        assert!((g.page_width_pt  - 100.0 * 2.83465).abs() < 0.01);
        assert!((g.page_height_pt - 200.0 * 2.83465).abs() < 0.01);
    }

    // ── Margins ───────────────────────────────────────────────────────────────

    #[test]
    fn margins_are_converted_to_pt() {
        let cfg = PrintConfig {
            margins: Margins { top: 1.0, bottom: 2.0, left: 3.0, right: 4.0 },
            ..PrintConfig::default()
        };
        let g = PageGeometry::from_config(&cfg);
        assert!((g.margin_top_pt    - 1.0 * CM_TO_PT).abs() < 0.001);
        assert!((g.margin_bottom_pt - 2.0 * CM_TO_PT).abs() < 0.001);
        assert!((g.margin_left_pt   - 3.0 * CM_TO_PT).abs() < 0.001);
        assert!((g.margin_right_pt  - 4.0 * CM_TO_PT).abs() < 0.001);
    }

    #[test]
    fn content_width_equals_page_minus_margins() {
        let cfg = PrintConfig {
            margins: Margins { top: 1.0, bottom: 1.0, left: 2.0, right: 3.0 },
            ..PrintConfig::default()
        };
        let g = PageGeometry::from_config(&cfg);
        let expected = g.page_width_pt - g.margin_left_pt - g.margin_right_pt;
        assert!((g.content_width_pt - expected).abs() < 0.001);
    }

    // ── Single column ─────────────────────────────────────────────────────────

    #[test]
    fn single_column_width_equals_content_width() {
        let cfg = PrintConfig { columns: 1, ..PrintConfig::default() };
        let g = PageGeometry::from_config(&cfg);
        assert_eq!(g.columns, 1);
        assert_eq!(g.column_gap_pt, 0.0);
        assert!((g.column_width_pt - g.content_width_pt).abs() < 0.001);
    }

    // ── Two columns ───────────────────────────────────────────────────────────

    #[test]
    fn two_columns_have_gap_and_correct_width() {
        let cfg = PrintConfig { columns: 2, ..PrintConfig::default() };
        let g = PageGeometry::from_config(&cfg);
        assert_eq!(g.columns, 2);
        assert!((g.column_gap_pt - COLUMN_GAP_PT).abs() < 0.001);
        // Two columns + one gap must fill content width
        let reconstructed = g.column_width_pt * 2.0 + g.column_gap_pt;
        assert!((reconstructed - g.content_width_pt).abs() < 0.001);
    }

    #[test]
    fn column_x_offsets_are_correct() {
        let cfg = PrintConfig { columns: 2, ..PrintConfig::default() };
        let g = PageGeometry::from_config(&cfg);
        assert_eq!(g.column_x(0), 0.0);
        let expected_col1 = g.column_width_pt + g.column_gap_pt;
        assert!((g.column_x(1) - expected_col1).abs() < 0.001);
    }

    // ── PageComposer helpers ──────────────────────────────────────────────────

    fn spacer(height: f64) -> Fragment {
        Fragment { x: 0.0, y: 0.0, width: 0.0, height, kind: crate::layout::fragment::FragmentKind::Spacer }
    }

    fn composer_1col() -> PageComposer {
        let cfg = PrintConfig { columns: 1, ..PrintConfig::default() };
        PageComposer::new(PageGeometry::from_config(&cfg))
    }

    fn composer_2col() -> PageComposer {
        let cfg = PrintConfig { columns: 2, ..PrintConfig::default() };
        PageComposer::new(PageGeometry::from_config(&cfg))
    }

    // ── Single-page: all blocks fit ───────────────────────────────────────────

    #[test]
    fn single_page_no_overflow() {
        let mut c = composer_1col();
        c.push_block(50.0, vec![spacer(50.0)]);
        c.push_block(50.0, vec![spacer(50.0)]);
        let (pages, _fw) = c.finalize();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].len(), 2);
    }

    // ── Overflow → new page ───────────────────────────────────────────────────

    #[test]
    fn block_taller_than_page_creates_new_page() {
        let mut c = composer_1col();
        let content_h = c.geometry.content_height_pt;
        // Fill just under the limit, then push a block that overflows.
        c.push_block(content_h - 10.0, vec![spacer(content_h - 10.0)]);
        c.push_block(20.0, vec![spacer(20.0)]); // overflows → new page
        let (pages, _fw) = c.finalize();
        assert_eq!(pages.len(), 2, "overflow should create a second page");
    }

    #[test]
    fn multiple_overflows_produce_multiple_pages() {
        let mut c = composer_1col();
        let content_h = c.geometry.content_height_pt;
        for _ in 0..5 {
            c.push_block(content_h, vec![spacer(content_h)]);
        }
        let (pages, _fw) = c.finalize();
        assert_eq!(pages.len(), 5);
    }

    // ── Fragment coordinate translation ───────────────────────────────────────

    #[test]
    fn fragment_y_is_offset_by_cursor() {
        let mut c = composer_1col();
        c.push_block(30.0, vec![spacer(30.0)]);
        // Second block: its fragment starts at y=0 locally → should land at y=30.
        c.push_block(20.0, vec![Fragment { x: 0.0, y: 0.0, width: 10.0, height: 20.0, kind: crate::layout::fragment::FragmentKind::Spacer }]);
        let (pages, _fw) = c.finalize();
        let second = &pages[0][1];
        assert!((second.y - 30.0).abs() < 0.001, "y should be offset by first block height");
    }

    // ── force_break ───────────────────────────────────────────────────────────

    #[test]
    fn force_break_on_empty_page_is_noop() {
        let mut c = composer_1col();
        c.force_break();
        c.push_block(10.0, vec![spacer(10.0)]);
        let (pages, _fw) = c.finalize();
        assert_eq!(pages.len(), 1, "force_break on empty page should not create empty page");
    }

    #[test]
    fn force_break_after_content_creates_new_page() {
        let mut c = composer_1col();
        c.push_block(10.0, vec![spacer(10.0)]);
        c.force_break();
        c.push_block(10.0, vec![spacer(10.0)]);
        let (pages, _fw) = c.finalize();
        assert_eq!(pages.len(), 2);
    }

    // ── Two-column balancing ──────────────────────────────────────────────────

    #[test]
    fn two_columns_balance_at_threshold() {
        let mut c = composer_2col();
        let threshold = c.geometry.content_height_pt * 0.7;
        // Push a block that crosses the 60% balance threshold.
        c.push_block(threshold + 1.0, vec![spacer(threshold + 1.0)]);
        // Composer should have advanced to column 1 automatically.
        assert_eq!(c.current_col, 1, "should be in column 1 after crossing 60% threshold");
        assert!((c.cursor_y).abs() < 0.001, "cursor_y resets when switching column");
    }

    #[test]
    fn two_column_overflow_stays_on_same_page() {
        let mut c = composer_2col();
        let content_h = c.geometry.content_height_pt;
        // Fill col 0 (just under 60% threshold to avoid auto-balance).
        let threshold = content_h * 0.7;
        c.push_block(threshold - 1.0, vec![spacer(threshold - 1.0)]);
        // Now a block that overflows col 0 → should go to col 1, same page.
        c.push_block(content_h, vec![spacer(content_h)]);
        let (pages, _fw) = c.finalize();
        assert_eq!(pages.len(), 1, "overflow in col 0 should go to col 1, not new page");
    }

    #[test]
    fn two_column_overflow_in_last_col_creates_new_page() {
        let mut c = composer_2col();
        let content_h = c.geometry.content_height_pt;
        // Fill col 0 past 60% threshold → auto-balance moves to col 1.
        c.push_block(content_h * 0.7 + 1.0, vec![spacer(content_h * 0.7 + 1.0)]);
        assert_eq!(c.current_col, 1, "should be in col 1 after balance");
        // Add some content to col 1, then overflow it → new page.
        c.push_block(10.0, vec![spacer(10.0)]);
        c.push_block(content_h, vec![spacer(content_h)]); // overflows col 1 → new page
        let (pages, _fw) = c.finalize();
        assert_eq!(pages.len(), 2, "overflow in last column should start new page");
    }

    // ── full_width / TASK-022 ─────────────────────────────────────────────────

    /// Critério: questão full_width em layout 2 colunas ocupa content_width_pt.
    #[test]
    fn effective_column_width_full_width_returns_content_width_in_2col() {
        let cfg = PrintConfig { columns: 2, ..PrintConfig::default() };
        let c   = PageComposer::new(PageGeometry::from_config(&cfg));
        assert!(
            (c.effective_column_width(true) - c.geometry.content_width_pt).abs() < 0.001,
            "full_width=true should return content_width_pt ({:.2}), got {:.2}",
            c.geometry.content_width_pt,
            c.effective_column_width(true),
        );
    }

    #[test]
    fn effective_column_width_false_returns_column_width_in_2col() {
        let cfg = PrintConfig { columns: 2, ..PrintConfig::default() };
        let c   = PageComposer::new(PageGeometry::from_config(&cfg));
        assert!(
            (c.effective_column_width(false) - c.geometry.column_width_pt).abs() < 0.001,
            "full_width=false should return column_width_pt",
        );
    }

    #[test]
    fn effective_column_width_true_equals_false_in_1col() {
        let cfg = PrintConfig { columns: 1, ..PrintConfig::default() };
        let c   = PageComposer::new(PageGeometry::from_config(&cfg));
        assert!(
            (c.effective_column_width(true) - c.effective_column_width(false)).abs() < 0.001,
            "in 1-col mode full_width should make no difference",
        );
    }

    #[test]
    fn full_width_is_strictly_wider_than_column_in_2col() {
        let cfg = PrintConfig { columns: 2, ..PrintConfig::default() };
        let c   = PageComposer::new(PageGeometry::from_config(&cfg));
        assert!(
            c.effective_column_width(true) > c.effective_column_width(false),
            "content_width_pt should be wider than column_width_pt in 2-col mode",
        );
    }

    #[test]
    fn push_block_full_width_applies_no_column_x_offset() {
        let cfg = PrintConfig { columns: 2, ..PrintConfig::default() };
        let mut c = PageComposer::new(PageGeometry::from_config(&cfg));
        let frag = Fragment { x: 0.0, y: 0.0, width: c.geometry.content_width_pt, height: 10.0,
            kind: crate::layout::fragment::FragmentKind::Spacer };
        c.push_block_full_width(10.0, vec![frag]);
        let (pages, _fw) = c.finalize();
        let placed = &pages[0][0];
        // x should remain 0 (no column offset applied).
        assert!((placed.x).abs() < 0.001,
            "full-width block should have x=0, got x={:.2}", placed.x);
    }

    #[test]
    fn push_block_full_width_resets_to_col_0() {
        let cfg = PrintConfig { columns: 2, ..PrintConfig::default() };
        let mut c = PageComposer::new(PageGeometry::from_config(&cfg));
        // Advance to col 1 via balancing (60% threshold).
        let threshold = c.geometry.content_height_pt * 0.7;
        c.push_block(threshold + 1.0, vec![]);
        assert_eq!(c.current_col, 1, "should be in col 1 before full-width block");

        c.push_block_full_width(10.0, vec![]);
        assert_eq!(c.current_col, 0, "push_block_full_width should reset to col 0");
    }

    #[test]
    fn push_block_full_width_overflows_to_new_page() {
        let cfg = PrintConfig { columns: 1, ..PrintConfig::default() };
        let mut c = PageComposer::new(PageGeometry::from_config(&cfg));
        let content_h = c.geometry.content_height_pt;
        // Fill most of the page.
        c.push_block(content_h - 5.0, vec![spacer(content_h - 5.0)]);
        // Push a full-width block that won't fit.
        c.push_block_full_width(20.0, vec![spacer(20.0)]);
        let (pages, _fw) = c.finalize();
        assert_eq!(pages.len(), 2, "full_width overflow should produce a new page");
    }

    #[test]
    fn column_geom_for_full_width_has_content_width() {
        let cfg = PrintConfig { columns: 2, ..PrintConfig::default() };
        let c   = PageComposer::new(PageGeometry::from_config(&cfg));
        let geom_fw = c.column_geom_for(true);
        let geom_nw = c.column_geom_for(false);
        assert!((geom_fw.column_width_pt - c.geometry.content_width_pt).abs() < 0.001);
        assert!((geom_nw.column_width_pt - c.geometry.column_width_pt).abs()  < 0.001);
    }

    #[test]
    fn two_column_x_offset_applied_to_col1_fragments() {
        let mut c = composer_2col();
        let col_w = c.geometry.column_width_pt;
        let gap   = c.geometry.column_gap_pt;
        let content_h = c.geometry.content_height_pt;
        // Force advance to col 1 (60% threshold).
        c.push_block(content_h * 0.7 + 1.0, vec![]);
        assert_eq!(c.current_col, 1);
        let frag = Fragment { x: 0.0, y: 0.0, width: 10.0, height: 5.0, kind: crate::layout::fragment::FragmentKind::Spacer };
        c.push_block(5.0, vec![frag]);
        let (pages, _fw) = c.finalize();
        let col1_frag = pages[0].last().unwrap();
        let expected_x = col_w + gap;
        assert!((col1_frag.x - expected_x).abs() < 0.001, "col 1 fragment x should be offset by col_width + gap");
    }
}
