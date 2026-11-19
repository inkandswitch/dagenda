//! One document as written, before its ids are resolved.
//!
//! The types follow the indentation: a [`Track`] holds tracks and tasks, a
//! [`Task`] holds its [`Details`] and subtasks, and a [`Milestone`] lists
//! what it needs and wants. A structure the notation doesn't allow, such as a
//! track inside a task, has no representation here.

use crate::{
    handle::Handle,
    id::Id,
    reference::Ref,
    span::{Span, Spanned},
    status::Status,
    text::{Note, Text},
};
use alloc::vec::Vec;

/// A parsed document: one `roadmap` text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// The `title` line, if there is one.
    pub title: Option<Spanned<Text>>,

    /// The top-level blocks, in source order.
    pub items: Vec<Item>,
}

/// A block at the top level of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// `track <id> [<label>]`
    Track(Spanned<Track>),

    /// `<box> task <id> [<label>]`, outside any track.
    Task(Spanned<Task>),

    /// `milestone <id> [<label>]`
    Milestone(Spanned<Milestone>),

    /// `extend <id>`
    Extend(Spanned<Extend>),
}

/// A bucket of work. Its span covers its first line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    /// What it's called.
    pub id: Spanned<Id>,

    /// The display text in brackets, if any.
    pub label: Option<Spanned<Text>>,

    /// The person responsible for it.
    pub dri: Option<Spanned<Handle>>,

    /// Notes about it.
    pub notes: Vec<Spanned<Note>>,

    /// The tracks and tasks inside it, in source order.
    pub children: Vec<TrackChild>,
}

/// Something inside a track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackChild {
    /// A nested track.
    Track(Spanned<Track>),

    /// A task that lives in this track.
    Task(Spanned<Task>),
}

/// A unit of work. Its span covers its first line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// The box at the start of the line.
    pub status: Spanned<Status>,

    /// What it's called.
    pub id: Spanned<Id>,

    /// The display text in brackets, if any.
    pub label: Option<Spanned<Text>>,

    /// Its `needs`, `est`, `dri`, and so on.
    pub details: Details,

    /// The tasks indented under it.
    pub subtasks: Vec<Spanned<Task>>,
}

/// What a task, or an `extend` of one, can say about it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Details {
    /// What it can't finish without, from every `needs` line.
    pub needs: Vec<Spanned<Id>>,

    /// The estimate, as written (`2w`).
    pub est: Option<Spanned<Text>>,

    /// When it should land, as written (`2026-Q4`, `EOY26`).
    pub target: Option<Spanned<Text>>,

    /// The person responsible for it.
    pub dri: Option<Spanned<Handle>>,

    /// Its issues and pull requests.
    pub refs: Vec<Spanned<Ref>>,

    /// Notes about it.
    pub notes: Vec<Spanned<Note>>,
}

/// A gate: reached when every must-have is done. Its span covers its first
/// line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Milestone {
    /// What it's called.
    pub id: Spanned<Id>,

    /// The display text in brackets, if any.
    pub label: Option<Spanned<Text>>,

    /// When it should be reached, as written.
    pub target: Option<Spanned<Text>>,

    /// Notes about it.
    pub notes: Vec<Spanned<Note>>,

    /// The must-haves: the members above the cut line.
    pub needs: Vec<Spanned<Id>>,

    /// The nice-to-haves: the members below the cut line.
    pub wants: Vec<Spanned<Id>>,

    /// Where the `---` is, if there is one.
    pub cut: Option<Span>,
}

/// Details and subtasks for a task defined in another document. Its span
/// covers its first line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extend {
    /// The task it adds to.
    pub id: Spanned<Id>,

    /// What it adds.
    pub details: Details,

    /// Subtasks it adds, which live here rather than in the task's document.
    pub subtasks: Vec<Spanned<Task>>,
}
