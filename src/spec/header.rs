use serde::{Deserialize, Serialize};
use super::inline::InlineContent;

/// Typed institutional header, rendered at the top of page 1.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct InstitutionalHeader {
    /// School / institution name.
    pub institution: Option<String>,
    /// Exam title line.
    pub title: Option<String>,
    /// Subject name.
    pub subject: Option<String>,
    /// Academic year or semester.
    pub year: Option<String>,
    /// Logo image key (registered via add_image).
    pub logo_key: Option<String>,
    /// Fill-in fields printed below the header block (Name, Class, Date, Grade, etc.).
    pub student_fields: Vec<StudentField>,
    /// Running header on pages 2+ (supports {page} and {pages} tokens).
    pub running_header: Option<RunningHeader>,
    /// Running footer on all pages.
    pub running_footer: Option<RunningHeader>,
    /// Instructions rendered below the student fields.
    pub instructions: Vec<InlineContent>,
}

/// A labelled blank line for the student to fill in by hand.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentField {
    /// E.g., "Nome", "Turma", "Data", "Nota".
    pub label: String,
    /// Width of the underline in cm. If None, fills the remaining row width.
    #[serde(default)]
    pub width_cm: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct RunningHeader {
    pub left:   Option<String>,
    pub center: Option<String>,
    pub right:  Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn student_fields_deserialize() {
        let json = r#"{
            "studentFields": [
                {"label":"Nome"},
                {"label":"Turma","widthCm":8.0},
                {"label":"Nota","widthCm":5.0}
            ]
        }"#;
        let h: InstitutionalHeader = serde_json::from_str(json).unwrap();
        assert_eq!(h.student_fields.len(), 3);
        assert_eq!(h.student_fields[0].label, "Nome");
        assert_eq!(h.student_fields[1].width_cm, Some(8.0));
        assert!(h.student_fields[0].width_cm.is_none());
    }

    #[test]
    fn running_header_tokens() {
        let json = r#"{"left":"Matemática","right":"Pág. {page}/{pages}"}"#;
        let rh: RunningHeader = serde_json::from_str(json).unwrap();
        assert!(rh.right.unwrap().contains("{page}"));
    }
}
