//! Roadmaps as dependency graphs: the notation, its parser, and the model
//! every view is drawn from.
//!
//! A plan is a DAG of work. The text is a tree that says where each thing
//! lives: a track holds tasks, and a task holds its details and subtasks.
//! `needs` lines and milestone members make the graph on top of the tree, by
//! id, so they can cross tracks and documents.
//!
//! ```text
//! roadmap
//! title Department of Data
//!
//! track sync [Sync]
//!   [-] task newRepo [New repo architecture]
//!     needs fragments
//!     dri @alice
//!     pr gh:owner/repo#123
//!   [x] task fragments [Fragments]
//!   [ ] task docTypes [Custom doc types]
//!
//! milestone repo3 [Repo 3]
//!   target EOY26
//!   newRepo
//!   ---
//!   docTypes
//! ```
//!
//! | Form                          | Meaning                                                                 |
//! |-------------------------------|-------------------------------------------------------------------------|
//! | `roadmap`                     | The header, always the first line                                       |
//! | `title …`                     | The document's title                                                    |
//! | `track <id> [<label>]`        | A bucket. Tracks nest                                                   |
//! | `<box> task <id> [<label>]`   | A unit of work, in the track around it. Tasks nest: inner ones are subtasks |
//! | `milestone <id> [<label>]`    | A gate, listing the tasks and milestones it needs, one per line         |
//! | `---`                         | A milestone's cut line: the members below it are nice-to-haves          |
//! | `extend <id>`                 | Details and subtasks for a task defined in another document             |
//! | `needs <id>, <id>, …`         | This can't finish until those do                                        |
//! | `est <duration>`              | An estimate, as in `2w`                                                 |
//! | `target <when>`               | When it should land, as in `2026-Q4` or `EOY26`                         |
//! | `dri @<handle>`               | The person responsible                                                  |
//! | `pr gh:<owner>/<repo>#<n>`    | A pull request (`issue gh:…` for an issue)                              |
//! | `note <text>`                 | A note in markdown; leave the text off and indent it below for a long one |
//! | `// …`                        | A comment line                                                          |
//!
//! Labels are the display text in brackets; write `\]` for a `]` inside one.
//! The [`parser`] module has the full grammar and the indentation rules.
//!
//! # Status boxes
//!
//! Every task starts with one, and only `[x]` counts as finished:
//!
//! | Box   | Status                       |
//! |-------|------------------------------|
//! | `[ ]` | Not started                  |
//! | `[>]` | Deferred                     |
//! | `[x]` | Done                         |
//! | `[-]` | In progress                  |
//! | `[?]` | Idea, or needs clarification |
//! | `[!]` | Needs attention              |
//! | `[*]` | Research                     |
//! | `[~]` | Dropped                      |
//!
//! # Documents
//!
//! Which document a block is written in decides who can read it, so the
//! notation has no `private` flag. A private plan adds to a public task with
//! `extend`, and ids are global, so either can `needs` the other. Each detail
//! has one owner: two documents can't both set a task's `dri`.
//!
//! # Pipeline
//!
//! ```text
//! source ──parser::document──▶ ast::Document ──┐
//! source ──parser::document──▶ ast::Document ──┼──Plan::resolve──▶ plan::Plan
//!                                              ┘   (ids, then derived state)
//!                                                          │
//!                                    ┌─────────────────────┴───────────┐
//!                                    ▼                                 ▼
//!                             Plan::to_text                      Plan::to_svg
//!                          (outline: milestones,          (tech_tree: layout,
//!                           tracks, status boxes)          then SVG)
//! ```
//!
//! [`parse`] does both steps for a single document.
//!
//! ```
//! use dagenda_core::{id::Id, plan::Readiness};
//!
//! let plan = dagenda_core::parse(
//!     "roadmap
//!      track core [Core]
//!        [x] task parser [Parser]
//!        [ ] task resolver [Resolver]
//!          needs parser
//!      milestone itParses [It parses]
//!        parser
//!        resolver",
//! )?;
//!
//! assert_eq!(plan.readiness(&Id::new("resolver")?), Some(Readiness::Ready));
//!
//! let progress = plan.progress(&Id::new("itParses")?).ok_or("no milestone")?;
//! assert_eq!((progress.needs.done, progress.needs.total), (1, 2));
//! # Ok::<(), Box<dyn core::error::Error>>(())
//! ```

#![no_std]

extern crate alloc;

pub mod ast;
pub mod handle;
pub mod id;
pub mod outline;
pub mod parser;
pub mod plan;
pub mod reference;
pub mod span;
pub mod status;
pub mod tech_tree;
pub mod text;

use parser::ParseError;
use plan::{Plan, ResolveError};
use span::Span;
use thiserror::Error;

/// Parse and resolve a single document.
///
/// # Errors
///
/// Returns [`Error::Parse`] for malformed text, and [`Error::Resolve`] for
/// well-formed text whose ids don't resolve.
pub fn parse(source: &str) -> Result<Plan, Error> {
    Ok(Plan::resolve(&[parser::document(source)?])?)
}

/// Anything that can go wrong in [`parse`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Error {
    /// Malformed source text.
    #[error(transparent)]
    Parse(#[from] ParseError),

    /// Well-formed text that doesn't resolve.
    #[error(transparent)]
    Resolve(#[from] ResolveError),
}

impl Error {
    /// Where in the source the problem is.
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Parse(e) => e.span(),
            Self::Resolve(e) => e.location().span,
        }
    }
}
