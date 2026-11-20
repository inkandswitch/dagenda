//! Wasm bindings for dagenda roadmaps.
//!
//! This crate only moves data across the JS boundary; all logic lives in
//! [`dagenda_core`]. A plan is one or more documents, so they can refer to
//! each other's ids: the `…Plan` functions take the documents' names and
//! texts as two arrays, in the same order. The others are shorthands for a
//! plan of one unnamed document.

use dagenda_core::{
    check::{self, Source},
    id::Id,
    parser,
    plan::Plan,
};
use wasm_bindgen::prelude::{JsError, wasm_bindgen};

/// A problem with the source: where it is, how bad, and what it is.
#[wasm_bindgen(getter_with_clone)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// Which document it's in, from 0.
    pub document: u32,

    /// 1-based line.
    pub line: u32,

    /// 1-based column, in characters.
    pub column: u32,

    /// `"error"` (the plan doesn't resolve) or `"warning"`.
    pub severity: String,

    /// What's wrong.
    pub message: String,
}

impl From<check::Problem> for Problem {
    fn from(problem: check::Problem) -> Self {
        Self {
            document: number(problem.document),
            line: number(problem.line),
            column: number(problem.column),
            severity: problem.severity.to_string(),
            message: problem.message,
        }
    }
}

/// Where something is defined: which document, and which line.
#[wasm_bindgen]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    /// Which document, from 0.
    pub document: u32,

    /// 1-based line.
    pub line: u32,
}

/// Every problem with `source`: its error if it doesn't parse or resolve,
/// else its warnings. Empty if it's fine.
#[wasm_bindgen]
#[must_use]
pub fn check(source: &str) -> Vec<Problem> {
    check_plan(Vec::new(), vec![source.into()])
}

/// Every problem with a plan's documents: each document's parse error, or
/// the plan's first resolve error, or else its warnings.
#[wasm_bindgen(js_name = checkPlan)]
#[must_use]
pub fn check_plan(names: Vec<String>, texts: Vec<String>) -> Vec<Problem> {
    check::check(&sources(names, texts))
        .1
        .into_iter()
        .map(Problem::from)
        .collect()
}

/// Draw `source`'s tech tree as a standalone SVG document.
///
/// # Errors
///
/// Throws with the problem and its `line:column` if `source` doesn't parse
/// or resolve.
#[wasm_bindgen(js_name = renderSvg)]
pub fn render_svg(source: &str) -> Result<String, JsError> {
    render_plan_svg(Vec::new(), vec![source.into()])
}

/// Draw a plan's tech tree, from all its documents, as a standalone SVG
/// document.
///
/// # Errors
///
/// Throws with the first problem and its `name:line:column` if a document
/// doesn't parse or the plan doesn't resolve.
#[wasm_bindgen(js_name = renderPlanSvg)]
pub fn render_plan_svg(names: Vec<String>, texts: Vec<String>) -> Result<String, JsError> {
    Ok(plan(names, texts)?.to_svg())
}

/// Write `source` as text: milestones, then tracks and their tasks.
///
/// # Errors
///
/// Throws with the problem and its `line:column` if `source` doesn't parse
/// or resolve.
#[wasm_bindgen(js_name = renderText)]
pub fn render_text(source: &str) -> Result<String, JsError> {
    render_plan_text(Vec::new(), vec![source.into()])
}

/// Write a plan, from all its documents, as text.
///
/// # Errors
///
/// Throws with the first problem and its `name:line:column` if a document
/// doesn't parse or the plan doesn't resolve.
#[wasm_bindgen(js_name = renderPlanText)]
pub fn render_plan_text(names: Vec<String>, texts: Vec<String>) -> Result<String, JsError> {
    Ok(plan(names, texts)?.to_text())
}

/// The line where `id`'s track, task, or milestone is defined, if it is.
/// Works on text that parses, even if it doesn't resolve, so a page can
/// jump to a definition while the plan has an error elsewhere.
#[wasm_bindgen]
#[must_use]
pub fn locate(source: &str, id: &str) -> Option<u32> {
    locate_plan(vec![source.into()], id).map(|place| place.line)
}

/// Which document, and which line, defines `id`, if any does. Documents
/// that don't parse are skipped.
#[wasm_bindgen(js_name = locatePlan)]
#[must_use]
pub fn locate_plan(texts: Vec<String>, id: &str) -> Option<Place> {
    let id = Id::new(id).ok()?;
    texts.into_iter().enumerate().find_map(|(document, text)| {
        let parsed = parser::document(&text).ok()?;
        let (_, span) = parsed
            .definitions()
            .into_iter()
            .find(|(found, _)| **found == id)?;
        Some(Place {
            document: number(document),
            line: number(span.line_col(&text).0),
        })
    })
}

/// Each text with its name, or no name if there are fewer names.
fn sources(names: Vec<String>, texts: Vec<String>) -> Vec<Source> {
    names
        .into_iter()
        .chain(core::iter::repeat_with(String::new))
        .zip(texts)
        .map(|(name, text)| Source { name, text })
        .collect()
}

/// The plan, or its first problem as an error: `name:line:column: message`,
/// or `line:column: message` for an unnamed document.
fn plan(names: Vec<String>, texts: Vec<String>) -> Result<Plan, JsError> {
    let (plan, problems) = check::check(&sources(names, texts));
    plan.ok_or_else(|| {
        let message = problems.first().map_or_else(
            || String::from("the plan doesn't resolve"),
            |problem| {
                let place = format!("{}:{}", problem.line, problem.column);
                if problem.name.is_empty() {
                    format!("{place}: {}", problem.message)
                } else {
                    format!("{}:{place}: {}", problem.name, problem.message)
                }
            },
        );
        JsError::new(&message)
    })
}

/// A count or position, as JS gets it.
fn number(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
