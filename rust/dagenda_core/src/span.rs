//! Byte spans into source text, and the nodes that carry them.

/// A byte range in source text, `start..end`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    /// Inclusive start offset.
    pub start: usize,

    /// Exclusive end offset.
    pub end: usize,
}

impl Span {
    /// The span `start..end`.
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// An empty span at `offset`.
    #[must_use]
    pub const fn at(offset: usize) -> Self {
        Self::new(offset, offset)
    }

    /// The 1-based line and column (in characters) where this span starts.
    ///
    /// ```
    /// use dagenda_core::span::Span;
    ///
    /// let source = "roadmap\ntrack x [X]";
    /// assert_eq!(Span::new(14, 15).line_col(source), (2, 7));
    /// ```
    #[must_use]
    pub fn line_col(self, source: &str) -> (usize, usize) {
        let before = source.get(..self.start).unwrap_or(source);
        let line_start = before.rfind('\n').map_or(0, |i| i + 1);
        let column = before.get(line_start..).unwrap_or_default().chars().count();
        (before.matches('\n').count() + 1, column + 1)
    }
}

/// A node and where it came from in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spanned<T> {
    /// The node.
    pub node: T,

    /// Its place in the source.
    pub span: Span,
}

impl<T> Spanned<T> {
    /// Wrap `node`, found at `span`.
    #[must_use]
    pub const fn new(node: T, span: Span) -> Self {
        Self { node, span }
    }
}
