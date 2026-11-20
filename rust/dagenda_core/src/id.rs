//! Ids: what tracks, tasks, and milestones are called and referred to by.

use alloc::string::String;
use core::fmt;
use thiserror::Error;

/// The notation's keywords. None of them can be an id, so a line holding just
/// one is never mistaken for a milestone member.
pub const KEYWORDS: [&str; 18] = [
    "avatar",
    "color",
    "colour",
    "dri",
    "est",
    "extend",
    "issue",
    "link",
    "milestone",
    "needs",
    "nice-to-have",
    "note",
    "pr",
    "roadmap",
    "target",
    "task",
    "title",
    "track",
];

/// The name of a track, task, or milestone: an ASCII letter, then letters,
/// digits, `-`, and `_`.
///
/// Ids are case-sensitive, and global across every document in a plan, so a
/// private plan can say it `needs` a task in a public one.
///
/// ```
/// use dagenda_core::id::{Id, InvalidId};
///
/// assert_eq!(Id::new("newRepo")?.as_str(), "newRepo");
/// assert_eq!(Id::new("2fast"), Err(InvalidId::Start('2')));
/// assert_eq!(Id::new("note"), Err(InvalidId::Keyword("note")));
/// # Ok::<(), InvalidId>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(String);

impl Id {
    /// Check and wrap an id.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidId`] if `id` is empty, doesn't start with a letter,
    /// contains anything but letters, digits, `-`, and `_`, or is a keyword.
    pub fn new(id: &str) -> Result<Self, InvalidId> {
        let mut chars = id.chars();
        match chars.next() {
            None => return Err(InvalidId::Empty),
            Some(first) if !first.is_ascii_alphabetic() => return Err(InvalidId::Start(first)),
            Some(_) => {}
        }

        if let Some(c) = chars.find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))) {
            return Err(InvalidId::Char(c));
        }

        if let Some(keyword) = KEYWORDS.iter().find(|keyword| **keyword == id) {
            return Err(InvalidId::Keyword(keyword));
        }

        Ok(Self(id.into()))
    }

    /// The id as written.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why a string isn't a valid [`Id`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum InvalidId {
    /// Nothing there.
    #[error("expected an id")]
    Empty,

    /// It starts with something other than a letter.
    #[error("an id starts with a letter, not {0:?}")]
    Start(char),

    /// A character outside letters, digits, `-`, and `_`.
    #[error("an id can't contain {0:?}; use letters, digits, `-`, or `_`")]
    Char(char),

    /// One of the notation's [`KEYWORDS`].
    #[error("`{0}` is a keyword, so it can't be an id")]
    Keyword(&'static str),
}
