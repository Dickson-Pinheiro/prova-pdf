//! Orchestrates the 4-phase PDF generation pipeline.
//!
//! Phase 1: Validation   — check required fields and font availability
//! Phase 2: Style cascade — resolve PrintConfig → Section → Question → Inline
//! Phase 3: Layout       — lay out all elements into positioned fragments
//! Phase 4: Emission     — write PDF bytes from fragments
//!
//! Currently a stub — implemented incrementally as layout/emission modules are added.

use crate::fonts::{FontRegistry, FontRules};
use crate::spec::ExamSpec;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("no font registered: call add_font('body', 0, bytes) first")]
    NoFont,
    #[error("PDF emission error: {0}")]
    EmissionError(String),
}

pub struct RenderContext {
    pub registry: FontRegistry,
    pub rules:    FontRules,
    pub images:   HashMap<String, Vec<u8>>,
}

/// Render an ExamSpec to PDF bytes.
pub fn render(
    _spec: &ExamSpec,
    ctx: &RenderContext,
) -> Result<Vec<u8>, PipelineError> {
    if !ctx.registry.is_ready() {
        return Err(PipelineError::NoFont);
    }
    // TODO: implement full pipeline
    Err(PipelineError::EmissionError("not yet implemented".into()))
}
