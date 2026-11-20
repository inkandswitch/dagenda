//! Wasm bindings for dagenda roadmaps.
//!
//! This crate only moves data across the JS boundary; all logic lives in
//! [`dagenda_core`]. Everything crosses as plain objects, which log, spread,
//! and `JSON.stringify` like any others, and need no freeing.
//!
//! A plan is one or more documents, `[{ name, source }]` (the shape of
//! `<dagenda-view>`'s `documents`), so they can refer to each other's ids.
//! The functions without `Plan` in their names are shorthands for a plan of
//! one unnamed document.
//!
//! Edits made on the picture (`setStatus`, `addNeed`, `rescope`,
//! `reorder`) return each changed document's whole new source; the page
//! applies the difference as a splice.
//!
//! Anything refused throws an `Error` whose `kind` says why, with the ids
//! (or the problems) involved alongside, so a page can tell "already there"
//! from "would make a loop" without reading the message. The generated
//! TypeScript has them all, as `Refusal`.

use dagenda_core::{
    check::{self, Severity, Source},
    edit::{self, EditError},
    id::Id,
    parser,
    plan::Plan,
    status::Status,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, JsValue, prelude::wasm_bindgen};

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &str = r#"
/** A document of a plan: its text, and what to call it in problems. */
export interface Document {
    name?: string;
    source: string;
}

/**
 * A problem with a plan: which document it's in (from 0), where (1-based;
 * the column counts Unicode scalar values, not UTF-16 code units), how bad,
 * and what's wrong.
 */
export interface Problem {
    document: number;
    line: number;
    column: number;
    severity: "error" | "warning";
    message: string;
}

/** A document's whole new source, after an edit made on the picture. */
export interface Edit {
    document: number;
    source: string;
}

/** Where something is defined: which document (from 0), and which line. */
export interface Place {
    document: number;
    line: number;
}

/** Why something was refused: a thrown `Error` carries its `kind`, and these fields. */
export type Refusal =
    | { kind: "documents" }
    | { kind: "id"; id: string }
    | { kind: "status"; mark: string }
    | { kind: "problems"; problems: Problem[] }
    | { kind: "unparsed"; name: string }
    | { kind: "unresolved" }
    | { kind: "unknown"; id: string }
    | { kind: "notATask"; id: string }
    | { kind: "notAMilestone"; id: string }
    | { kind: "itself"; id: string }
    | { kind: "alreadyNeeds"; dependent: string; prerequisite: string }
    | { kind: "loop"; dependent: string; prerequisite: string }
    | { kind: "neededThrough"; task: string; milestone: string }
    | { kind: "differentDocuments"; milestone: string; other: string }
    | { kind: "buildsOn"; upper: string; lower: string };
"#;

/// Every problem with `source`: its errors if it doesn't parse or resolve,
/// else its warnings. Empty if it's fine.
///
/// # Errors
///
/// Only if the problems can't be handed to JS.
#[wasm_bindgen(unchecked_return_type = "Problem[]")]
pub fn check(source: &str) -> Result<JsValue, JsValue> {
    plain(&problems(&one(source)))
}

/// Every problem with a plan's documents: every parse error in each, or
/// every resolve error, or else the warnings.
///
/// # Errors
///
/// Throws `kind: "documents"` if `documents` isn't `[{ name?, source }]`.
#[wasm_bindgen(js_name = checkPlan, unchecked_return_type = "Problem[]")]
pub fn check_plan(
    #[wasm_bindgen(unchecked_param_type = "Document[]")] documents: &JsValue,
) -> Result<JsValue, JsValue> {
    plain(&problems(&sources(documents)?))
}

/// Draw `source`'s tech tree as a standalone SVG document.
///
/// # Errors
///
/// Throws `kind: "problems"`, with every problem, if `source` doesn't parse
/// or resolve.
#[wasm_bindgen(js_name = renderSvg)]
pub fn render_svg(source: &str) -> Result<String, JsValue> {
    Ok(plan(&one(source))?.to_svg())
}

/// Draw a plan's tech tree, from all its documents, as a standalone SVG
/// document.
///
/// # Errors
///
/// Throws `kind: "problems"`, with every problem, if a document doesn't
/// parse or the plan doesn't resolve; `kind: "documents"` for anything but
/// `[{ name?, source }]`.
#[wasm_bindgen(js_name = renderPlanSvg)]
pub fn render_plan_svg(
    #[wasm_bindgen(unchecked_param_type = "Document[]")] documents: &JsValue,
) -> Result<String, JsValue> {
    Ok(plan(&sources(documents)?)?.to_svg())
}

/// Write `source` as text: milestones, then tracks and their tasks.
///
/// # Errors
///
/// Throws `kind: "problems"`, with every problem, if `source` doesn't parse
/// or resolve.
#[wasm_bindgen(js_name = renderText)]
pub fn render_text(source: &str) -> Result<String, JsValue> {
    Ok(plan(&one(source))?.to_text())
}

/// Write a plan, from all its documents, as text.
///
/// # Errors
///
/// Throws `kind: "problems"`, with every problem, if a document doesn't
/// parse or the plan doesn't resolve; `kind: "documents"` for anything but
/// `[{ name?, source }]`.
#[wasm_bindgen(js_name = renderPlanText)]
pub fn render_plan_text(
    #[wasm_bindgen(unchecked_param_type = "Document[]")] documents: &JsValue,
) -> Result<String, JsValue> {
    Ok(plan(&sources(documents)?)?.to_text())
}

/// The line where `id`'s track, task, or milestone is defined, if it is.
/// Works on text that parses, even if it doesn't resolve, so a page can
/// jump to a definition while the plan has an error elsewhere.
#[wasm_bindgen]
#[must_use]
pub fn locate(source: &str, id: &str) -> Option<u32> {
    place(&one(source), id).map(|place| place.line)
}

/// Which document, and which line, defines `id`, if any does. Documents
/// that don't parse are skipped.
///
/// # Errors
///
/// Throws `kind: "documents"` if `documents` isn't `[{ name?, source }]`.
#[wasm_bindgen(js_name = locatePlan, unchecked_return_type = "Place | undefined")]
pub fn locate_plan(
    #[wasm_bindgen(unchecked_param_type = "Document[]")] documents: &JsValue,
    id: &str,
) -> Result<JsValue, JsValue> {
    place(&sources(documents)?, id).map_or(Ok(JsValue::UNDEFINED), |place| plain(&place))
}

/// Set a task's status box; `mark` is what goes inside it (`" "`, `"x"`,
/// `"-"`, …). No edits if it already has that status.
///
/// # Errors
///
/// Throws a `Refusal`: `"status"` for a mark that isn't a status box, or
/// whatever the edit itself refuses (`"unknown"`, `"notATask"`, …).
#[wasm_bindgen(js_name = setStatus, unchecked_return_type = "Edit[]")]
pub fn set_status(
    #[wasm_bindgen(unchecked_param_type = "Document[]")] documents: &JsValue,
    task: &str,
    mark: &str,
) -> Result<JsValue, JsValue> {
    let mut marks = mark.chars();
    let status = match (marks.next(), marks.next()) {
        (Some(only), None) => Status::from_mark(only),
        _ => None,
    }
    .ok_or_else(|| Refused {
        message: format!("`[{mark}]` isn't a status box"),
        refusal: Refusal::Status { mark: mark.into() },
    })?;
    made(edit::set_status(&sources(documents)?, &id(task)?, status))
}

/// Make `dependent` need `prerequisite`.
///
/// # Errors
///
/// Throws a `Refusal`: `"alreadyNeeds"`, `"loop"`, `"itself"`,
/// `"unresolved"`, and so on.
#[wasm_bindgen(js_name = addNeed, unchecked_return_type = "Edit[]")]
pub fn add_need(
    #[wasm_bindgen(unchecked_param_type = "Document[]")] documents: &JsValue,
    dependent: &str,
    prerequisite: &str,
) -> Result<JsValue, JsValue> {
    made(edit::add_need(
        &sources(documents)?,
        &id(dependent)?,
        &id(prerequisite)?,
    ))
}

/// Re-scope a task from milestone `from` to milestone `to`; either can be
/// missing (`undefined` or `null`), for the work on no milestone.
///
/// # Errors
///
/// Throws a `Refusal`: `"loop"`, `"neededThrough"` (`from` needs the task
/// through other work), `"notAMilestone"`, and so on.
#[wasm_bindgen(unchecked_return_type = "Edit[]")]
pub fn rescope(
    #[wasm_bindgen(unchecked_param_type = "Document[]")] documents: &JsValue,
    task: &str,
    from: Option<String>,
    to: Option<String>,
) -> Result<JsValue, JsValue> {
    let from = from.map(|from| id(&from)).transpose()?;
    let to = to.map(|to| id(&to)).transpose()?;
    made(edit::rescope(
        &sources(documents)?,
        &id(task)?,
        from.as_ref(),
        to.as_ref(),
    ))
}

/// Move milestone `milestone`'s era just above `other`'s (or just below).
///
/// # Errors
///
/// Throws a `Refusal`: `"buildsOn"` (one needs the other),
/// `"differentDocuments"`, `"notAMilestone"`, and so on.
#[wasm_bindgen(unchecked_return_type = "Edit[]")]
pub fn reorder(
    #[wasm_bindgen(unchecked_param_type = "Document[]")] documents: &JsValue,
    milestone: &str,
    other: &str,
    above: bool,
) -> Result<JsValue, JsValue> {
    made(edit::reorder(
        &sources(documents)?,
        &id(milestone)?,
        &id(other)?,
        above,
    ))
}

/// A document of a plan, as JS hands it over.
#[derive(Debug, Deserialize)]
struct Document {
    #[serde(default)]
    name: String,
    source: String,
}

/// A problem, as JS gets it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Problem {
    document: u32,
    line: u32,
    column: u32,
    severity: &'static str,
    message: String,
}

impl From<check::Problem> for Problem {
    fn from(problem: check::Problem) -> Self {
        Self {
            document: number(problem.document),
            line: number(problem.line),
            column: number(problem.column),
            severity: match problem.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            },
            message: problem.message,
        }
    }
}

/// A document's new source, as JS gets it.
#[derive(Debug, Serialize)]
struct Edit {
    document: u32,
    source: String,
}

/// Where something is defined, as JS gets it.
#[derive(Debug, Clone, Copy, Serialize)]
struct Place {
    document: u32,
    line: u32,
}

/// Something refused: thrown as an `Error` with `message`, and with the
/// refusal's `kind` and fields on it.
#[derive(Debug)]
struct Refused {
    message: String,
    refusal: Refusal,
}

impl From<Refused> for JsValue {
    fn from(refused: Refused) -> Self {
        let error = js_sys::Error::new(&refused.message);
        if let Ok(fields) = serde_wasm_bindgen::to_value(&refused.refusal) {
            js_sys::Object::assign(&error, fields.unchecked_ref());
        }
        error.into()
    }
}

impl From<EditError> for Refused {
    fn from(error: EditError) -> Self {
        let message = error.to_string();
        let refusal = match error {
            EditError::Unparsed(name) => Refusal::Unparsed { name },
            EditError::Unresolved => Refusal::Unresolved,
            EditError::Unknown(id) => Refusal::Unknown { id: id.to_string() },
            EditError::NotATask(id) => Refusal::NotATask { id: id.to_string() },
            EditError::NotAMilestone(id) => Refusal::NotAMilestone { id: id.to_string() },
            EditError::Itself(id) => Refusal::Itself { id: id.to_string() },
            EditError::AlreadyNeeds {
                dependent,
                prerequisite,
            } => Refusal::AlreadyNeeds {
                dependent: dependent.to_string(),
                prerequisite: prerequisite.to_string(),
            },
            EditError::Loop {
                dependent,
                prerequisite,
            } => Refusal::Loop {
                dependent: dependent.to_string(),
                prerequisite: prerequisite.to_string(),
            },
            EditError::NeededThrough { task, milestone } => Refusal::NeededThrough {
                task: task.to_string(),
                milestone: milestone.to_string(),
            },
            EditError::DifferentDocuments { milestone, other } => Refusal::DifferentDocuments {
                milestone: milestone.to_string(),
                other: other.to_string(),
            },
            EditError::BuildsOn { upper, lower } => Refusal::BuildsOn {
                upper: upper.to_string(),
                lower: lower.to_string(),
            },
        };
        Self { message, refusal }
    }
}

/// Why something was refused, for JS to branch on. Mirrors `Refusal` in
/// [`TYPES`].
#[derive(Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum Refusal {
    /// `documents` wasn't `[{ name?, source }]`.
    Documents,

    /// Not an id.
    Id {
        id: String,
    },

    /// Not what goes in a status box.
    Status {
        mark: String,
    },

    /// The plan doesn't parse or resolve, so it can't be drawn.
    Problems {
        problems: Vec<Problem>,
    },

    Unparsed {
        name: String,
    },
    Unresolved,
    Unknown {
        id: String,
    },
    NotATask {
        id: String,
    },
    NotAMilestone {
        id: String,
    },
    Itself {
        id: String,
    },
    AlreadyNeeds {
        dependent: String,
        prerequisite: String,
    },
    Loop {
        dependent: String,
        prerequisite: String,
    },
    NeededThrough {
        task: String,
        milestone: String,
    },
    DifferentDocuments {
        milestone: String,
        other: String,
    },
    BuildsOn {
        upper: String,
        lower: String,
    },
}

/// `value` as a plain JS value.
fn plain<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(JsValue::from)
}

/// A plan of just `source`, unnamed.
fn one(source: &str) -> Vec<Source> {
    vec![Source {
        name: String::new(),
        text: source.into(),
    }]
}

/// The documents JS handed over.
fn sources(documents: &JsValue) -> Result<Vec<Source>, Refused> {
    serde_wasm_bindgen::from_value::<Vec<Document>>(documents.clone())
        .map(|documents| {
            documents
                .into_iter()
                .map(|document| Source {
                    name: document.name,
                    text: document.source,
                })
                .collect()
        })
        .map_err(|error| Refused {
            message: format!("expected documents as `[{{ name, source }}]`: {error}"),
            refusal: Refusal::Documents,
        })
}

fn problems(sources: &[Source]) -> Vec<Problem> {
    check::check(sources)
        .1
        .into_iter()
        .map(Problem::from)
        .collect()
}

/// The plan, or every problem with it, thrown. The message is the first
/// problem at `name:line:column` (`line:column` for an unnamed document).
fn plan(sources: &[Source]) -> Result<Plan, Refused> {
    let (plan, problems) = check::check(sources);
    plan.ok_or_else(|| {
        let message = problems.first().map_or_else(
            || String::from("the plan doesn't resolve"),
            |problem| {
                let place = format!("{}:{}", problem.line, problem.column);
                let more = match problems.len() {
                    0 | 1 => String::new(),
                    2 => String::from(" (and 1 more problem)"),
                    count => format!(" (and {} more problems)", count - 1),
                };
                if problem.name.is_empty() {
                    format!("{place}: {}{more}", problem.message)
                } else {
                    format!("{}:{place}: {}{more}", problem.name, problem.message)
                }
            },
        );
        Refused {
            message,
            refusal: Refusal::Problems {
                problems: problems.into_iter().map(Problem::from).collect(),
            },
        }
    })
}

/// Where `id` is defined among `sources`, skipping any that don't parse.
fn place(sources: &[Source], id: &str) -> Option<Place> {
    let id = Id::new(id).ok()?;
    sources.iter().enumerate().find_map(|(document, source)| {
        let parsed = parser::document(&source.text).ok()?;
        let (_, span) = parsed
            .definitions()
            .into_iter()
            .find(|(found, _)| **found == id)?;
        Some(Place {
            document: number(document),
            line: number(span.line_col(&source.text).0),
        })
    })
}

fn id(text: &str) -> Result<Id, Refused> {
    Id::new(text).map_err(|error| Refused {
        message: error.to_string(),
        refusal: Refusal::Id { id: text.into() },
    })
}

fn made(edits: Result<Vec<edit::Edit>, EditError>) -> Result<JsValue, JsValue> {
    let edits: Vec<Edit> = edits
        .map_err(Refused::from)?
        .into_iter()
        .map(|edit| Edit {
            document: number(edit.document),
            source: edit.text,
        })
        .collect();
    plain(&edits)
}

/// A count or position, as JS gets it.
fn number(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
