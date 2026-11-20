//! People's pictures, for the stickers on their cards.

use crate::reference::{InvalidUrl, Url};
use alloc::{borrow::Cow, format, string::String};
use core::fmt;
use thiserror::Error;

/// Where a person's picture is: their GitHub account's, written
/// `gh:<login>`, or any web address.
///
/// ```
/// use dagenda_core::avatar::Avatar;
///
/// let github = Avatar::new("gh:octocat")?;
/// assert_eq!(github.src(), "https://github.com/octocat.png?size=96");
/// assert_eq!(github.to_string(), "gh:octocat");
///
/// let elsewhere = Avatar::new("https://example.com/me.png")?;
/// assert_eq!(elsewhere.src(), "https://example.com/me.png");
/// # Ok::<(), dagenda_core::avatar::InvalidAvatar>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Avatar(Source);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Source {
    /// A GitHub login.
    Github(String),

    /// A picture's address.
    Url(Url),
}

impl Avatar {
    /// Parse an avatar as written.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidAvatar`] if `written` is neither `gh:` and a GitHub
    /// login (letters, digits, and `-`) nor a valid web address.
    pub fn new(written: &str) -> Result<Self, InvalidAvatar> {
        if let Some(login) = written.strip_prefix("gh:") {
            if login.is_empty() || !login.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                return Err(InvalidAvatar::Login);
            }
            return Ok(Self(Source::Github(login.into())));
        }

        if !(written.starts_with("https://") || written.starts_with("http://")) {
            return Err(InvalidAvatar::Scheme);
        }
        Ok(Self(Source::Url(Url::new(written)?)))
    }

    /// The picture's address.
    #[must_use]
    pub fn src(&self) -> Cow<'_, str> {
        match &self.0 {
            Source::Github(login) => Cow::Owned(format!("https://github.com/{login}.png?size=96")),
            Source::Url(url) => Cow::Borrowed(url.as_str()),
        }
    }
}

/// As written.
impl fmt::Display for Avatar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Source::Github(login) => write!(f, "gh:{login}"),
            Source::Url(url) => write!(f, "{url}"),
        }
    }
}

/// Why a string isn't a valid [`Avatar`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum InvalidAvatar {
    /// Neither `gh:` nor a web address.
    #[error("expected `gh:` and a GitHub login, as in `avatar gh:octocat`, or a picture's address")]
    Scheme,

    /// No login after `gh:`, or one with characters GitHub doesn't allow.
    #[error("expected a GitHub login (letters, digits, `-`) after `gh:`, as in `gh:octocat`")]
    Login,

    /// A web address that isn't valid.
    #[error(transparent)]
    Url(#[from] InvalidUrl),
}
