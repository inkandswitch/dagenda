//! References to GitHub issues and pull requests, and links to anything
//! else: a comment, a design doc, a demo.

use crate::text::Text;
use alloc::{
    format,
    string::{String, ToString},
};
use core::{fmt, num::NonZeroU64};
use thiserror::Error;

/// A GitHub issue or pull request, written `gh:owner/repo#123`.
///
/// ```
/// use dagenda_core::reference::GithubRef;
///
/// let reference = GithubRef::new("gh:owner/repo#365")?;
/// assert_eq!(reference.owner(), "owner");
/// assert_eq!(reference.repo(), "repo");
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

impl Ref {
    /// What to call it on a card: `PR #7`, or `#7` for an issue.
    ///
    /// ```
    /// use dagenda_core::reference::{GithubRef, Ref, RefKind};
    ///
    /// let target = GithubRef::new("gh:owner/repo#7")?;
    /// assert_eq!(Ref { kind: RefKind::PullRequest, target: target.clone() }.short(), "PR #7");
    /// assert_eq!(Ref { kind: RefKind::Issue, target }.short(), "#7");
    /// # Ok::<(), dagenda_core::reference::InvalidGithubRef>(())
    /// ```
    #[must_use]
    pub fn short(&self) -> String {
        match self.kind {
            RefKind::Issue => format!("#{}", self.target.number),
            RefKind::PullRequest => format!("PR #{}", self.target.number),
        }
    }
}

impl fmt::Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.kind.keyword(), self.target)
    }
}

/// A `link` line: any page about the work (a GitHub comment, a design doc,
/// a demo), with a label if its short name won't do.
///
/// ```
/// use dagenda_core::reference::{Link, Url};
///
/// let url = Url::new("https://github.com/owner/repo/issues/12#issuecomment-345")?;
/// assert_eq!(Link { url, label: None }.name(), "#12 comment");
/// # Ok::<(), dagenda_core::reference::InvalidUrl>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Link {
    /// Where it goes.
    pub url: Url,

    /// What to call it, if not its short name.
    pub label: Option<Text>,
}

impl Link {
    /// What to call it: its label, or its address's short name.
    #[must_use]
    pub fn name(&self) -> String {
        self.label
            .as_ref()
            .map_or_else(|| self.url.short(), ToString::to_string)
    }
}

impl fmt::Display for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "link {}", self.url)?;
        if let Some(label) = &self.label {
            f.write_str(" [")?;
            for c in label.as_str().chars() {
                if matches!(c, ']' | '\\') {
                    f.write_str("\\")?;
                }
                fmt::Write::write_char(f, c)?;
            }
            f.write_str("]")?;
        }
        Ok(())
    }
}

/// A web address: `https://` (or `http://`), a host, and no spaces.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Url(String);

impl Url {
    /// Parse an address as written.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidUrl`] if it doesn't start with `https://` or
    /// `http://`, has no host, or has a space, a quote, or an angle bracket.
    pub fn new(written: &str) -> Result<Self, InvalidUrl> {
        let rest = written
            .strip_prefix("https://")
            .or_else(|| written.strip_prefix("http://"))
            .ok_or(InvalidUrl::Scheme)?;
        if let Some(c) = written
            .chars()
            .find(|&c| c.is_whitespace() || c.is_control() || matches!(c, '"' | '<' | '>'))
        {
            return Err(InvalidUrl::Char(c));
        }
        let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
        if host.is_empty() {
            return Err(InvalidUrl::Host);
        }
        Ok(Self(written.into()))
    }

    /// The address as written.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// What to call it on a card. On GitHub, the issue, pull request, or
    /// discussion it points at (`#12`, `PR #7`, `discussion #3`), and
    /// whether it's a comment or review on one; elsewhere, its host.
    ///
    /// ```
    /// use dagenda_core::reference::Url;
    ///
    /// let short = |url| Url::new(url).map(|url| url.short());
    /// assert_eq!(short("https://github.com/o/r/pull/7#pullrequestreview-9")?, "PR #7 review");
    /// assert_eq!(short("https://github.com/o/r")?, "o/r");
    /// assert_eq!(short("https://www.example.com/design?v=2")?, "example.com");
    /// # Ok::<(), dagenda_core::reference::InvalidUrl>(())
    /// ```
    #[must_use]
    pub fn short(&self) -> String {
        let rest = self
            .0
            .split_once("://")
            .map_or(self.0.as_str(), |(_, rest)| rest);
        let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
        let host = host.split(['?', '#']).next().unwrap_or(host);
        let host = host.strip_prefix("www.").unwrap_or(host);
        if host != "github.com" {
            return host.into();
        }

        let (path, fragment) = path.split_once('#').unwrap_or((path, ""));
        let path = path.split('?').next().unwrap_or(path);
        let parts: alloc::vec::Vec<&str> =
            path.split('/').filter(|part| !part.is_empty()).collect();
        let what = if ["issuecomment-", "discussion_r", "discussioncomment-"]
            .iter()
            .any(|prefix| fragment.starts_with(prefix))
        {
            " comment"
        } else if fragment.starts_with("pullrequestreview-") {
            " review"
        } else {
            ""
        };
        match parts.as_slice() {
            [owner, repo, kind, number, ..] if number.bytes().all(|b| b.is_ascii_digit()) => {
                match *kind {
                    "pull" => format!("PR #{number}{what}"),
                    "issues" => format!("#{number}{what}"),
                    "discussions" => format!("discussion #{number}{what}"),
                    _ => format!("{owner}/{repo}"),
                }
            }
            [owner, repo, ..] => format!("{owner}/{repo}"),
            _ => String::from("GitHub"),
        }
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
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

/// Why a string isn't a valid [`Url`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum InvalidUrl {
    /// It doesn't start with `https://` or `http://`.
    #[error("expected a web address starting with `https://`")]
    Scheme,

    /// Nothing between the `//` and the path.
    #[error("expected a host after `https://`, as in `https://example.com`")]
    Host,

    /// A character that can't go in an address unescaped.
    #[error("a web address can't contain {0:?}; percent-encode it")]
    Char(char),
}
