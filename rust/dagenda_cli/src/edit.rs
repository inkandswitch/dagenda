//! `dagenda edit`: change a plan the way the picture does, from the command
//! line. Each edit is the smallest change to the text, in the document that
//! says the thing being changed, and it refuses what would break the plan (a
//! loop, an unknown id), so a tool or an agent can't leave the plan
//! unreadable, and its edit merges cleanly with what people type nearby.
//!
//! ```console
//! $ dagenda edit status cryptoReview done plan.dagenda private.dagenda
//! $ dagenda edit need betaCriteria cgkaGaps plan.dagenda
//! $ dagenda edit dri sdnFixups @alexjg,@expede plan.dagenda
//! ```

use clap::{Subcommand, ValueEnum};
use dagenda_core::{
    check::Source,
    date::Date,
    edit::{self, Edit, EditError, Setting},
    handle::{Handle, Handles},
    id::Id,
    status::Status,
};
use std::path::PathBuf;

/// One edit, and the documents of the plan it's made in. Give every document
/// of the plan: the edit goes where the thing it changes is written.
#[derive(Debug, Subcommand)]
pub enum EditCommand {
    /// Set a task's status: a box's mark (x, -, ^, @, ?, !, *, ~, >, or a space)
    /// or its name (done, in-progress, in-review, awaiting, idea,
    /// needs-attention, research, dropped, deferred, not-started)
    Status {
        /// The task
        #[arg(value_parser = Id::new)]
        task: Id,

        /// Its status now
        #[arg(value_parser = status)]
        status: Status,

        /// The plan's documents
        #[arg(required = true)]
        documents: Vec<PathBuf>,
    },

    /// Make a task need another task, or a milestone
    Need {
        /// The task
        #[arg(value_parser = Id::new)]
        task: Id,

        /// What it needs: a task, or a milestone
        #[arg(value_parser = Id::new)]
        prerequisite: Id,

        /// The plan's documents
        #[arg(required = true)]
        documents: Vec<PathBuf>,
    },

    /// Put a task on a milestone's must-haves (`none`: on no milestone),
    /// taking it off `--from`'s list
    Scope {
        /// The task
        #[arg(value_parser = Id::new)]
        task: Id,

        /// The milestone, or `none`
        #[arg(value_parser = milestone)]
        milestone: Milestone,

        /// The milestone it's on now
        #[arg(long, value_parser = Id::new)]
        from: Option<Id>,

        /// The plan's documents
        #[arg(required = true)]
        documents: Vec<PathBuf>,
    },

    /// Move a milestone's era above or below another's
    Order {
        /// The milestone to move
        #[arg(value_parser = Id::new)]
        milestone: Id,

        /// Above or below
        position: Position,

        /// The milestone it goes next to
        #[arg(value_parser = Id::new)]
        other: Id,

        /// The plan's documents
        #[arg(required = true)]
        documents: Vec<PathBuf>,
    },

    /// Set who's responsible for a task: `@alice`, or `@alice,@bob`
    Dri {
        /// The task
        #[arg(value_parser = Id::new)]
        task: Id,

        /// Who: `@alice`, or `@alice,@bob`
        #[arg(value_parser = people)]
        people: Handles,

        /// The plan's documents
        #[arg(required = true)]
        documents: Vec<PathBuf>,
    },

    /// Set when a task is expected: 2026-11-19, 2026-11, 2026-Q4, 2026, or
    /// EOY26
    Est {
        /// The task
        #[arg(value_parser = Id::new)]
        task: Id,

        /// The date
        #[arg(value_parser = Date::new)]
        date: Date,

        /// The plan's documents
        #[arg(required = true)]
        documents: Vec<PathBuf>,
    },

    /// Set when a task or a milestone should land
    Target {
        /// The task or milestone
        #[arg(value_parser = Id::new)]
        id: Id,

        /// The date
        #[arg(value_parser = Date::new)]
        date: Date,

        /// The plan's documents
        #[arg(required = true)]
        documents: Vec<PathBuf>,
    },
}

impl EditCommand {
    /// The plan's documents, as given.
    #[must_use]
    pub fn documents(&self) -> &[PathBuf] {
        match self {
            Self::Status { documents, .. }
            | Self::Need { documents, .. }
            | Self::Scope { documents, .. }
            | Self::Order { documents, .. }
            | Self::Dri { documents, .. }
            | Self::Est { documents, .. }
            | Self::Target { documents, .. } => documents,
        }
    }

    /// Make the edit on `sources`: each changed document's new text, and
    /// what the edit says it did.
    ///
    /// # Errors
    ///
    /// Returns [`EditError`] if the edit can't be made: a document doesn't
    /// parse, an id isn't the kind it should be, or it would make a loop.
    pub fn apply(&self, sources: &[Source]) -> Result<(Vec<Edit>, String), EditError> {
        Ok(match self {
            Self::Status { task, status, .. } => (
                edit::set_status(sources, task, *status)?,
                format!("`{task}` is {}", status.describe()),
            ),
            Self::Need {
                task, prerequisite, ..
            } => (
                edit::add_need(sources, task, prerequisite)?,
                format!("`{task}` needs `{prerequisite}`"),
            ),
            Self::Scope {
                task,
                milestone,
                from,
                ..
            } => (
                edit::rescope(sources, task, from.as_ref(), milestone.0.as_ref())?,
                milestone.0.as_ref().map_or_else(
                    || format!("`{task}` is on no milestone"),
                    |milestone| format!("`{milestone}` needs `{task}`"),
                ),
            ),
            Self::Order {
                milestone,
                position,
                other,
                ..
            } => (
                edit::reorder(sources, milestone, other, *position == Position::Above)?,
                format!("`{milestone}` is {position} `{other}`"),
            ),
            Self::Dri { task, people, .. } => set(sources, task, &Setting::Dri(people.clone()))?,
            Self::Est { task, date, .. } => set(sources, task, &Setting::Est(*date))?,
            Self::Target { id, date, .. } => set(sources, id, &Setting::Target(*date))?,
        })
    }
}

/// Where a milestone's era goes, next to another's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Position {
    /// Above it: later
    Above,

    /// Below it: sooner
    Below,
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Above => "above",
            Self::Below => "below",
        })
    }
}

fn set(sources: &[Source], id: &Id, setting: &Setting) -> Result<(Vec<Edit>, String), EditError> {
    let said = format!("`{id}`'s {} is {setting}", setting.keyword());
    Ok((edit::set(sources, id, setting)?, said))
}

/// A status, by its box's mark (`x`, or `[x]`) or by its name (`done`).
///
/// ```
/// use dagenda_cli::edit::status;
/// use dagenda_core::status::Status;
///
/// assert_eq!(status("done"), Ok(Status::Done));
/// assert_eq!(status("[-]"), Ok(Status::InProgress));
/// assert_eq!(status(" "), Ok(Status::NotStarted));
/// assert!(status("finished").is_err());
/// ```
///
/// # Errors
///
/// Returns what's wrong if `arg` is neither.
pub fn status(arg: &str) -> Result<Status, String> {
    let named = match arg {
        "not-started" => Some(Status::NotStarted),
        "deferred" => Some(Status::Deferred),
        "done" => Some(Status::Done),
        "in-progress" => Some(Status::InProgress),
        "in-review" => Some(Status::InReview),
        "awaiting" => Some(Status::Awaiting),
        "idea" => Some(Status::Idea),
        "needs-attention" => Some(Status::NeedsAttention),
        "research" => Some(Status::Research),
        "dropped" => Some(Status::Dropped),
        _ => None,
    };
    let mark = arg
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(arg);
    let mut chars = mark.chars();
    let marked = match (chars.next(), chars.next()) {
        (Some(mark), None) => Status::from_mark(mark),
        _ => None,
    };
    named.or(marked).ok_or_else(|| {
        format!(
            "{arg:?} isn't a status; use a box's mark (x, -, ^, @, ?, !, *, ~, >, or a space) or a name like done or in-progress"
        )
    })
}

/// People, comma-separated: `@alice,@bob`, each once.
///
/// ```
/// use dagenda_cli::edit::people;
///
/// assert_eq!(people("@alice, @bob").map(|people| people.to_string()), Ok("@alice, @bob".into()));
/// assert!(people("@alice,@alice").is_err());
/// assert!(people("").is_err());
/// ```
///
/// # Errors
///
/// Returns what's wrong: a handle that isn't one, someone listed twice, or
/// no one.
pub fn people(arg: &str) -> Result<Handles, String> {
    let mut handles: Vec<Handle> = Vec::new();
    for written in arg.split(',').map(str::trim) {
        let handle = Handle::new(written).map_err(|e| format!("{written:?}: {e}"))?;
        if handles.contains(&handle) {
            return Err(format!("`{handle}` is listed twice"));
        }
        handles.push(handle);
    }
    Handles::new(handles).ok_or_else(|| String::from("expected someone, as in @alice"))
}

/// A milestone, or none: the work on no milestone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Milestone(Option<Id>);

/// A milestone's id, or `none` for no milestone.
fn milestone(arg: &str) -> Result<Milestone, String> {
    if arg == "none" {
        return Ok(Milestone(None));
    }
    Id::new(arg)
        .map(|id| Milestone(Some(id)))
        .map_err(|e| e.to_string())
}
