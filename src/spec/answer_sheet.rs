//! Public schema for the OMR answer sheet (folha de respostas / gabarito).
//!
//! Modelled on the lize "Folha de Respostas Avulsa" template — see
//! `tests/answer_sheet/ANALYSIS.md` for the measured reference geometry.
//!
//! The institutional header block reuses [`InstitutionalHeader`]:
//! `institution` fills the top row, `title` fills the "PROVA:" row,
//! `logo_key` fills the left cell.  `student_fields` are laid out as:
//! all fields but the last share the row above "PROVA:" (side by side),
//! and the last field gets the full-width bottom row.  With
//! `[UNIDADE, TURMA, ALUNO]` this reproduces the reference exactly.

use serde::{Deserialize, Serialize};

use super::header::InstitutionalHeader;

fn default_true() -> bool { true }

/// Complete input for `generate_answer_sheet`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AnswerSheetSpec {
    /// Machine-readable tracking line printed centered at the very top
    /// (e.g. `#A:1:2ea687c7-8ff8-4821-8d55-1443fe392a9c#`).
    pub tracking_code: Option<String>,
    /// Data encoded into the QR code in the header. A JSON object/array is
    /// serialized compactly; a JSON string is encoded as-is.
    pub qr_data: Option<serde_json::Value>,
    /// Institutional header (logo, institution, exam title, student fields).
    pub header: InstitutionalHeader,
    /// Bullet items of the "Orientações" panel. Empty/omitted = the panel is
    /// left blank (no title, no bullets); the reserved vertical space is kept
    /// so the layout below does not shift. The combined length of all items
    /// must not exceed [`MAX_ORIENTATIONS_CHARS`].
    pub orientations: Vec<String>,
    /// Label under the signature line.
    pub signature_label: Option<String>,
    /// Text of the grey fill-instructions strip. None = default lize wording.
    pub fill_instructions: Option<String>,
    /// Draw the Correto/Errado marking example inside the grey strip.
    #[serde(default = "default_true")]
    pub show_fill_example: bool,
    /// Answer bubble grid.
    pub answers: AnswerGrid,
    /// Centered footer under the answers box (e.g. "Lize - 2026").
    pub footer_text: Option<String>,
}

impl Default for AnswerSheetSpec {
    fn default() -> Self {
        Self {
            tracking_code:     None,
            qr_data:           None,
            header:            InstitutionalHeader::default(),
            orientations:      Vec::new(),
            signature_label:   None,
            fill_instructions: None,
            show_fill_example: true,
            answers:           AnswerGrid::default(),
            footer_text:       None,
        }
    }
}

/// Answer bubble grid ("Respostas" box).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AnswerGrid {
    /// Number of questions (rows).
    pub count: u32,
    /// Visible alternatives per question (1–5 → letters A–E). The template
    /// always reserves 5 bubble columns; hidden ones are painted in the row
    /// background color, exactly like the Chromium reference.
    pub alternatives: u8,
    /// First question number.
    pub start_number: u32,
    /// Rows per column before wrapping to the next column inside the box.
    pub rows_per_column: u32,
}

impl Default for AnswerGrid {
    fn default() -> Self {
        Self { count: 0, alternatives: 4, start_number: 1, rows_per_column: 30 }
    }
}

impl AnswerSheetSpec {
    /// The QR payload as the exact string to encode, if any.
    pub fn qr_payload(&self) -> Option<String> {
        match &self.qr_data {
            None => None,
            Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(s)) => Some(s.clone()),
            Some(v) => serde_json::to_string(v).ok(),
        }
    }
}

/// Maximum combined length (Unicode scalar values) of all "Orientações"
/// bullet items. Specs exceeding this are rejected during validation.
pub const MAX_ORIENTATIONS_CHARS: usize = 700;

/// Default signature-line label.
pub const DEFAULT_SIGNATURE_LABEL: &str = "Assinatura do aluno";

/// Default fill-instructions text (lize wording).
pub const DEFAULT_FILL_INSTRUCTIONS: &str =
    "Preencha os círculos deste cartão resposta com nitidez, completamente e \
     utilizando caneta esferográfica de cor preta, conforme exemplo ao lado:";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_spec_deserializes_with_defaults() {
        let spec: AnswerSheetSpec = serde_json::from_str(r#"{"answers":{"count":5}}"#).unwrap();
        assert_eq!(spec.answers.count, 5);
        assert_eq!(spec.answers.alternatives, 4);
        assert_eq!(spec.answers.start_number, 1);
        assert!(spec.show_fill_example);
        assert!(spec.qr_data.is_none());
        assert!(spec.orientations.is_empty());
    }

    #[test]
    fn qr_payload_object_is_compact_json() {
        let spec: AnswerSheetSpec = serde_json::from_str(
            r#"{"qrData":{"a":1,"b":"x"}}"#,
        ).unwrap();
        let p = spec.qr_payload().unwrap();
        assert!(p.contains("\"a\":1"));
        assert!(!p.contains(' '), "payload must be compact: {p}");
    }

    #[test]
    fn qr_payload_string_passthrough() {
        let spec: AnswerSheetSpec = serde_json::from_str(
            r#"{"qrData":"plain-text"}"#,
        ).unwrap();
        assert_eq!(spec.qr_payload().unwrap(), "plain-text");
    }

    #[test]
    fn qr_payload_null_is_none() {
        let spec: AnswerSheetSpec = serde_json::from_str(r#"{"qrData":null}"#).unwrap();
        assert!(spec.qr_payload().is_none());
    }

    #[test]
    fn header_reuses_institutional_header() {
        let spec: AnswerSheetSpec = serde_json::from_str(r#"{
            "header": {
                "institution": "REDE DECISÃO",
                "title": "P5_MATEMÁTICA_F7_ANGLO_2026",
                "logoKey": "client_logo",
                "studentFields": [
                    {"label": "UNIDADE"}, {"label": "TURMA"}, {"label": "ALUNO"}
                ]
            }
        }"#).unwrap();
        assert_eq!(spec.header.institution.as_deref(), Some("REDE DECISÃO"));
        assert_eq!(spec.header.student_fields.len(), 3);
    }

    #[test]
    fn roundtrip_serialize() {
        let spec = AnswerSheetSpec {
            tracking_code: Some("#A:1:x#".into()),
            answers: AnswerGrid { count: 10, alternatives: 5, ..Default::default() },
            ..Default::default()
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: AnswerSheetSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tracking_code.as_deref(), Some("#A:1:x#"));
        assert_eq!(back.answers.alternatives, 5);
    }
}
