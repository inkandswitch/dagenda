//! Check documents as one plan, reporting every problem by document, line,
//! and column: `name:line:column: severity: message`, the shape editors and
//! CI annotate. The CLI and the Wasm bindings both report through this.

use crate::{
    parser,
    plan::{Location, Plan, ResolveErrorKind},
    span::Span,
};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;

/// A document to check: what to call it, and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// What to call it in a problem: a path, `<stdin>`, a tab's name.
    pub name: String,

    /// Its text.
    pub text: String,
}

/// Check documents as one plan, so they can refer to each other's ids.
///
/// Every error in every document that doesn't parse is reported (the first
/// in each malformed top-level block). If they all parse, the plan is
/// resolved: every resolve error is reported, or else every warning, along
/// with the plan.
///
/// ```
/// use dagenda_core::check::{Source, check};
///
/// let source = Source {
///     name: "plan.dagenda".into(),
///     text: "roadmap\n[ ] x\n  needs y\n".into(),
/// };
/// let (plan, problems) = check(&[source]);
/// assert!(plan.is_none());
/// assert_eq!(problems[0].to_string(), "plan.dagenda:3:9: error: nothing in the plan is called `y`");
/// ```
#[must_use]
pub fn check(sources: &[Source]) -> (Option<Plan>, Vec<Problem>) {
    let mut documents = Vec::with_capacity(sources.len());
    let mut problems = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        match parser::document(&source.text) {
            Ok(document) => documents.push(document),
            Err(errors) => problems.extend(errors.iter().map(|error| {
                Problem::new(
                    index,
                    source,
                    error.span(),
                    Severity::Error,
                    error.to_string(),
                )
            })),
        }
    }
    if !problems.is_empty() {
        return (None, problems);
    }

    match Plan::resolve(&documents) {
        Ok(plan) => {
            problems.extend(plan.warnings().iter().filter_map(|warning| {
                Problem::at(sources, warning.at, Severity::Warning, warning.to_string())
            }));
            (Some(plan), problems)
        }
        Err(errors) => {
            problems.extend(errors.iter().filter_map(|error| {
                let mut message = error.to_string();
                if let Some(first) = first_location(error.kind())
                    && let Some(source) = sources.get(first.document.index())
                {
                    // Name the document only when it's another one
                    let (line, column) = first.span.line_col(&source.text);
                    message = if first.document == error.location().document {
                        format!("{message} (first at {line}:{column})")
                    } else {
                        format!("{message} (first at {}:{line}:{column})", source.name)
                    };
                }
                Problem::at(sources, error.location(), Severity::Error, message)
            }));
            (None, problems)
        }
    }
}

/// A problem at a place in a named source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// Which of the sources it's in, from 0.
    pub document: usize,

    /// That source's name.
    pub name: String,

    /// 1-based line.
    pub line: usize,

    /// 1-based column, in characters.
    pub column: usize,

    /// Whether it stops the plan resolving.
    pub severity: Severity,

    /// What's wrong.
    pub message: String,
}

impl Problem {
    /// A problem at `span` in `source`, the `document`th.
    #[must_use]
    pub fn new(
        document: usize,
        source: &Source,
        span: Span,
        severity: Severity,
        message: String,
    ) -> Self {
        let (line, column) = span.line_col(&source.text);
        Self {
            document,
            name: source.name.clone(),
            line,
            column,
            severity,
            message,
        }
    }

    /// A problem at `at`, in whichever of `sources` it points into.
    fn at(sources: &[Source], at: Location, severity: Severity, message: String) -> Option<Self> {
        let document = at.document.index();
        sources
            .get(document)
            .map(|source| Self::new(document, source, at.span, severity, message))
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}: {}",
            self.name, self.line, self.column, self.severity, self.message
        )
    }
}

/// How bad a problem is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Worth a look; the plan still resolves.
    Warning,

    /// The plan doesn't resolve.
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Warning => "warning",
            Self::Error => "error",
        })
    }
}

/// Where the other half of a two-place error is.
const fn first_location(kind: &ResolveErrorKind) -> Option<Location> {
    match kind {
        ResolveErrorKind::Duplicate { first, .. }
        | ResolveErrorKind::DuplicatePerson { first, .. }
        | ResolveErrorKind::Conflict { first, .. } => Some(*first),
        ResolveErrorKind::Unknown(_)
        | ResolveErrorKind::WrongKind { .. }
        | ResolveErrorKind::Cycle(_) => None,
    }
}
