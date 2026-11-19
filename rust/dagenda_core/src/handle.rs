//! People, written `@handle`.

use alloc::string::String;
use core::fmt;
use thiserror::Error;

/// A person, written `@handle`: letters, digits, and `-`, like a GitHub
/// login.
///
/// ```
/// use dagenda_core::handle::Handle;
///
/// let handle = Handle::new("@alice")?;
/// assert_eq!(handle.name(), "alice");
/// assert_eq!(handle.to_string(), "@alice");
/// # Ok::<(), dagenda_core::handle::InvalidHandle>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Handle(String);

impl Handle {
    /// Parse a handle as written, `@` included.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidHandle`] if `written` doesn't start with `@`, has
    /// nothing after it, or has anything but letters, digits, and `-`.
    pub fn new(written: &str) -> Result<Self, InvalidHandle> {
        let name = written.strip_prefix('@').ok_or(InvalidHandle::MissingAt)?;
        if name.is_empty() {
            return Err(InvalidHandle::Empty);
        }

        if let Some(c) = name
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || *c == '-'))
        {
            return Err(InvalidHandle::Char(c));
        }

        Ok(Self(name.into()))
    }

    /// The name, without the `@`.
    #[must_use]
    pub const fn name(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}", self.0)
    }
}

/// Why a string isn't a valid [`Handle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum InvalidHandle {
    /// No `@` at the start.
    #[error("a person is written with `@`, as in `@alice`")]
    MissingAt,

    /// Nothing after the `@`.
    #[error("expected a name after `@`")]
    Empty,

    /// A character outside letters, digits, and `-`.
    #[error("a handle can't contain {0:?}; use letters, digits, or `-`")]
    Char(char),
}
