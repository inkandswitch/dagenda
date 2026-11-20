//! Edits made on the picture (or by a tool), as new text for the documents
//! they touch.
//!
//! The text is the truth: a click or a drag on the tech tree asks for an
//! edit here, and gets back each changed document's whole new text. The
//! page applies the difference as the smallest splice, so a picture edit
//! joins that person's undo and merges with what others type. Edits keep the
//! writer's layout: they swap a status box, add a line in its block's own
//! indent, or move a block whole. An edit that would make no sense (a loop, a
//! milestone below one it builds on) is refused with the reason.
//!
//! ```
//! use dagenda_core::{check::Source, edit, id::Id, status::Status};
//!
//! let plan = Source {
//!     name: "plan".into(),
//!     text: "roadmap\n[ ] a\n[ ] b\n".into(),
//! };
//! let edits = edit::set_status(&[plan], &Id::new("a")?, Status::Done)?;
//! assert_eq!(edits[0].text, "roadmap\n[x] a\n[ ] b\n");
//! # Ok::<(), Box<dyn core::error::Error>>(())
//! ```

use crate::{
    ast::{self, Document, Item, TrackChild},
    check::Source,
    date::Date,
    handle::Handles,
    id::Id,
    parser,
    plan::Plan,
    span::{Span, Spanned},
    status::Status,
};
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;
use thiserror::Error;

/// A document's new text, after an edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// Which of the sources, from 0.
    pub document: usize,

    /// Its whole new text.
    pub text: String,
}

/// Set a task's status box, in the document that defines the task. No
/// edits if it already has that status.
///
/// # Errors
///
/// Returns [`EditError`] if a document doesn't parse, or `task` isn't a
/// task.
pub fn set_status(sources: &[Source], task: &Id, status: Status) -> Result<Vec<Edit>, EditError> {
    let documents = parse(sources)?;
    let (document, found) = find_task(&documents, task)?;
    if found.node.status.node == status {
        return Ok(Vec::new());
    }
    let span = found.node.status.span;
    Ok(edits(
        sources,
        [(
            document,
            (span.start, span.end, format!("[{}]", status.mark())),
        )],
    ))
}

/// Make `dependent` need `prerequisite`, in the document that defines
/// `dependent`: appended to its last `needs` line, or on a new `needs` line
/// at the top of its block.
///
/// # Errors
///
/// Returns [`EditError`] if the plan doesn't parse and resolve, `dependent`
/// isn't a task, `prerequisite` doesn't exist, the need is already there,
/// or it would make a loop.
pub fn add_need(
    sources: &[Source],
    dependent: &Id,
    prerequisite: &Id,
) -> Result<Vec<Edit>, EditError> {
    if dependent == prerequisite {
        return Err(EditError::Itself(dependent.clone()));
    }
    let documents = parse(sources)?;
    let plan = Plan::resolve(&documents).map_err(|_| EditError::Unresolved)?;
    let (document, found) = find_task(&documents, dependent)?;
    if plan.task(prerequisite).is_none() && plan.milestone(prerequisite).is_none() {
        return Err(EditError::Unknown(prerequisite.clone()));
    }
    if found
        .node
        .details
        .needs
        .iter()
        .any(|need| need.node == *prerequisite)
        || plan
            .task(dependent)
            .is_some_and(|task| task.needs.iter().any(|need| need.node == *prerequisite))
    {
        return Err(EditError::AlreadyNeeds {
            dependent: dependent.clone(),
            prerequisite: prerequisite.clone(),
        });
    }
    if waits_on(&plan, prerequisite, dependent) {
        return Err(EditError::Loop {
            dependent: dependent.clone(),
            prerequisite: prerequisite.clone(),
        });
    }

    let text = text_of(sources, document);
    let splice = if let Some(last) = found.node.details.needs.last() {
        (last.span.end, last.span.end, format!(", {prerequisite}"))
    } else {
        let at = next_line(text, found.span.start);
        let line = format!("needs {prerequisite}");
        (
            at,
            at,
            new_line(text, at, child_indent(text, found.span.start), &line),
        )
    };
    Ok(edits(sources, [(document, splice)]))
}

/// Re-scope a task from one milestone to another: take it off `from`'s
/// list (if it's on it), and put it on `to`'s must-haves (a nice-to-have
/// there becomes one). `None` for `to` takes it off every era: the
/// work on no milestone. With no `from`, a task `to` already needs through
/// other work is left as it is.
///
/// # Errors
///
/// Returns [`EditError`] if a document doesn't parse, an id isn't the kind
/// it should be, the task already waits on `to` (a loop), or `from` needs
/// the task only through other work, so it can't leave `from`'s era.
pub fn rescope(
    sources: &[Source],
    task: &Id,
    from: Option<&Id>,
    to: Option<&Id>,
) -> Result<Vec<Edit>, EditError> {
    if from == to {
        return Ok(Vec::new());
    }
    let documents = parse(sources)?;
    find_task(&documents, task)?;
    let plan = Plan::resolve(&documents).ok();
    if let (Some(to), Some(plan)) = (to, &plan)
        && waits_on(plan, task, to)
    {
        return Err(EditError::Loop {
            dependent: to.clone(),
            prerequisite: task.clone(),
        });
    }
    let mut splices = Vec::new();

    if let Some(from) = from {
        let (document, milestone) = find_milestone(&documents, from)?;
        let text = text_of(sources, document);
        let listed: Vec<_> = members(&milestone.node)
            .filter(|member| member.node == *task)
            .collect();
        if listed.is_empty() && to.is_none() {
            // Not on its list: nothing to take off, unless it still needs
            // the task through other work, which keeps it in that era
            return if plan
                .as_ref()
                .is_some_and(|plan| !waits_on(plan, from, task))
            {
                Ok(Vec::new())
            } else {
                Err(EditError::NeededThrough {
                    task: task.clone(),
                    milestone: from.clone(),
                })
            };
        }
        splices.extend(off_list(text, &milestone.node, task).map(|splice| (document, splice)));
    }

    if let Some(to) = to {
        let (document, found) = find_milestone(&documents, to)?;
        let text = text_of(sources, document);
        let milestone = &found.node;
        // Brought in (from nowhere) to a milestone that needs it already,
        // through other work: nothing to add
        let already = from.is_none() && plan.as_ref().is_some_and(|plan| waits_on(plan, to, task));
        if !already && !milestone.needs.iter().any(|member| member.node == *task) {
            // A nice-to-have becomes a must-have
            let wanted = milestone.wants.iter().any(|member| member.node == *task);
            if wanted {
                splices.extend(off_list(text, milestone, task).map(|splice| (document, splice)));
            }
            let (at, indent) = match (milestone.needs.last(), milestone.nice_to_have) {
                (Some(last), _) => (
                    next_line(text, last.span.start),
                    indent_of(text, last.span.start),
                ),
                (None, Some(nice)) => (line_start(text, nice.start), indent_of(text, nice.start)),
                (None, None) => (
                    block_end(text, found.span.start),
                    child_indent(text, found.span.start),
                ),
            };
            splices.push((
                document,
                (at, at, new_line(text, at, indent, task.as_str())),
            ));
        }
    }
    Ok(edits(sources, splices))
}

/// Move a milestone's block just after `other`'s (`above`: its era goes
/// above `other`'s), or just before it. Both must be in one document.
///
/// # Errors
///
/// Returns [`EditError`] if a document doesn't parse, an id isn't a
/// milestone, the two are in different documents, or one builds on the
/// other, so their order is fixed.
pub fn reorder(
    sources: &[Source],
    milestone: &Id,
    other: &Id,
    above: bool,
) -> Result<Vec<Edit>, EditError> {
    if milestone == other {
        return Ok(Vec::new());
    }
    let documents = parse(sources)?;
    let (document, moving) = find_milestone(&documents, milestone)?;
    let (other_document, staying) = find_milestone(&documents, other)?;
    if document != other_document {
        return Err(EditError::DifferentDocuments {
            milestone: milestone.clone(),
            other: other.clone(),
        });
    }
    if let Ok(plan) = Plan::resolve(&documents) {
        let (upper, lower) = if above {
            (other, milestone)
        } else {
            (milestone, other)
        };
        if waits_on(&plan, upper, lower) {
            return Err(EditError::BuildsOn {
                upper: upper.clone(),
                lower: lower.clone(),
            });
        }
    }

    let text = text_of(sources, document);
    let eol = eol(text);
    let block = |start: usize| {
        let from = line_start(text, start);
        let end = block_end(text, start);
        (from, end, skip_blank_lines(text, end))
    };
    let (from, end, gap_end) = block(moving.span.start);
    // The last block takes the blank lines before it along, so the text
    // doesn't end in one
    let cut_from = if gap_end == text.len() {
        blank_lines_before(text, from)
    } else {
        from
    };
    let (other_from, _, other_gap_end) = block(staying.span.start);
    let at = if above { other_gap_end } else { other_from };
    if at == from || at == gap_end {
        return Ok(Vec::new());
    }

    // The block whole, then one blank line, so blocks stay apart
    let mut moved = String::from(text.get(from..end).unwrap_or_default());
    if !moved.ends_with('\n') {
        moved.push_str(eol);
    }
    let lead = if at == text.len() && !text.is_empty() && !text.ends_with(&format!("{eol}{eol}")) {
        if text.ends_with('\n') {
            eol.to_string()
        } else {
            format!("{eol}{eol}")
        }
    } else {
        String::new()
    };
    let tail = if at == text.len() { "" } else { eol };
    Ok(edits(
        sources,
        [
            (document, (cut_from, gap_end, String::new())),
            (document, (at, at, format!("{lead}{moved}{tail}"))),
        ],
    ))
}

/// Set a task's `dri`, `est`, or `target`, or a milestone's `target`: on
/// the line that says it now, whichever document that's in (an `extend`
/// keeps what it set), or else on a new line at the top of the block that
/// defines it. No edits if it already says that.
///
/// ```
/// use dagenda_core::{check::Source, edit::{self, Setting}, handle::{Handle, Handles}, id::Id};
///
/// let plan = Source {
///     name: "plan".into(),
///     text: "roadmap\n[ ] a\n  needs b\n[ ] b\n".into(),
/// };
/// let bob = Handles::new(vec![Handle::new("@bob")?]).ok_or("no one")?;
/// let edits = edit::set(&[plan], &Id::new("a")?, &Setting::Dri(bob))?;
/// assert_eq!(edits[0].text, "roadmap\n[ ] a\n  dri @bob\n  needs b\n[ ] b\n");
/// # Ok::<(), Box<dyn core::error::Error>>(())
/// ```
///
/// # Errors
///
/// Returns [`EditError`] if a document doesn't parse, or `id` isn't a task
/// (or, for a `target`, a milestone).
pub fn set(sources: &[Source], id: &Id, setting: &Setting) -> Result<Vec<Edit>, EditError> {
    let documents = parse(sources)?;
    let (document, head, now) = match find_task(&documents, id) {
        Ok((document, task)) => {
            let now = extends(&documents, id)
                .chain(core::iter::once((document, &task.node.details)))
                .find_map(|(index, details)| current(details, setting).map(|now| (index, now)));
            (document, task.span.start, now)
        }
        Err(EditError::NotATask(_)) if matches!(setting, Setting::Target(_)) => {
            let (document, milestone) = find_milestone(&documents, id)?;
            let now = match setting {
                Setting::Target(date) => milestone
                    .node
                    .target
                    .as_ref()
                    .map(|target| (document, (target.span, target.node == *date))),
                Setting::Dri(_) | Setting::Est(_) => None,
            };
            (document, milestone.span.start, now)
        }
        Err(e) => return Err(e),
    };

    let splice = match now {
        Some((_, (_, true))) => return Ok(Vec::new()),
        Some((document, (span, false))) => (document, (span.start, span.end, setting.to_string())),
        None => {
            let text = text_of(sources, document);
            let at = next_line(text, head);
            let line = format!("{} {setting}", setting.keyword());
            (
                document,
                (at, at, new_line(text, at, child_indent(text, head), &line)),
            )
        }
    };
    Ok(edits(sources, [splice]))
}

/// A detail a task has one of, wherever it's written: `dri`, `est`, or
/// `target` (which a milestone has too).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Setting {
    /// `dri @alice, @bob`
    Dri(Handles),

    /// `est <date>`
    Est(Date),

    /// `target <date>`
    Target(Date),
}

impl Setting {
    /// Its keyword, as in `dri`.
    #[must_use]
    pub const fn keyword(&self) -> &'static str {
        match self {
            Self::Dri(_) => "dri",
            Self::Est(_) => "est",
            Self::Target(_) => "target",
        }
    }
}

/// The value, as written after the keyword.
impl fmt::Display for Setting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dri(handles) => write!(f, "{handles}"),
            Self::Est(date) | Self::Target(date) => write!(f, "{date}"),
        }
    }
}

/// Why an edit made on the picture can't be made.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EditError {
    /// A document that doesn't parse (by name; empty if it has none).
    #[error("{} doesn't parse, so the picture can't edit it", Named(.0))]
    Unparsed(String),

    /// The plan doesn't resolve, so loops can't be checked.
    #[error("the plan doesn't resolve, so the picture can't edit it; fix the error first")]
    Unresolved,

    /// No task or milestone has this id.
    #[error("nothing in the plan is called `{0}`")]
    Unknown(Id),

    /// The id is a milestone or a track, where a task was meant.
    #[error("`{0}` isn't a task")]
    NotATask(Id),

    /// The id is a task or a track, where a milestone was meant.
    #[error("`{0}` isn't a milestone")]
    NotAMilestone(Id),

    /// A task asked to need itself.
    #[error("`{0}` can't need itself")]
    Itself(Id),

    /// The need is already written.
    #[error("`{dependent}` already needs `{prerequisite}`")]
    AlreadyNeeds {
        /// What would wait.
        dependent: Id,

        /// What it would wait on.
        prerequisite: Id,
    },

    /// The need would close a loop.
    #[error("`{prerequisite}` already waits on `{dependent}`, so `{dependent}` can't need it")]
    Loop {
        /// What would wait.
        dependent: Id,

        /// What it would wait on, which already waits on it.
        prerequisite: Id,
    },

    /// A milestone needs the task only through other work.
    #[error("`{milestone}` needs `{task}` through other work, so it stays in that era")]
    NeededThrough {
        /// The task.
        task: Id,

        /// The milestone that still needs it.
        milestone: Id,
    },

    /// Milestones in different documents have no order between them to change.
    #[error("`{milestone}` and `{other}` are in different documents, so each keeps its own order")]
    DifferentDocuments {
        /// The milestone being moved.
        milestone: Id,

        /// The one it was moved past.
        other: Id,
    },

    /// One milestone needs the other, so their order is fixed.
    #[error("`{upper}` builds on `{lower}`, so it stays above it")]
    BuildsOn {
        /// The milestone that needs the other.
        upper: Id,

        /// The one it needs.
        lower: Id,
    },
}

/// A document as an error names it: its name in backticks, or "the text".
struct Named<'a>(&'a str);

impl fmt::Display for Named<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            f.write_str("the text")
        } else {
            write!(f, "`{}`", self.0)
        }
    }
}

/// A change to one document: replace bytes `from..to` with the text.
type Splice = (usize, usize, String);

/// Every document, parsed.
fn parse(sources: &[Source]) -> Result<Vec<Document>, EditError> {
    sources
        .iter()
        .map(|source| {
            parser::document(&source.text).map_err(|_| EditError::Unparsed(source.name.clone()))
        })
        .collect()
}

fn text_of(sources: &[Source], document: usize) -> &str {
    sources
        .get(document)
        .map_or("", |source| source.text.as_str())
}

/// Apply each document's splices, giving its new text.
fn edits<I: IntoIterator<Item = (usize, Splice)>>(sources: &[Source], splices: I) -> Vec<Edit> {
    let mut by_document: BTreeMap<usize, Vec<Splice>> = BTreeMap::new();
    for (document, splice) in splices {
        by_document.entry(document).or_default().push(splice);
    }
    by_document
        .into_iter()
        .map(|(document, splices)| Edit {
            document,
            text: apply(text_of(sources, document), splices),
        })
        .collect()
}

/// `text` with `splices` (which don't overlap) made.
fn apply(text: &str, mut splices: Vec<Splice>) -> String {
    splices.sort_by_key(|&(from, to, _)| (from, to));
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (from, to, insert) in splices {
        out.push_str(text.get(cursor..from.max(cursor)).unwrap_or_default());
        out.push_str(&insert);
        cursor = cursor.max(to);
    }
    out.push_str(text.get(cursor..).unwrap_or_default());
    out
}

/// Where `setting`'s detail is in `details`, and whether it already says that.
fn current(details: &ast::Details, setting: &Setting) -> Option<(Span, bool)> {
    match setting {
        Setting::Dri(handles) => details
            .dri
            .as_ref()
            .map(|dri| (dri.span, dri.node == *handles)),
        Setting::Est(date) => details
            .est
            .as_ref()
            .map(|est| (est.span, est.node == *date)),
        Setting::Target(date) => details
            .target
            .as_ref()
            .map(|target| (target.span, target.node == *date)),
    }
}

/// Each `extend` of `id`'s details, and which document it's in.
fn extends<'a>(
    documents: &'a [Document],
    id: &'a Id,
) -> impl Iterator<Item = (usize, &'a ast::Details)> {
    documents
        .iter()
        .enumerate()
        .flat_map(move |(index, document)| {
            document.items.iter().filter_map(move |item| match item {
                Item::Extend(extend) if extend.node.id.node == *id => {
                    Some((index, &extend.node.details))
                }
                Item::Extend(_)
                | Item::Track(_)
                | Item::Task(_)
                | Item::Milestone(_)
                | Item::Person(_) => None,
            })
        })
}

/// Where a task is defined (not extended), and in which document.
fn find_task<'a>(
    documents: &'a [Document],
    id: &Id,
) -> Result<(usize, &'a Spanned<ast::Task>), EditError> {
    for (index, document) in documents.iter().enumerate() {
        let mut found = Vec::new();
        for item in &document.items {
            match item {
                Item::Track(track) => track_tasks(&track.node, &mut found),
                Item::Task(task) => task_tree(task, &mut found),
                Item::Extend(extend) => extend
                    .node
                    .under
                    .iter()
                    .for_each(|task| task_tree(task, &mut found)),
                Item::Milestone(_) | Item::Person(_) => {}
            }
        }
        if let Some(task) = found.into_iter().find(|task| task.node.id.node == *id) {
            return Ok((index, task));
        }
    }
    Err(if find_milestone(documents, id).is_ok() {
        EditError::NotATask(id.clone())
    } else {
        EditError::Unknown(id.clone())
    })
}

/// Where a milestone is defined, and in which document.
fn find_milestone<'a>(
    documents: &'a [Document],
    id: &Id,
) -> Result<(usize, &'a Spanned<ast::Milestone>), EditError> {
    documents
        .iter()
        .enumerate()
        .find_map(|(index, document)| {
            document.items.iter().find_map(|item| match item {
                Item::Milestone(milestone) if milestone.node.id.node == *id => {
                    Some((index, milestone))
                }
                Item::Milestone(_)
                | Item::Track(_)
                | Item::Task(_)
                | Item::Extend(_)
                | Item::Person(_) => None,
            })
        })
        .ok_or_else(|| EditError::NotAMilestone(id.clone()))
}

fn track_tasks<'a>(track: &'a ast::Track, found: &mut Vec<&'a Spanned<ast::Task>>) {
    for child in &track.children {
        match child {
            TrackChild::Track(nested) => track_tasks(&nested.node, found),
            TrackChild::Task(task) => task_tree(task, found),
        }
    }
}

fn task_tree<'a>(task: &'a Spanned<ast::Task>, found: &mut Vec<&'a Spanned<ast::Task>>) {
    found.push(task);
    for under in &task.node.under {
        task_tree(under, found);
    }
}

/// Take `task` off `milestone`'s lists, and its `nice-to-have` line too if
/// that leaves it empty.
fn off_list<'a>(
    text: &'a str,
    milestone: &'a ast::Milestone,
    task: &'a Id,
) -> impl Iterator<Item = Splice> + 'a {
    let emptied =
        !milestone.wants.is_empty() && milestone.wants.iter().all(|member| member.node == *task);
    members(milestone)
        .filter(move |member| member.node == *task)
        .map(move |member| remove_line(text, member.span.start))
        .chain(
            milestone
                .nice_to_have
                .filter(|_| emptied)
                .map(|nice| remove_line(text, nice.start)),
        )
}

/// A milestone's members: its must-haves, then its nice-to-haves.
fn members(milestone: &ast::Milestone) -> impl Iterator<Item = &Spanned<Id>> {
    milestone.needs.iter().chain(&milestone.wants)
}

/// Whether `from` needs `target`, directly or through what it needs.
fn waits_on(plan: &Plan, from: &Id, target: &Id) -> bool {
    let needs = |id: &Id| -> Vec<Id> {
        plan.task(id)
            .map(|task| &task.needs)
            .or_else(|| plan.milestone(id).map(|milestone| &milestone.needs))
            .map(|needs| needs.iter().map(|need| need.node.clone()).collect())
            .unwrap_or_default()
    };
    let mut seen = Vec::new();
    let mut stack = needs(from);
    while let Some(next) = stack.pop() {
        if next == *target {
            return true;
        }
        if seen.contains(&next) || seen.len() > plan.tasks().len() + plan.milestones().len() {
            continue;
        }
        stack.extend(needs(&next));
        seen.push(next);
    }
    false
}

/// `text`'s line break: `\r\n` if it uses them, else `\n`.
fn eol(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Where the line holding byte `at` starts.
fn line_start(text: &str, at: usize) -> usize {
    text.get(..at)
        .and_then(|before| before.rfind('\n'))
        .map_or(0, |newline| newline + 1)
}

/// Just past the line holding byte `at`, its line break included.
fn next_line(text: &str, at: usize) -> usize {
    text.get(at..)
        .and_then(|after| after.find('\n'))
        .map_or(text.len(), |newline| at + newline + 1)
}

/// How far the line holding `at` is indented. A span starts after its indent.
fn indent_of(text: &str, at: usize) -> usize {
    at - line_start(text, at)
}

/// Just past a block: the line holding `start`, and every line after it
/// that's indented further (blank lines between them included).
fn block_end(text: &str, start: usize) -> usize {
    let indent = indent_of(text, start);
    let mut end = next_line(text, start);
    let mut cursor = end;
    while cursor < text.len() {
        let next = next_line(text, cursor);
        let line = text.get(cursor..next).unwrap_or_default().trim_end();
        let content = line.trim_start_matches(' ');
        if !content.is_empty() {
            if line.len() - content.len() <= indent {
                break;
            }
            end = next;
        }
        cursor = next;
    }
    end
}

/// Just past any blank lines starting at `at`.
fn skip_blank_lines(text: &str, at: usize) -> usize {
    let mut cursor = at;
    while cursor < text.len() {
        let next = next_line(text, cursor);
        if !text.get(cursor..next).unwrap_or_default().trim().is_empty() {
            break;
        }
        cursor = next;
    }
    cursor
}

/// Where the blank lines just before `at` (a line's start) begin.
fn blank_lines_before(text: &str, at: usize) -> usize {
    let mut cursor = at;
    while cursor > 0 {
        let start = line_start(text, cursor - 1);
        if !text
            .get(start..cursor)
            .unwrap_or_default()
            .trim()
            .is_empty()
        {
            break;
        }
        cursor = start;
    }
    cursor
}

/// The indent of a block's children: its first child's, or two more than
/// the block's own if it has none.
fn child_indent(text: &str, start: usize) -> usize {
    let indent = indent_of(text, start);
    let end = block_end(text, start);
    let mut cursor = next_line(text, start);
    while cursor < end {
        let next = next_line(text, cursor);
        let line = text.get(cursor..next).unwrap_or_default().trim_end();
        let content = line.trim_start_matches(' ');
        if !content.is_empty() {
            return line.len() - content.len();
        }
        cursor = next;
    }
    indent + 2
}

/// A whole line to insert at `at` (a line's start): `indent` spaces, then
/// `content`, with a line break before it if `at` is the end of a last line
/// that has none.
fn new_line(text: &str, at: usize, indent: usize, content: &str) -> String {
    let eol = eol(text);
    let lead = if at == text.len() && !text.is_empty() && !text.ends_with('\n') {
        eol
    } else {
        ""
    };
    format!("{lead}{}{content}{eol}", " ".repeat(indent))
}

/// Remove the whole line holding `at`.
fn remove_line(text: &str, at: usize) -> Splice {
    (line_start(text, at), next_line(text, at), String::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn source(text: &str) -> Vec<Source> {
        vec![Source {
            name: String::new(),
            text: text.into(),
        }]
    }

    #[test]
    fn a_block_ends_where_the_indent_comes_back() {
        let text = "milestone m\n  a\n\n  b\n[ ] c\n";
        assert_eq!(block_end(text, 0), text.find("[ ]").unwrap_or_default());
        assert_eq!(child_indent(text, 0), 2);
        assert_eq!(child_indent("[ ] a\n", 0), 2);
    }

    #[test]
    fn line_breaks_follow_the_text() {
        let edits = add_need(
            &source("roadmap\r\n[ ] a\r\n[ ] b\r\n"),
            &Id::new("b").expect("a valid id"),
            &Id::new("a").expect("a valid id"),
        );
        assert_eq!(
            edits.map(|edits| edits.into_iter().map(|edit| edit.text).collect::<Vec<_>>()),
            Ok(vec![String::from(
                "roadmap\r\n[ ] a\r\n[ ] b\r\n  needs a\r\n"
            )])
        );
    }
}
