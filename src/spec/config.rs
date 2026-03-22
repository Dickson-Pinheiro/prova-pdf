use serde::{Deserialize, Serialize};

/// Root print + layout configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintConfig {
    // --- Page geometry ---
    #[serde(default)]
    pub page_size: PageSize,
    #[serde(default)]
    pub margins: Margins,
    /// 1 or 2 columns.
    #[serde(default = "default_columns")]
    pub columns: u8,

    // --- Typography ---
    #[serde(default = "default_font_size")]
    pub font_size: f64,
    #[serde(default)]
    pub line_spacing: LineSpacing,
    /// Default font family name (must match a registered family in FontRegistry).
    #[serde(default = "default_font_family")]
    pub font_family: String,

    // --- Answer spaces ---
    /// Height of each discursive answer line, in cm. Default 0.8cm.
    #[serde(default = "default_line_height_cm")]
    pub discursive_line_height: f64,
    #[serde(default)]
    pub discursive_space_type: DiscursiveSpaceType,

    // --- Economy/display flags ---
    /// Removes answer spaces; forces two_columns. For space-saving prints.
    #[serde(default)]
    pub economy_mode: bool,
    /// Insert page break before every question.
    #[serde(default)]
    pub break_all_questions: bool,
    /// Convert images to grayscale.
    #[serde(default)]
    pub image_grayscale: bool,
    /// Force all colors to black (overrides image_grayscale).
    #[serde(default)]
    pub all_black: bool,

    // --- Rendering flags ---
    #[serde(default)]
    pub show_score: bool,
    #[serde(default)]
    pub hide_numbering: bool,
    /// Show full institutional header with student fill-in fields.
    #[serde(default = "default_true")]
    pub header_full: bool,

    // --- Multiple-choice layout ---
    /// Vertical gap between consecutive alternatives, in cm. Default 0.3cm.
    #[serde(default = "default_alternative_spacing")]
    pub alternative_spacing_cm: f64,
    /// Letter case for auto-generated choice labels.
    #[serde(default)]
    pub letter_case: LetterCase,
    /// Remove colored badges on alternatives; show plain text instead.
    #[serde(default)]
    pub remove_color_alternatives: bool,

    // --- Break behaviour flags (from lize printConfig) ---
    /// Allow page breaks inside the enunciation (stem) of a question.
    #[serde(default)]
    pub break_enunciation: bool,
    /// Allow page breaks inside the alternatives block.
    #[serde(default)]
    pub break_alternatives: bool,
    /// Force alternatives to appear on the same page as the stem.
    /// 0 = no constraint (default), other values = implementation-specific.
    #[serde(default)]
    pub force_choices_with_statement: u8,
    /// Textual question format: 0 = no answer lines, 1 = show answer lines (default).
    #[serde(default = "default_text_question_format")]
    pub text_question_format: u8,

    // --- Visibility flags ---
    /// Hide the discipline/subject name in section headers.
    #[serde(default)]
    pub hide_discipline_name: bool,
    /// Hide the knowledge area name in section headers.
    #[serde(default)]
    pub hide_knowledge_area_name: bool,
    /// Hide question references (board, source, etc.).
    #[serde(default)]
    pub hide_questions_references: bool,
    /// Show the question board/source name.
    #[serde(default)]
    pub show_question_board: bool,
}

fn default_columns() -> u8 { 1 }
fn default_font_size() -> f64 { 12.0 }
fn default_font_family() -> String { "body".to_string() }
fn default_line_height_cm() -> f64 { 0.85 }
fn default_true() -> bool { true }
fn default_alternative_spacing() -> f64 { 0.3 }
fn default_text_question_format() -> u8 { 1 }

impl Default for PrintConfig {
    fn default() -> Self {
        Self {
            page_size: PageSize::default(),
            margins: Margins::default(),
            columns: 1,
            font_size: 12.0,
            line_spacing: LineSpacing::default(),
            font_family: "body".to_string(),
            discursive_line_height: 0.85,
            discursive_space_type: DiscursiveSpaceType::default(),
            economy_mode: false,
            break_all_questions: false,
            image_grayscale: false,
            all_black: false,
            show_score: false,
            hide_numbering: false,
            header_full: true,
            alternative_spacing_cm: 0.3,
            letter_case: LetterCase::default(),
            remove_color_alternatives: false,
            break_enunciation: false,
            break_alternatives: false,
            force_choices_with_statement: 0,
            text_question_format: 1,
            hide_discipline_name: false,
            hide_knowledge_area_name: false,
            hide_questions_references: false,
            show_question_board: false,
        }
    }
}

/// Letter case used for multiple-choice alternative labels.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum LetterCase {
    #[default]
    Upper,
    Lower,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Margins {
    /// Top margin in cm. Default 0.6 (matches lize @page margin-top).
    #[serde(default = "default_margin_vertical")]
    pub top: f64,
    /// Bottom margin in cm. Default 0.6 (matches lize @page margin-bottom).
    #[serde(default = "default_margin_vertical")]
    pub bottom: f64,
    /// Left margin in cm. Default 1.5 (practical minimum for PDF output).
    #[serde(default = "default_margin_horizontal")]
    pub left: f64,
    /// Right margin in cm. Default 1.5 (practical minimum for PDF output).
    #[serde(default = "default_margin_horizontal")]
    pub right: f64,
}

fn default_margin_vertical() -> f64 { 0.6 }
fn default_margin_horizontal() -> f64 { 1.5 }

impl Default for Margins {
    fn default() -> Self {
        Self { top: 0.6, bottom: 0.6, left: 1.5, right: 1.5 }
    }
}

impl Margins {
    /// 1 cm = 28.3465 pt
    pub fn top_pt(&self)    -> f64 { self.top    * 28.3465 }
    pub fn bottom_pt(&self) -> f64 { self.bottom * 28.3465 }
    pub fn left_pt(&self)   -> f64 { self.left   * 28.3465 }
    pub fn right_pt(&self)  -> f64 { self.right  * 28.3465 }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum PageSize {
    #[default]
    A4,
    /// SEDUC standard: 200 × 266 mm
    Ata,
    Custom { width_mm: f64, height_mm: f64 },
}

impl PageSize {
    pub fn width_pt(&self) -> f64 {
        match self {
            PageSize::A4 => 595.276,
            PageSize::Ata => 566.929,
            PageSize::Custom { width_mm, .. } => width_mm * 2.83465,
        }
    }
    pub fn height_pt(&self) -> f64 {
        match self {
            PageSize::A4 => 841.890,
            PageSize::Ata => 754.016,
            PageSize::Custom { height_mm, .. } => height_mm * 2.83465,
        }
    }
}

/// Line spacing multiplier for body text.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum LineSpacing {
    #[default]
    Normal,
    OneAndHalf,
    TwoAndHalf,
    ThreeAndHalf,
}

impl LineSpacing {
    pub fn multiplier(self) -> f64 {
        match self {
            LineSpacing::Normal       => 1.2,
            LineSpacing::OneAndHalf   => 1.5,
            LineSpacing::TwoAndHalf   => 2.5,
            LineSpacing::ThreeAndHalf => 3.5,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum DiscursiveSpaceType {
    #[default]
    Lines,
    Blank,
    /// Space with no visible lines or border.
    NoBorder,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SeparateMode {
    #[default]
    None,
    BySubject,
    ByCategory,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_print_config_serializes() {
        let c = PrintConfig::default();
        let j = serde_json::to_string(&c).unwrap();
        assert!(j.contains("\"columns\":1"));
    }

    #[test]
    fn page_size_ata_dimensions() {
        let s = PageSize::Ata;
        assert!((s.width_pt()  - 566.929).abs() < 0.01);
        assert!((s.height_pt() - 754.016).abs() < 0.01);
    }

    #[test]
    fn margins_cm_to_pt() {
        let m = Margins { top: 1.0, bottom: 1.0, left: 1.0, right: 1.0 };
        assert!((m.top_pt() - 28.3465).abs() < 0.001);
    }

    #[test]
    fn line_spacing_multipliers() {
        assert_eq!(LineSpacing::Normal.multiplier(), 1.2);
        assert_eq!(LineSpacing::OneAndHalf.multiplier(), 1.5);
        assert_eq!(LineSpacing::TwoAndHalf.multiplier(), 2.5);
        assert_eq!(LineSpacing::ThreeAndHalf.multiplier(), 3.5);
    }

    #[test]
    fn economy_mode_default_false() {
        let c = PrintConfig::default();
        assert!(!c.economy_mode);
    }
}
