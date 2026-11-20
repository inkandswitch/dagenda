//! Wasm bindings for dagenda roadmaps.
//!
//! This crate only moves data across the JS boundary; all logic lives in
//! [`dagenda_core`]. A plan is one or more documents, so they can refer to
//! each other's ids: the `…Plan` functions take the documents' names and
//! texts as two arrays, in the same order. The others are shorthands for a
//! plan of one unnamed document.
//!
//! Edits made on the picture (`setStatus`, `addNeed`, `rescope`,
//! `reorder`) return each changed document's whole new text; the page
//! applies the difference as a splice. A refused edit throws its reason.

use dagenda_core::{
    check::{self, Source},
    edit::{self, EditError},
    id::Id,
    parser,
    plan::Plan,
    status::Status,
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

/// A document's new text, after an edit made on the picture.
#[wasm_bindgen(getter_with_clone)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// Which document, from 0.
    pub document: u32,

    /// Its whole new text.
    pub text: String,
}

impl From<edit::Edit> for Edit {
    fn from(edit: edit::Edit) -> Self {
        Self {
            document: number(edit.document),
            text: edit.text,
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

/// Set a task's status box; `mark` is what goes inside it (`" "`, `"x"`,
/// `"-"`, …). No edits if it already has that status.
///
/// # Errors
///
/// Throws if a document doesn't parse, `task` isn't a task, or `mark`
/// isn't a status.
#[wasm_bindgen(js_name = setStatus)]
pub fn set_status(
    names: Vec<String>,
    texts: Vec<String>,
    task: &str,
    mark: &str,
) -> Result<Vec<Edit>, JsError> {
    let status = mark
        .chars()
        .next()
        .filter(|_| mark.chars().count() == 1)
        .and_then(Status::from_mark)
        .ok_or_else(|| JsError::new(&format!("`[{mark}]` isn't a status box")))?;
    made(edit::set_status(&sources(names, texts), &id(task)?, status))
}

/// Make `dependent` need `prerequisite`.
///
/// # Errors
///
/// Throws if the plan doesn't parse and resolve, the need is already there,
/// or it would make a loop.
#[wasm_bindgen(js_name = addNeed)]
pub fn add_need(
    names: Vec<String>,
    texts: Vec<String>,
    dependent: &str,
    prerequisite: &str,
) -> Result<Vec<Edit>, JsError> {
    made(edit::add_need(
        &sources(names, texts),
        &id(dependent)?,
        &id(prerequisite)?,
    ))
}

/// Re-scope a task from milestone `from` to milestone `to`; either can be
/// missing, for the work on no milestone.
///
/// # Errors
///
/// Throws if a document doesn't parse, it would make a loop, or `from`
/// needs the task through other work.
#[wasm_bindgen]
pub fn rescope(
    names: Vec<String>,
    texts: Vec<String>,
    task: &str,
    from: Option<String>,
    to: Option<String>,
) -> Result<Vec<Edit>, JsError> {
    let from = from.map(|from| id(&from)).transpose()?;
    let to = to.map(|to| id(&to)).transpose()?;
    made(edit::rescope(
        &sources(names, texts),
        &id(task)?,
        from.as_ref(),
        to.as_ref(),
    ))
}

/// Move milestone `milestone`'s era just above `other`'s (or just below).
///
/// # Errors
///
/// Throws if a document doesn't parse, the two are in different documents,
/// or one builds on the other.
#[wasm_bindgen]
pub fn reorder(
    names: Vec<String>,
    texts: Vec<String>,
    milestone: &str,
    other: &str,
    above: bool,
) -> Result<Vec<Edit>, JsError> {
    made(edit::reorder(
        &sources(names, texts),
        &id(milestone)?,
        &id(other)?,
        above,
    ))
}

fn id(text: &str) -> Result<Id, JsError> {
    Id::new(text).map_err(|error| JsError::new(&error.to_string()))
}

fn made(edits: Result<Vec<edit::Edit>, EditError>) -> Result<Vec<Edit>, JsError> {
    edits
        .map(|edits| edits.into_iter().map(Edit::from).collect())
        .map_err(|error| JsError::new(&error.to_string()))
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
