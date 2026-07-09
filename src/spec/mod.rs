pub mod answer;
#[cfg(feature = "answer-sheet")]
pub mod answer_sheet;
pub mod config;
pub mod exam;
pub mod header;
pub mod inline;
pub mod question;
pub mod style;

pub use answer::AnswerSpace;
#[cfg(feature = "answer-sheet")]
pub use answer_sheet::AnswerSheetSpec;
pub use config::{LineSpacing, Margins, PageSize, PrintConfig};
pub use exam::{Appendix, ExamSpec, Section};
pub use inline::InlineContent;
pub use question::{BaseText, BaseTextPosition, Question, QuestionKind};
pub use style::{Border, FontStyle, FontWeight, Style};
