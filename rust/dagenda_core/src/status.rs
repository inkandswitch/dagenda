//! Where a task stands: the box at the start of its line.

use core::fmt;

/// A task's status, written as a box at the start of its line.
///
/// Only [`Status::Done`] counts as finished: a milestone is reached when all
/// its must-haves are done, and a task that `needs` another waits for it to
/// be done.
///
/// ```
/// use dagenda_core::status::Status;
///
/// assert_eq!(Status::from_mark('x'), Some(Status::Done));
/// assert_eq!(Status::Deferred.to_string(), "[>]");
/// assert!(!Status::Dropped.is_finished());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Status {
    /// `[ ]`: not started.
    NotStarted,

    /// `[>]`: deferred.
    Deferred,

    /// `[x]`: done, the only finished status.
    Done,

    /// `[-]`: in progress.
    InProgress,

    /// `[^]`: in review: the work is up for review, as a pull request.
    InReview,

    /// `[@]`: awaiting a reply: the next move is someone else's.
    Awaiting,

    /// `[?]`: an idea, or work that needs clarifying before it can start.
    Idea,

    /// `[!]`: defined work that's stuck, blocked on a decision, or at risk.
    NeedsAttention,

    /// `[*]`: research.
    Research,

    /// `[~]`: dropped. Kept, crossed out, so the plan remembers what was
    /// decided against.
    Dropped,
}

impl Status {
    /// Every status, in the order the boxes are documented.
    pub const ALL: [Self; 10] = [
        Self::NotStarted,
        Self::Deferred,
        Self::Done,
        Self::InProgress,
        Self::InReview,
        Self::Awaiting,
        Self::Idea,
        Self::NeedsAttention,
        Self::Research,
        Self::Dropped,
    ];

    /// The status whose box holds `mark`, as `x` is in `[x]`.
    #[must_use]
    pub const fn from_mark(mark: char) -> Option<Self> {
        match mark {
            ' ' => Some(Self::NotStarted),
            '>' => Some(Self::Deferred),
            'x' => Some(Self::Done),
            '-' => Some(Self::InProgress),
            '^' => Some(Self::InReview),
            '@' => Some(Self::Awaiting),
            '?' => Some(Self::Idea),
            '!' => Some(Self::NeedsAttention),
            '*' => Some(Self::Research),
            '~' => Some(Self::Dropped),
            _ => None,
        }
    }

    /// The character inside the box.
    #[must_use]
    pub const fn mark(self) -> char {
        match self {
            Self::NotStarted => ' ',
            Self::Deferred => '>',
            Self::Done => 'x',
            Self::InProgress => '-',
            Self::InReview => '^',
            Self::Awaiting => '@',
            Self::Idea => '?',
            Self::NeedsAttention => '!',
            Self::Research => '*',
            Self::Dropped => '~',
        }
    }

    /// Its name as one word: what the CLI takes, and the class on its card
    /// in the picture.
    ///
    /// ```
    /// use dagenda_core::status::Status;
    ///
    /// assert_eq!(Status::InReview.name(), "in-review");
    /// ```
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NotStarted => "not-started",
            Self::Deferred => "deferred",
            Self::Done => "done",
            Self::InProgress => "in-progress",
            Self::InReview => "in-review",
            Self::Awaiting => "awaiting",
            Self::Idea => "idea",
            Self::NeedsAttention => "needs-attention",
            Self::Research => "research",
            Self::Dropped => "dropped",
        }
    }

    /// What the status means, in a few words.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::NotStarted => "not started",
            Self::Deferred => "deferred",
            Self::Done => "done",
            Self::InProgress => "in progress",
            Self::InReview => "in review",
            Self::Awaiting => "awaiting a reply",
            Self::Idea => "an idea",
            Self::NeedsAttention => "needs attention",
            Self::Research => "research",
            Self::Dropped => "dropped",
        }
    }

    /// Whether the work is finished. Only [`Status::Done`] is.
    #[must_use]
    pub const fn is_finished(self) -> bool {
        matches!(self, Self::Done)
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}]", self.mark())
    }
}

#[cfg(test)]
mod tests {
    use super::Status;
    use alloc::collections::BTreeSet;

    #[test]
    fn marks_and_names_are_each_status_s_own() {
        let names: BTreeSet<_> = Status::ALL.iter().map(|status| status.name()).collect();
        assert_eq!(names.len(), Status::ALL.len(), "a name per status");
        for status in Status::ALL {
            assert_eq!(Status::from_mark(status.mark()), Some(status));
        }
    }
}
