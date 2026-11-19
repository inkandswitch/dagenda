//! References to GitHub issues and pull requests.

use alloc::{format, string::String};
use core::{fmt, num::NonZeroU64};
use thiserror::Error;

/// A GitHub issue or pull request, written `gh:owner/repo#123`.
///
/// ```
/// use dagenda_core::reference::GithubRef;
///
/// let reference = GithubRef::new("gh:inkandswitch/keyhive#365")?;
/// assert_eq!(reference.owner(), "inkandswitch");
/// assert_eq!(reference.repo(), "keyhive");
/// assert_eq!(reference.number().get(), 365);
/// # Ok::<(), dagenda_core::reference::InvalidGithubRef>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GithubRef {
    owner: String,
    repo: String,
    number: NonZeroU64,
}

impl GithubRef {
    /// Parse a reference as written.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidGithubRef`] if `written` isn't
    /// `gh:<owner>/<repo>#<number>`.
    pub fn new(written: &str) -> Result<Self, InvalidGithubRef> {
        let rest = written
            .strip_prefix("gh:")
            .ok_or(InvalidGithubRef::Scheme)?;
        let (path, number) = rest.split_once('#').ok_or(InvalidGithubRef::Number)?;
        let (owner, repo) = path.split_once('/').ok_or(InvalidGithubRef::Repo)?;

        if owner.is_empty() || !owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(InvalidGithubRef::Owner);
        }

        let repo_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.');
        if repo.is_empty() || !repo.chars().all(repo_char) {
            return Err(InvalidGithubRef::Repo);
        }

        if !number.bytes().all(|b| b.is_ascii_digit()) {
            return Err(InvalidGithubRef::Number);
        }

        let number = number
            .parse::<NonZeroU64>()
            .map_err(|_| InvalidGithubRef::Number)?;

        Ok(Self {
            owner: owner.into(),
            repo: repo.into(),
            number,
        })
    }

    /// The account or organisation that owns the repository.
    #[must_use]
    pub const fn owner(&self) -> &str {
        self.owner.as_str()
    }

    /// The repository's name.
    #[must_use]
    pub const fn repo(&self) -> &str {
        self.repo.as_str()
    }

    /// The issue or pull request number.
    #[must_use]
    pub const fn number(&self) -> NonZeroU64 {
        self.number
    }
}

impl fmt::Display for GithubRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "gh:{}/{}#{}", self.owner, self.repo, self.number)
    }
}

/// Whether a reference is to an issue or a pull request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RefKind {
    /// `issue gh:…`
    Issue,

    /// `pr gh:…`
    PullRequest,
}

impl RefKind {
    /// The keyword that introduces it.
    #[must_use]
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Issue => "issue",
            Self::PullRequest => "pr",
        }
    }
}

/// An `issue` or `pr` line: what it points at, and which kind it is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ref {
    /// Issue or pull request.
    pub kind: RefKind,

    /// Where it is.
    pub target: GithubRef,
}

impl Ref {
    /// Its page on GitHub.
    ///
    /// ```
    /// use dagenda_core::reference::{GithubRef, Ref, RefKind};
    ///
    /// let target = GithubRef::new("gh:owner/repo#7")?;
    /// let reference = Ref { kind: RefKind::PullRequest, target };
    /// assert_eq!(reference.url(), "https://github.com/owner/repo/pull/7");
    /// # Ok::<(), dagenda_core::reference::InvalidGithubRef>(())
    /// ```
    #[must_use]
    pub fn url(&self) -> String {
        let section = match self.kind {
            RefKind::Issue => "issues",
            RefKind::PullRequest => "pull",
        };
        format!(
            "https://github.com/{}/{}/{section}/{}",
            self.target.owner, self.target.repo, self.target.number
        )
    }
}

impl fmt::Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.kind.keyword(), self.target)
    }
}

/// Why a string isn't a valid [`GithubRef`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum InvalidGithubRef {
    /// It doesn't start with `gh:`.
    #[error("expected a GitHub reference like `gh:owner/repo#123`")]
    Scheme,

    /// The owner is missing, or has characters GitHub doesn't allow.
    #[error(
        "expected a GitHub owner (letters, digits, `-`) after `gh:`, as in `gh:owner/repo#123`"
    )]
    Owner,

    /// No `/repo`, or one with characters GitHub doesn't allow.
    #[error("expected `owner/repo` after `gh:`, as in `gh:owner/repo#123`")]
    Repo,

    /// No `#` and number, or not a positive number.
    #[error("expected `#` and a number after the repository, as in `gh:owner/repo#123`")]
    Number,
}
