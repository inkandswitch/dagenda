//! Property tests for the parser and the resolver.

mod common;

use bolero::check;
use dagenda_core::{
    plan::{Plan, Readiness},
    span::Span,
    status::Status,
    tech_tree::{Edge, Node, TechTree},
};
use std::{fs, path::Path};
use testresult::TestResult;

/// Arbitrary text never panics, and every span lies within it, on character
/// boundaries.
#[test]
fn never_panics_and_spans_are_in_bounds() {
    check!()
        .with_type::<String>()
        .for_each(|source| match dagenda_core::parse(source) {
            Ok(plan) => assert_consistent(&plan, source),
            Err(error) => assert_in_bounds(error.span(), source),
        });
}

/// Plans built from the notation's own lines, at random indents, either fail
/// with an error inside the source or resolve to something consistent.
#[test]
fn plans_are_consistent() {
    check!()
        .with_type::<Vec<(u8, u8, u8, u8)>>()
        .for_each(|lines| {
            let source = build(lines);
            match dagenda_core::parse(&source) {
                Ok(plan) => assert_consistent(&plan, &source),
                Err(error) => assert_in_bounds(error.span(), &source),
            }
        });
}

/// How far a block is indented doesn't matter, only that it's further than
/// its parent.
#[test]
fn indentation_is_relative() -> TestResult {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_none_or(|ext| ext != "dagenda") {
            continue;
        }

        let source = fs::read_to_string(&path)?;
        let expected = common::report(&source, false);
        for factor in 2..=4 {
            assert_eq!(
                common::report(&reindent(&source, factor), false),
                expected,
                "{} indented ×{factor}",
                path.display()
            );
        }
    }
    Ok(())
}

const ANY: [&str; 6] = ["a", "b", "c", "d", "m", "n"];
const BOXES: [char; 8] = [' ', '>', 'x', '-', '?', '!', '*', '~'];
const MILESTONES: [&str; 2] = ["m", "n"];
const TASKS: [&str; 4] = ["a", "b", "c", "d"];
const TRACKS: [&str; 2] = ["s", "t"];

/// A source from the notation's own lines: each tuple picks an indent, a
/// kind of line, and what goes in it.
fn build(lines: &[(u8, u8, u8, u8)]) -> String {
    let mut source = String::from("roadmap\n");
    for &(indent, kind, first, second) in lines {
        let line = match kind % 8 {
            0 => {
                let mark = BOXES
                    .get(usize::from(second) % BOXES.len())
                    .copied()
                    .unwrap_or(' ');
                format!("[{mark}] task {}", pick(&TASKS, first))
            }
            1 => format!("needs {}, {}", pick(&ANY, first), pick(&ANY, second)),
            2 => format!("milestone {}", pick(&MILESTONES, first)),
            3 => pick(&ANY, first).to_owned(),
            4 => String::from("---"),
            5 => format!("track {}", pick(&TRACKS, first)),
            6 => format!("extend {}", pick(&TASKS, first)),
            _ => String::from("note something"),
        };
        source.push_str(&" ".repeat(usize::from(indent % 4) * 2));
        source.push_str(&line);
        source.push('\n');
    }
    source
}

fn pick<'a>(items: &[&'a str], index: u8) -> &'a str {
    items
        .get(usize::from(index) % items.len())
        .copied()
        .unwrap_or_default()
}

/// Every line's indent, times `factor`.
fn reindent(source: &str, factor: usize) -> String {
    source
        .lines()
        .map(|line| {
            let text = line.trim_start_matches(' ');
            let indent = line.len() - text.len();
            format!("{}{text}", " ".repeat(indent * factor))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_in_bounds(span: Span, source: &str) {
    assert!(
        span.start <= span.end && span.end <= source.len(),
        "{span:?} outside {} bytes",
        source.len()
    );
    assert!(
        source.is_char_boundary(span.start) && source.is_char_boundary(span.end),
        "{span:?} splits a character"
    );
}

/// The tech tree fits: every card inside the canvas and inside its branch,
/// and no two cards overlapping. Every line runs in right angles from the
/// top of its prerequisite to the bottom of what waits on it, and never
/// passes behind any other card. The bands stack, and (with no subtasks
/// folded in to make loops) every must-have line points up and everything
/// a milestone needs sits under its line.
fn assert_laid_out(plan: &Plan) {
    let tree = TechTree::new(plan);
    for node in &tree.nodes {
        assert!(node.x + node.width <= tree.width && node.y + node.height <= tree.height);
        let branch = tree.branches.get(node.branch);
        assert!(
            branch.is_some_and(|branch| branch.x <= node.x
                && node.x + node.width <= branch.x + branch.width
                && branch.y <= node.y
                && node.y + node.height <= branch.y + branch.height),
            "{} is outside its branch",
            node.id
        );
    }

    for (index, a) in tree.nodes.iter().enumerate() {
        for b in tree.nodes.iter().skip(index + 1) {
            let apart = a.x + a.width <= b.x
                || b.x + b.width <= a.x
                || a.y + a.height <= b.y
                || b.y + b.height <= a.y;
            assert!(apart, "{} overlaps {}", a.id, b.id);
        }
    }

    let flat = plan.tasks().iter().all(|task| task.parent.is_none());
    for edge in &tree.edges {
        assert_ne!(edge.from, edge.to);
        let (from, to) = (tree.nodes.get(edge.from), tree.nodes.get(edge.to));
        assert!(from.is_some() && to.is_some(), "a line to a missing node");
        let (Some(from), Some(to)) = (from, to) else {
            continue;
        };
        if flat && !edge.wanted {
            assert!(from.row < to.row, "{} → {} points down", from.id, to.id);
        }
        assert_routed(&tree, edge, from, to);
    }

    for pair in tree.bands.windows(2) {
        let [below, above] = pair else { continue };
        assert_eq!(below.top, above.bottom, "bands stack without gaps");
    }
    for band in tree.bands.iter().filter(|_| flat) {
        let Some(gate) = band.milestone.and_then(|gate| tree.nodes.get(gate)) else {
            continue;
        };
        for member in plan.milestone(&gate.id).into_iter().flat_map(|m| &m.needs) {
            let card = tree.nodes.iter().find(|node| node.id == member.node);
            assert!(
                card.is_some_and(|card| card.y > band.top),
                "{} sits above {}'s line",
                member.node,
                gate.id
            );
        }
    }

    let svg = plan.to_svg();
    assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
    for node in &tree.nodes {
        assert!(svg.contains(&format!("data-id=\"{}\"", node.id)));
    }
    assert!(!plan.to_text().is_empty() || plan.tasks().is_empty());
}

fn assert_routed(tree: &TechTree, edge: &Edge, from: &Node, to: &Node) {
    let route = &edge.route;
    assert_eq!(
        route.first().map(|p| (p.x, p.y)),
        Some((from.middle(), from.y)),
        "{} → {} starts at the top of {}",
        from.id,
        to.id,
        from.id
    );
    assert_eq!(
        route.last().map(|p| (p.x, p.y)),
        Some((to.middle(), to.entry())),
        "{} → {} ends at the bottom of {}",
        from.id,
        to.id,
        to.id
    );

    for pair in route.windows(2) {
        let [a, b] = pair else { continue };
        assert!(
            a.x == b.x || a.y == b.y,
            "{} → {} turns a corner off the grid",
            from.id,
            to.id
        );
        assert!(a.x <= tree.width && b.x <= tree.width && a.y <= tree.height && b.y <= tree.height);

        let (left, right) = (a.x.min(b.x), a.x.max(b.x));
        let (top, bottom) = (a.y.min(b.y), a.y.max(b.y));
        for card in tree
            .nodes
            .iter()
            .filter(|card| card.id != from.id && card.id != to.id)
        {
            let behind = left < card.x + card.width
                && card.x < right.max(left + 1)
                && top < card.y + card.height
                && card.y < bottom.max(top + 1);
            assert!(!behind, "{} → {} passes behind {}", from.id, to.id, card.id);
        }
    }
}

/// What's derived agrees with itself: readiness with statuses and needs,
/// progress with what's finished, critical paths with what isn't.
fn assert_consistent(plan: &Plan, source: &str) {
    assert_laid_out(plan);

    for warning in plan.warnings() {
        assert_in_bounds(warning.at.span, source);
    }

    for task in plan.tasks() {
        assert_in_bounds(task.defined.span, source);
        let status_box = source.get(task.status_at.span.start..task.status_at.span.end);
        assert_eq!(
            status_box.map(ToString::to_string),
            Some(task.status.to_string())
        );

        let readiness = plan.readiness(&task.id);
        assert!(readiness.is_some(), "{} has no readiness", task.id);
        match readiness {
            Some(Readiness::Finished) => assert_eq!(task.status, Status::Done),
            Some(Readiness::Dropped) => assert_eq!(task.status, Status::Dropped),
            Some(Readiness::Ready) => {
                assert!(task.needs.iter().all(|need| plan.is_finished(&need.node)));
            }
            Some(Readiness::Blocked(by)) => {
                assert!(!by.is_empty());
                assert!(by.iter().all(|need| !plan.is_finished(need)));
            }
            None => {}
        }

        let path = plan.critical_path(&task.id);
        if task.status.is_finished() {
            assert!(
                path.is_empty(),
                "{} is done, but waits on {path:?}",
                task.id
            );
        } else {
            assert_eq!(
                path.last(),
                Some(&&task.id),
                "{}'s chain ends with it",
                task.id
            );
        }
        assert!(path.iter().all(|id| !plan.is_finished(id)));
    }

    for milestone in plan.milestones() {
        let progress = plan.progress(&milestone.id);
        assert!(progress.is_some_and(|progress| {
            progress.needs.done <= progress.needs.total
                && progress.wants.done <= progress.wants.total
                && progress.is_reached() == plan.is_finished(&milestone.id)
        }));

        let path = plan.critical_path(&milestone.id);
        assert!(path.iter().all(|id| !plan.is_finished(id)));
        assert_eq!(path.is_empty(), plan.is_finished(&milestone.id));
    }
}
