//! Roadmaps as dependency graphs: the notation, its parser, and the model
//! every view is drawn from.
//!
//! A plan is a DAG of work. The text is a tree that says where each thing
//! lives: a track holds tasks, and a task holds its details and the tasks it needs.
//! `needs` lines and milestone members make the graph on top of the tree, by
//! id, so they can cross tracks and documents.
//!
//! ```text
//! roadmap
//! title Product Roadmap
//!
//! track sync [Sync]
//!   [-] engine [New sync engine]
//!     needs protocol
//!     dri @alice
//!     pr gh:owner/repo#123
//!   [x] protocol [Wire protocol]
//!   [ ] offline [Offline mode]
//!
//! milestone v2 [Version 2]
//!   target EOY26
//!   engine
//!   nice-to-have
//!     offline
//! ```
//!
//! | Form                          | Meaning                                                                 |
//! |-------------------------------|-------------------------------------------------------------------------|
//! | `roadmap`                     | The header, always the first line                                       |
//! | `title …`                     | The document's title                                                    |
//! | `track <id> [<label>]`        | A bucket. Tracks nest                                                   |
//! | `<box> <id> [<label>]`        | A unit of work, in the track around it. A task written under another is one it needs |
//! | `milestone <id> [<label>]`    | A gate, listing the tasks and milestones it needs, one per line         |
//! | `nice-to-have`                | In a milestone: the members indented under it are wanted, not required  |
//! | `extend <id>`                 | Details, and tasks it needs, for a task defined in another document     |
//! | `needs <id>, <id>, …`         | This can't finish until those do                                        |
//! | `est <date>`                  | When it's expected to land, as in `2026-11-19`                          |
//! | `target <date>`               | When it should land, as in `2026-Q4` or `EOY26`                         |
//! | `colour <name or #rrggbb>`    | A track's colour: teal, indigo, rose, ochre, sage, plum, sky, terracotta, or any `#rrggbb` (`color` works too); otherwise the next free one |
//! | `dri @<handle>`               | The person responsible                                                  |
//! | `pr gh:<owner>/<repo>#<n>`    | A pull request (`issue gh:…` for an issue)                              |
//! | `link <url> [<label>]`        | Any other page about it: a GitHub comment, a doc, a demo                |
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
//!        [x] parser [Parser]
//!        [ ] resolver [Resolver]
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
//! assert_eq!((progress.work.done, progress.work.total), (1, 2), "the work behind it");
//! # Ok::<(), Box<dyn core::error::Error>>(())
//! ```

#![no_std]

extern crate alloc;

pub mod ast;
pub mod check;
pub mod colour;
pub mod date;
pub mod edit;
pub mod errors;
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

use parser::ParseErrors;
use plan::{Plan, ResolveErrors};
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
    Parse(#[from] ParseErrors),

    /// Well-formed text that doesn't resolve.
    #[error(transparent)]
    Resolve(#[from] ResolveErrors),
}

impl Error {
    /// Where in the source the problem is.
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Parse(e) => e.first().span(),
            Self::Resolve(e) => e.first().location().span,
        }
    }
}
