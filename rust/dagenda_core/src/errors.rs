//! Several errors at once: everything wrong with a document, or with a
//! plan, found in one pass, so one run reports them all instead of one fix
//! at a time.

use alloc::vec::{self, Vec};
use core::{
    error::Error,
    fmt,
    iter::{self, Chain, Once},
    num::NonZeroUsize,
};

/// One or more errors, in source order. Never empty: there's always a
/// [`first`](Self::first).
///
/// ```
/// let source = "roadmap\n[ ] a\n  needs x\n[ ] b\n  needs y\n";
/// let Err(dagenda_core::Error::Resolve(errors)) = dagenda_core::parse(source) else {
///     panic!("two unknown ids");
/// };
/// let messages: Vec<String> = errors.iter().map(ToString::to_string).collect();
/// assert_eq!(
///     messages,
///     ["nothing in the plan is called `x`", "nothing in the plan is called `y`"]
/// );
/// assert_eq!(
///     errors.to_string(),
///     "nothing in the plan is called `x` (and 1 more problem)"
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Errors<E> {
    first: E,
    rest: Vec<E>,
}

impl<E> Errors<E> {
    /// Just `error`.
    #[must_use]
    pub const fn one(error: E) -> Self {
        Self {
            first: error,
            rest: Vec::new(),
        }
    }

    /// `errors`, if there are any.
    #[must_use]
    pub fn new(errors: Vec<E>) -> Option<Self> {
        let mut errors = errors.into_iter();
        let first = errors.next()?;
        Some(Self {
            first,
            rest: errors.collect(),
        })
    }

    /// The first, in source order.
    #[must_use]
    pub const fn first(&self) -> &E {
        &self.first
    }

    /// How many there are.
    #[must_use]
    pub const fn len(&self) -> NonZeroUsize {
        NonZeroUsize::MIN.saturating_add(self.rest.len())
    }

    /// Each one, in source order.
    pub fn iter(&self) -> Chain<Once<&E>, core::slice::Iter<'_, E>> {
        iter::once(&self.first).chain(&self.rest)
    }
}

impl<E> IntoIterator for Errors<E> {
    type Item = E;
    type IntoIter = Chain<Once<E>, vec::IntoIter<E>>;

    fn into_iter(self) -> Self::IntoIter {
        iter::once(self.first).chain(self.rest)
    }
}

impl<'a, E> IntoIterator for &'a Errors<E> {
    type Item = &'a E;
    type IntoIter = Chain<Once<&'a E>, core::slice::Iter<'a, E>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// The first, and how many more there are.
impl<E: fmt::Display> fmt::Display for Errors<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.first)?;
        match self.rest.len() {
            0 => Ok(()),
            1 => f.write_str(" (and 1 more problem)"),
            more => write!(f, " (and {more} more problems)"),
        }
    }
}

impl<E: Error> Error for Errors<E> {}
