//! Plans that parse and resolve, but don't mean what they seem to.
//!
//! These are warnings rather than errors: a plan is sometimes mid-edit, and
//! sometimes shows a problem on purpose.

use super::Location;
use crate::id::Id;
use core::fmt;

/// Something worth a second look, and where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// Where it is.
    pub at: Location,

    /// What it is.
    pub kind: WarningKind,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

/// The things worth a second look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WarningKind {
    /// A milestone with no must-haves, which is always reached.
    EmptyMilestone(Id),

    /// A dropped must-have: the milestone can never be reached.
    DroppedMustHave {
        /// The milestone.
        milestone: Id,

        /// The dropped task.
        task: Id,
    },

    /// A deferred must-have, which still holds up the milestone.
    DeferredMustHave {
        /// The milestone.
        milestone: Id,

        /// The deferred task.
        task: Id,
    },

    /// A task that needs a dropped task, so it can never finish.
    NeedsDropped {
        /// The task.
        task: Id,

        /// What it needs.
        need: Id,
    },

    /// A done task that needs something unfinished.
    DoneBeforeNeeds {
        /// The task.
        task: Id,

        /// What it needs.
        need: Id,
    },

    /// An `extend` of a task defined in the same document.
    ExtendSameDocument(Id),
}

impl fmt::Display for WarningKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyMilestone(milestone) => write!(
                f,
                "`{milestone}` has no must-haves, only nice-to-haves, so it counts as reached"
            ),
            Self::DroppedMustHave { milestone, task } => write!(
                f,
                "`{task}` is dropped, so `{milestone}` can never be reached; take it off the list, or make it a nice-to-have"
            ),
            Self::DeferredMustHave { milestone, task } => write!(
                f,
                "`{task}` is deferred but still holds up `{milestone}`; make it a nice-to-have if `{milestone}` can go without it"
            ),
            Self::NeedsDropped { task, need } => write!(
                f,
                "`{task}` needs `{need}`, which is dropped, so `{task}` can never finish"
            ),
            Self::DoneBeforeNeeds { task, need } => {
                write!(f, "`{task}` is done, but it needs `{need}`, which isn't")
            }
            Self::ExtendSameDocument(id) => write!(
                f,
                "`{id}` is defined in this document, so put these details on it directly"
            ),
        }
    }
}
