//! The plan as text, for terminals, READMEs, and CI logs: each milestone
//! with its progress and what's next, then each track's tasks with their
//! status boxes and what they wait on.
//!
//! ```text
//! It parses
//!
//! Milestones
//!   ◇ itParses · It parses
//!     ▰▰▰▰▰▱▱▱▱▱ 1 of 2 · next: resolver
//!
//! Core
//!   [x] parser · Parser
//!   [ ] resolver · Resolver
//! ```

use crate::{
    id::Id,
    plan::{DocumentId, Plan, Readiness, Task},
    text::Text,
};
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;

impl Plan {
    /// Write the plan as text.
    ///
    /// ```
    /// let plan = dagenda_core::parse(
    ///     "roadmap
    ///      [x] parser [Parser]
    ///      [ ] resolver [Resolver]
    ///        needs parser",
    /// )?;
    /// assert!(plan.to_text().contains("[ ] resolver · Resolver"));
    /// # Ok::<(), dagenda_core::Error>(())
    /// ```
    #[must_use]
    pub fn to_text(&self) -> String {
        Outline(self).to_string()
    }
}

/// How many cells a progress bar has.
const BAR: usize = 10;

/// How many steps of a critical path to show before eliding the rest.
const NEXT: usize = 3;

/// A plan, written as text.
struct Outline<'a>(&'a Plan);

impl Outline<'_> {
    fn milestones(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let plan = self.0;
        if plan.milestones().is_empty() {
            return Ok(());
        }

        writeln!(f, "Milestones")?;
        for milestone in plan.milestones() {
            let Some(progress) = plan.progress(&milestone.id) else {
                continue;
            };
            let gate = if progress.is_reached() { '◆' } else { '◇' };
            write!(
                f,
                "  {gate} {}",
                Named(&milestone.id, milestone.label.as_ref())
            )?;
            if let Some(target) = &milestone.target {
                write!(f, " · {}", target.node)?;
            }
            writeln!(f)?;

            // The bar is all the work behind it; then its own must-haves
            let (done, total) = (progress.work.done, progress.work.total);
            let filled = (done * BAR).checked_div(total).unwrap_or(BAR);
            let bar: String = core::iter::repeat_n('▰', filled)
                .chain(core::iter::repeat_n('▱', BAR - filled.min(BAR)))
                .collect();
            write!(
                f,
                "    {bar} {done} of {total} · {} of {} direct",
                progress.needs.done, progress.needs.total
            )?;
            if progress.is_reached() {
                write!(f, " · reached")?;
            } else {
                let path = plan.critical_path(&milestone.id);
                if !path.is_empty() {
                    write!(f, " · next: {}", Path(&path))?;
                }
            }
            writeln!(f)?;
        }
        writeln!(f)
    }

    fn tracks(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let plan = self.0;
        let mut stack: Vec<_> = plan
            .tracks()
            .iter()
            .filter(|track| track.parent.is_none())
            .rev()
            .map(|track| (track, 0_usize))
            .collect();

        let mut shown = 0;
        while let Some((track, depth)) = stack.pop() {
            shown += 1;
            if shown > plan.tracks().len() {
                break;
            }
            let label = track.label.as_ref().map_or(track.id.as_str(), Text::as_str);
            writeln!(f, "{}{label}", Indent(depth))?;
            for task in plan
                .tasks()
                .iter()
                .filter(|task| task.track.as_ref() == Some(&track.id))
            {
                self.task(f, task, depth + 1)?;
            }

            let children: Vec<_> = plan
                .tracks()
                .iter()
                .filter(|child| child.parent.as_ref() == Some(&track.id))
                .collect();
            if children.is_empty() {
                writeln!(f)?;
            }
            stack.extend(children.into_iter().rev().map(|child| (child, depth + 1)));
        }

        let untracked: Vec<_> = plan
            .tasks()
            .iter()
            .filter(|task| task.track.is_none())
            .collect();
        if !untracked.is_empty() {
            writeln!(f, "Other tasks")?;
            for task in untracked {
                self.task(f, task, 1)?;
            }
            writeln!(f)?;
        }
        Ok(())
    }

    /// A task, and what it waits on.
    fn task(&self, f: &mut fmt::Formatter<'_>, task: &Task, depth: usize) -> fmt::Result {
        let plan = self.0;
        write!(
            f,
            "{}{} {}",
            Indent(depth),
            task.status,
            Named(&task.id, task.label.as_ref())
        )?;
        if let Some(Readiness::Blocked(by)) = plan.readiness(&task.id) {
            let waits: Vec<_> = by.iter().collect();
            write!(f, " · waits on {}", List(&waits))?;
        }
        writeln!(f)
    }
}

impl fmt::Display for Outline<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(title) = self.0.title(DocumentId::new(0)) {
            writeln!(f, "{title}")?;
            writeln!(f)?;
        }
        self.milestones(f)?;
        self.tracks(f)
    }
}

/// `id · Label`, or just `id`.
struct Named<'a>(&'a Id, Option<&'a Text>);

impl fmt::Display for Named<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.1 {
            Some(label) => write!(f, "{} · {label}", self.0),
            None => write!(f, "{}", self.0),
        }
    }
}

/// Two spaces per level.
struct Indent(usize);

impl fmt::Display for Indent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for _ in 0..self.0 {
            f.write_str("  ")?;
        }
        Ok(())
    }
}

/// `a, b, c`
struct List<'a>(&'a [&'a Id]);

impl fmt::Display for List<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, id) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{id}")?;
        }
        Ok(())
    }
}

/// `a → b → c`, eliding all but the first few steps.
struct Path<'a>(&'a [&'a Id]);

impl fmt::Display for Path<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, id) in self.0.iter().take(NEXT).enumerate() {
            if index > 0 {
                f.write_str(" → ")?;
            }
            write!(f, "{id}")?;
        }
        if self.0.len() > NEXT {
            write!(f, " → … ({} more)", self.0.len() - NEXT)?;
        }
        Ok(())
    }
}
