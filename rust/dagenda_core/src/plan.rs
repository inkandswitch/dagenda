//! Documents resolved into one [`Plan`]: ids first, then the state derived
//! from them.
//!
//! A plan is made of every document someone can read: a public plan, a
//! private plan, and so on. Ids are global across them, so a private plan can
//! `extend` a public task, or say it `needs` one. Each node and each detail
//! has exactly one owning document.
//!
//! Everything that can be derived is: whether a milestone is reached, which
//! tasks are ready or blocked, the critical path, and the warnings about
//! plans that parse but don't mean what they seem to.

mod build;
pub mod warning;

use crate::{
    ast::Document,
    date::Date,
    handle::Handle,
    id::Id,
    reference::{self, Ref},
    span::Span,
    status::Status,
    text::{Note, Text},
};
use alloc::{collections::BTreeMap, vec::Vec};
use build::Builder;
use core::fmt;
use thiserror::Error;
use warning::Warning;

/// Every document someone can read, resolved into one graph.
///
/// ```
/// use dagenda_core::{id::Id, parser, plan::{Plan, Readiness}};
///
/// let public = parser::document("roadmap\ntrack am [Automerge]\n  [ ] task signatures\n")?;
/// let private = parser::document("roadmap\nextend signatures\n  dri @alice\n")?;
/// let plan = Plan::resolve(&[public, private])?;
///
/// let signatures = Id::new("signatures")?;
/// let task = plan.task(&signatures).ok_or("no task")?;
/// assert_eq!(task.dri.as_ref().map(|dri| dri.node.name()), Some("alice"));
/// assert_eq!(plan.readiness(&signatures), Some(Readiness::Ready));
/// # Ok::<(), Box<dyn core::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Each document's title, by document.
    titles: Vec<Option<Located<Text>>>,

    tracks: Vec<Track>,
    tasks: Vec<Task>,
    milestones: Vec<Milestone>,

    /// Every id, and what it names.
    index: BTreeMap<Id, Entry>,

    /// Whether each task, then each milestone, is finished.
    finished: Vec<bool>,

    /// The longest chain of unfinished work ending at each task, then each
    /// milestone.
    chain: Vec<Link>,

    /// Sorted by where they are.
    warnings: Vec<Warning>,
}

impl Plan {
    /// Resolve documents into one plan. Each document's [`DocumentId`] is
    /// its index in `documents`.
    ///
    /// # Errors
    ///
    /// Returns a [`ResolveError`] for the first id defined twice, reference
    /// to nothing (or to the wrong kind of thing), detail set by two
    /// documents, or set of tasks and milestones that need each other.
    pub fn resolve(documents: &[Document]) -> Result<Self, ResolveError> {
        let mut builder = Builder::default();
        for (index, document) in documents.iter().enumerate() {
            builder.document(DocumentId(index), document)?;
        }
        builder.extends()?;
        builder.references()?;

        let graph = builder.graph();
        let order = builder.order(&graph)?;
        Ok(builder.finish(&graph, &order))
    }

    /// A document's title, if it has one.
    #[must_use]
    pub fn title(&self, document: DocumentId) -> Option<&Text> {
        self.titles
            .get(document.0)
            .and_then(Option::as_ref)
            .map(|title| &title.node)
    }

    /// Every track, in the order the documents define them.
    #[must_use]
    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    /// Every task, in the order the documents define them.
    #[must_use]
    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    /// Every milestone, in the order the documents define them.
    #[must_use]
    pub fn milestones(&self) -> &[Milestone] {
        &self.milestones
    }

    /// What `id` names, if anything.
    #[must_use]
    pub fn kind(&self, id: &Id) -> Option<NodeKind> {
        self.index.get(id).map(|entry| entry.node.kind())
    }

    /// The track called `id`.
    #[must_use]
    pub fn track(&self, id: &Id) -> Option<&Track> {
        match self.index.get(id)?.node {
            Node::Track(index) => self.tracks.get(index),
            Node::Task(_) | Node::Milestone(_) => None,
        }
    }

    /// The task called `id`.
    #[must_use]
    pub fn task(&self, id: &Id) -> Option<&Task> {
        match self.index.get(id)?.node {
            Node::Task(index) => self.tasks.get(index),
            Node::Track(_) | Node::Milestone(_) => None,
        }
    }

    /// The milestone called `id`.
    #[must_use]
    pub fn milestone(&self, id: &Id) -> Option<&Milestone> {
        match self.index.get(id)?.node {
            Node::Milestone(index) => self.milestones.get(index),
            Node::Track(_) | Node::Task(_) => None,
        }
    }

    /// Everything that parses but doesn't mean what it seems to, in source
    /// order.
    #[must_use]
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    /// Whether `id` is a done task or a reached milestone.
    #[must_use]
    pub fn is_finished(&self, id: &Id) -> bool {
        self.flat(id)
            .and_then(|node| self.finished.get(node).copied())
            .unwrap_or(false)
    }

    /// How far along a milestone is.
    #[must_use]
    pub fn progress(&self, milestone: &Id) -> Option<Progress> {
        let milestone = self.milestone(milestone)?;
        Some(Progress {
            needs: self.count(&milestone.needs),
            wants: self.count(&milestone.wants),
        })
    }

    /// Whether a task can be worked on now.
    #[must_use]
    pub fn readiness(&self, task: &Id) -> Option<Readiness> {
        let task = self.task(task)?;
        Some(match task.status {
            Status::Done => Readiness::Finished,
            Status::Dropped => Readiness::Dropped,
            Status::NotStarted
            | Status::Deferred
            | Status::InProgress
            | Status::Idea
            | Status::NeedsAttention
            | Status::Research => {
                let waiting: Vec<Id> = task
                    .needs
                    .iter()
                    .filter(|need| !self.is_finished(&need.node))
                    .map(|need| need.node.clone())
                    .collect();
                if waiting.is_empty() {
                    Readiness::Ready
                } else {
                    Readiness::Blocked(waiting)
                }
            }
        })
    }

    /// The longest chain of unfinished tasks that a task or milestone waits
    /// on, first to do first. A task ends its own chain. Until estimates are
    /// parsed, "longest" counts tasks.
    ///
    /// ```
    /// use dagenda_core::id::Id;
    ///
    /// let plan = dagenda_core::parse(
    ///     "roadmap
    ///      [ ] task a
    ///      [ ] task b
    ///        needs a
    ///      [ ] task c
    ///      milestone m
    ///        b
    ///        c",
    /// )?;
    /// let path: Vec<&str> = plan.critical_path(&Id::new("m")?).iter().map(|id| id.as_str()).collect();
    /// assert_eq!(path, ["a", "b"]);
    /// # Ok::<(), Box<dyn core::error::Error>>(())
    /// ```
    #[must_use]
    pub fn critical_path(&self, id: &Id) -> Vec<&Id> {
        let mut path = Vec::new();
        let mut next = self.flat(id);
        while let Some(node) = next {
            let Some(link) = self.chain.get(node) else {
                break;
            };
            if link.length == 0 || path.len() > self.tasks.len() {
                break;
            }
            if let Some(task) = self.tasks.get(node) {
                path.push(&task.id);
            }
            next = link.previous;
        }
        path.reverse();
        path
    }

    /// A task's or milestone's position in `finished` and `chain`.
    fn flat(&self, id: &Id) -> Option<usize> {
        flat(&self.index, self.tasks.len(), id)
    }

    fn count(&self, members: &[Located<Id>]) -> Count {
        Count {
            done: members
                .iter()
                .filter(|member| self.is_finished(&member.node))
                .count(),
            total: members.len(),
        }
    }
}

/// Which document something came from: its index in the slice given to
/// [`Plan::resolve`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentId(usize);

impl DocumentId {
    /// The document at `index` in the slice given to [`Plan::resolve`].
    #[must_use]
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    /// Its index in the slice given to [`Plan::resolve`].
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// A place in one of a plan's documents.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Location {
    /// Which document.
    pub document: DocumentId,

    /// Where in it.
    pub span: Span,
}

/// A value and where in the plan it was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located<T> {
    /// The value.
    pub node: T,

    /// Where it was written.
    pub at: Location,
}

/// A bucket of work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    /// What it's called.
    pub id: Id,

    /// Its display text, if it has any.
    pub label: Option<Text>,

    /// The track it's nested in.
    pub parent: Option<Id>,

    /// The person responsible for it.
    pub dri: Option<Located<Handle>>,

    /// Notes about it.
    pub notes: Vec<Located<Note>>,

    /// Its `track` line.
    pub defined: Location,
}

/// A unit of work, with every document's details about it merged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// What it's called.
    pub id: Id,

    /// Its display text, if it has any.
    pub label: Option<Text>,

    /// Where it stands.
    pub status: Status,

    /// The track it lives in. A task written under another lives in that
    /// one's track.
    pub track: Option<Id>,

    /// What it can't finish without: its `needs`, then the tasks written
    /// under it.
    pub needs: Vec<Located<Id>>,

    /// When it's expected to land.
    pub est: Option<Located<Date>>,

    /// When it should land.
    pub target: Option<Located<Date>>,

    /// The person responsible for it.
    pub dri: Option<Located<Handle>>,

    /// Its issues and pull requests.
    pub refs: Vec<Located<Ref>>,

    /// Links to anything else about it.
    pub links: Vec<Located<reference::Link>>,

    /// Notes about it.
    pub notes: Vec<Located<Note>>,

    /// Its `task` line.
    pub defined: Location,

    /// Its status box.
    pub status_at: Location,
}

/// A gate, reached when every must-have is finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Milestone {
    /// What it's called.
    pub id: Id,

    /// Its display text, if it has any.
    pub label: Option<Text>,

    /// When it should be reached.
    pub target: Option<Located<Date>>,

    /// Notes about it.
    pub notes: Vec<Located<Note>>,

    /// The must-haves: tasks and milestones.
    pub needs: Vec<Located<Id>>,

    /// The nice-to-haves, below the cut line.
    pub wants: Vec<Located<Id>>,

    /// Its `milestone` line.
    pub defined: Location,
}

/// How many of a milestone's members are finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Count {
    /// Finished members.
    pub done: usize,

    /// All members.
    pub total: usize,
}

impl Count {
    /// Whether every member is finished.
    #[must_use]
    pub const fn is_complete(self) -> bool {
        self.done == self.total
    }
}

/// How far along a milestone is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Progress {
    /// The must-haves.
    pub needs: Count,

    /// The nice-to-haves.
    pub wants: Count,
}

impl Progress {
    /// Whether the milestone is reached: every must-have is finished.
    #[must_use]
    pub const fn is_reached(self) -> bool {
        self.needs.is_complete()
    }
}

/// Whether a task can be worked on now.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Readiness {
    /// It's done.
    Finished,

    /// It's dropped.
    Dropped,

    /// Everything it needs is finished.
    Ready,

    /// It waits on these, which aren't finished.
    Blocked(Vec<Id>),
}

/// What an id names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// A track.
    Track,

    /// A task.
    Task,

    /// A milestone.
    Milestone,
}

impl fmt::Display for NodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Track => "a track",
            Self::Task => "a task",
            Self::Milestone => "a milestone",
        })
    }
}

/// Where a reference is, for errors about what it can point at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Site {
    /// In a `needs` line.
    Needs,

    /// In a milestone's list.
    Member,

    /// On an `extend` line.
    Extend,
}

impl fmt::Display for Site {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Needs => "`needs` points at tasks and milestones",
            Self::Member => "a milestone lists tasks and milestones",
            Self::Extend => "`extend` adds detail to tasks",
        })
    }
}

/// What an id names: an index into one of the plan's lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Node {
    Track(usize),
    Task(usize),
    Milestone(usize),
}

impl Node {
    const fn kind(self) -> NodeKind {
        match self {
            Self::Track(_) => NodeKind::Track,
            Self::Task(_) => NodeKind::Task,
            Self::Milestone(_) => NodeKind::Milestone,
        }
    }
}

/// An id's node, and where it's defined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Entry {
    node: Node,
    at: Location,
}

/// The end of the longest chain of unfinished work reaching a node.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Link {
    /// Unfinished tasks on the chain, this one included.
    length: usize,

    /// The node before this one on the chain.
    previous: Option<usize>,
}

/// A task's or milestone's position in a list of tasks followed by
/// milestones.
fn flat(index: &BTreeMap<Id, Entry>, tasks: usize, id: &Id) -> Option<usize> {
    match index.get(id)?.node {
        Node::Task(task) => Some(task),
        Node::Milestone(milestone) => Some(tasks + milestone),
        Node::Track(_) => None,
    }
}

/// Ids in backticks, in order: `a` → `b` → `c`.
struct Chain<'a>(&'a [Id]);

impl fmt::Display for Chain<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, id) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(" → ")?;
            }
            write!(f, "`{id}`")?;
        }
        Ok(())
    }
}

/// The message for a cycle: one task that needs itself, or several that need
/// each other.
struct Loop<'a>(&'a [Id]);

impl fmt::Display for Loop<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            [single, _] => write!(f, "`{single}` needs itself, so it can never finish"),
            ids => write!(
                f,
                "these need each other, so none can finish: {}",
                Chain(ids)
            ),
        }
    }
}

/// Documents that don't resolve.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{kind}")]
pub struct ResolveError {
    at: Location,
    kind: ResolveErrorKind,
}

impl ResolveError {
    /// An error at `at`.
    #[must_use]
    pub const fn new(at: Location, kind: ResolveErrorKind) -> Self {
        Self { at, kind }
    }

    /// Where the problem is.
    #[must_use]
    pub const fn location(&self) -> Location {
        self.at
    }

    /// What the problem is.
    #[must_use]
    pub const fn kind(&self) -> &ResolveErrorKind {
        &self.kind
    }
}

/// The ways documents can fail to resolve.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResolveErrorKind {
    /// An id defined twice, in one document or across two.
    #[error("`{id}` is defined more than once")]
    Duplicate {
        /// The id.
        id: Id,

        /// Where it was defined first.
        first: Location,
    },

    /// A reference to an id nothing defines.
    #[error("nothing in the plan is called `{0}`")]
    Unknown(Id),

    /// A reference to the wrong kind of thing.
    #[error("`{id}` is {is}, but {site}")]
    WrongKind {
        /// The id.
        id: Id,

        /// What it names.
        is: NodeKind,

        /// Where the reference is.
        site: Site,
    },

    /// A detail that two documents both set.
    #[error("`{key}` is already set for `{id}`; each detail has one owner")]
    Conflict {
        /// The task.
        id: Id,

        /// The detail's keyword.
        key: &'static str,

        /// Where it was set first.
        first: Location,
    },

    /// Tasks and milestones that need each other.
    #[error("{}", Loop(.0))]
    Cycle(Vec<Id>),
}
