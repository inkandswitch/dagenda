//! Wasm bindings for dagenda roadmaps.
//!
//! This crate only moves data across the JS boundary; all logic lives in
//! [`dagenda_core`]. The functions take one document's source text; plans
//! made of several documents come with the collaborative editor.

use dagenda_core::{
    Error,
    id::Id,
    parser,
    plan::{Plan, ResolveErrorKind},
    span::Span,
};
use wasm_bindgen::prelude::{JsError, wasm_bindgen};

/// A problem with the source: where it is, how bad, and what it is.
#[wasm_bindgen(getter_with_clone)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// 1-based line.
    pub line: u32,

    /// 1-based column, in characters.
    pub column: u32,

    /// `"error"` (the plan doesn't resolve) or `"warning"`.
    pub severity: String,

    /// What's wrong.
    pub message: String,
}

impl Problem {
    fn new(source: &str, span: Span, severity: &str, message: String) -> Self {
        let (line, column) = span.line_col(source);
        Self {
            line: u32::try_from(line).unwrap_or(u32::MAX),
            column: u32::try_from(column).unwrap_or(u32::MAX),
            severity: severity.into(),
            message,
        }
    }
}

/// Every problem with `source`: its error if it doesn't parse or resolve,
/// else its warnings. Empty if it's fine.
#[wasm_bindgen]
#[must_use]
pub fn check(source: &str) -> Vec<Problem> {
    match dagenda_core::parse(source) {
        Ok(plan) => plan
            .warnings()
            .iter()
            .map(|warning| Problem::new(source, warning.at.span, "warning", warning.to_string()))
            .collect(),
        Err(error) => vec![Problem::new(
            source,
            error.span(),
            "error",
            message(source, &error),
        )],
    }
}

/// Draw `source`'s tech tree as a standalone SVG document.
///
/// # Errors
///
/// Throws with the problem and its `line:column` if `source` doesn't parse
/// or resolve.
#[wasm_bindgen(js_name = renderSvg)]
pub fn render_svg(source: &str) -> Result<String, JsError> {
    Ok(plan(source)?.to_svg())
}

/// Write `source` as text: milestones, then tracks and their tasks.
///
/// # Errors
///
/// Throws with the problem and its `line:column` if `source` doesn't parse
/// or resolve.
#[wasm_bindgen(js_name = renderText)]
pub fn render_text(source: &str) -> Result<String, JsError> {
    Ok(plan(source)?.to_text())
}

/// The line where `id`'s track, task, or milestone is defined, if it is.
/// Works on text that parses, even if it doesn't resolve, so a page can
/// jump to a definition while the plan has an error elsewhere.
#[wasm_bindgen]
#[must_use]
pub fn locate(source: &str, id: &str) -> Option<u32> {
    let id = Id::new(id).ok()?;
    let document = parser::document(source).ok()?;
    let (_, span) = document
        .definitions()
        .into_iter()
        .find(|(found, _)| **found == id)?;
    u32::try_from(span.line_col(source).0).ok()
}

fn plan(source: &str) -> Result<Plan, JsError> {
    dagenda_core::parse(source).map_err(|error| {
        let (line, column) = error.span().line_col(source);
        JsError::new(&format!("{line}:{column}: {}", message(source, &error)))
    })
}

/// An error's message, saying where the first definition is for a
/// duplicate or a conflict.
fn message(source: &str, error: &Error) -> String {
    let first = match error {
        Error::Resolve(resolve) => match resolve.kind() {
            ResolveErrorKind::Duplicate { first, .. }
            | ResolveErrorKind::Conflict { first, .. } => Some(first.span),
            ResolveErrorKind::Unknown(_)
            | ResolveErrorKind::WrongKind { .. }
            | ResolveErrorKind::Cycle(_)
            | ResolveErrorKind::SubtaskCycle(_) => None,
        },
        Error::Parse(_) => None,
    };
    match first {
        Some(span) => {
            let (line, column) = span.line_col(source);
            format!("{error} (first at {line}:{column})")
        }
        None => error.to_string(),
    }
}
