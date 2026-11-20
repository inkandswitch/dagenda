//! People, written `@handle`.

use alloc::{string::String, vec::Vec};
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

/// One or more people, in the order they're listed: who's responsible for a
/// task or a track, written `@alice, @bob`.
///
/// ```
/// use dagenda_core::handle::{Handle, Handles};
///
/// let handles = Handles::new(vec![Handle::new("@alice")?, Handle::new("@bob")?]).expect("two");
/// assert_eq!(handles.to_string(), "@alice, @bob");
/// assert!(Handles::new(Vec::new()).is_none());
/// # Ok::<(), dagenda_core::handle::InvalidHandle>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Handles(Vec<Handle>);

impl Handles {
    /// `handles`, if there are any.
    #[must_use]
    pub fn new(handles: Vec<Handle>) -> Option<Self> {
        (!handles.is_empty()).then_some(Self(handles))
    }

    /// Each one, in the order listed.
    pub fn iter(&self) -> core::slice::Iter<'_, Handle> {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a Handles {
    type Item = &'a Handle;
    type IntoIter = core::slice::Iter<'a, Handle>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl fmt::Display for Handles {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, handle) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{handle}")?;
        }
        Ok(())
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
