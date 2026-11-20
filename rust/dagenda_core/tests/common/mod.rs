//! Shared test helpers.

use dagenda_core::{
    plan::{DocumentId, Located, Plan, Readiness, Task},
    text::{Note, Text},
};
use std::fmt;

/// Everything the core derives from a single document, as stable text: what
/// the goldens pin, and what re-indenting mustn't change. Unless `exact`, it
/// leaves out notes, whose own indentation is part of their content, and
/// warnings' columns.
pub(crate) fn report(source: &str, exact: bool) -> String {
    match dagenda_core::parse(source) {
        Ok(plan) => Report {
            plan: &plan,
            source,
            exact,
        }
        .to_string(),
        Err(error) => {
            let (line, column) = error.span().line_col(source);
            format!("error at {line}:{column}: {error}\n")
        }
    }
}

/// A plan's report, section by section.
struct Report<'a> {
    plan: &'a Plan,
    source: &'a str,
    exact: bool,
}

impl Report<'_> {
    fn tracks(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "\ntracks")?;
        for track in self.plan.tracks() {
            write!(f, "  {}{}", track.id, Label(track.label.as_ref()))?;
            if let Some(parent) = &track.parent {
                write!(f, ", in {parent}")?;
            }
            if let Some(dri) = &track.dri {
                write!(f, ", dri {}", dri.node)?;
            }
            writeln!(f)?;
            self.notes(f, &track.notes)?;
        }
        Ok(())
    }

    fn tasks(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "\ntasks")?;
        for task in self.plan.tasks() {
            write!(
                f,
                "  {} {}{}",
                task.status,
                task.id,
                Label(task.label.as_ref())
            )?;
            if let Some(track) = &task.track {
                write!(f, ", in {track}")?;
            }
            match self.plan.readiness(&task.id) {
                Some(Readiness::Finished) => writeln!(f, ": finished")?,
                Some(Readiness::Dropped) => writeln!(f, ": dropped")?,
                Some(Readiness::Ready) => writeln!(f, ": ready")?,
                Some(Readiness::Blocked(by)) => writeln!(f, ": blocked by {}", join(&by, ", "))?,
                None => writeln!(f, ": missing")?,
            }

            let details = details(task);
            if !details.is_empty() {
                writeln!(f, "    {}", details.join("; "))?;
            }
            self.notes(f, &task.notes)?;
        }
        Ok(())
    }

    fn milestones(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "\nmilestones")?;
        for milestone in self.plan.milestones() {
            let Some(progress) = self.plan.progress(&milestone.id) else {
                continue;
            };
            let reached = if progress.is_reached() {
                "reached"
            } else {
                "not reached"
            };
            write!(
                f,
                "  {}{}: {reached}; {}/{} needed, {}/{} wanted",
                milestone.id,
                Label(milestone.label.as_ref()),
                progress.needs.done,
                progress.needs.total,
                progress.wants.done,
                progress.wants.total,
            )?;
            if let Some(target) = &milestone.target {
                write!(f, ", target {}", target.node)?;
            }
            writeln!(f)?;

            let path = self.plan.critical_path(&milestone.id);
            if !path.is_empty() {
                writeln!(f, "    critical path: {}", join(&path, " → "))?;
            }
            self.notes(f, &milestone.notes)?;
        }
        Ok(())
    }

    fn warnings(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "\nwarnings")?;
        for warning in self.plan.warnings() {
            let (line, column) = warning.at.span.line_col(self.source);
            if self.exact {
                writeln!(f, "  {line}:{column}: {warning}")?;
            } else {
                writeln!(f, "  line {line}: {warning}")?;
            }
        }
        Ok(())
    }

    fn notes(&self, f: &mut fmt::Formatter<'_>, notes: &[Located<Note>]) -> fmt::Result {
        if !self.exact {
            return Ok(());
        }
        for note in notes {
            for line in note.node.as_str().lines() {
                if line.is_empty() {
                    writeln!(f, "    |")?;
                } else {
                    writeln!(f, "    | {line}")?;
                }
            }
        }
        Ok(())
    }
}

impl fmt::Display for Report<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(title) = self.plan.title(DocumentId::new(0)) {
            writeln!(f, "title: {title}")?;
        }
        self.tracks(f)?;
        self.tasks(f)?;
        self.milestones(f)?;
        self.warnings(f)
    }
}

/// A task's details, as written.
fn details(task: &Task) -> Vec<String> {
    let mut details = Vec::new();
    if !task.needs.is_empty() {
        let needs: Vec<_> = task.needs.iter().map(|need| &need.node).collect();
        details.push(format!("needs {}", join(&needs, ", ")));
    }
    if let Some(est) = &task.est {
        details.push(format!("est {}", est.node));
    }
    if let Some(target) = &task.target {
        details.push(format!("target {}", target.node));
    }
    if let Some(dri) = &task.dri {
        details.push(format!("dri {}", dri.node));
    }
    details.extend(
        task.refs
            .iter()
            .map(|reference| format!("{} ({})", reference.node, reference.node.url())),
    );
    details.extend(
        task.links
            .iter()
            .map(|link| format!("{} ({})", link.node, link.node.name())),
    );
    details
}

/// ` [label]`, or nothing.
struct Label<'a>(Option<&'a Text>);

impl fmt::Display for Label<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(label) => write!(f, " [{label}]"),
            None => Ok(()),
        }
    }
}

fn join<T: fmt::Display>(items: &[T], separator: &str) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(separator)
}
