//! Property tests for the parser and the resolver.

mod common;

use bolero::check;
use dagenda_core::{
    check::Source,
    colour::{Colour, Named, Rgb},
    date::Date,
    edit::{self, Edit, EditError, Setting},
    handle::{Handle, Handles},
    plan::{Plan, Readiness},
    span::Span,
    status::Status,
    tech_tree::{Edge, Node, Point, TechTree},
};
use std::{fmt::Write as _, fs, path::Path};
use testresult::TestResult;

/// Arbitrary text never panics, and every span lies within it, on character
/// boundaries.
#[test]
fn never_panics_and_spans_are_in_bounds() {
    check!()
        .with_type::<String>()
        .for_each(|source| match dagenda_core::parse(source) {
            Ok(plan) => assert_consistent(&plan, source),
            Err(error) => assert_errors_in_bounds(&error, source),
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
                Err(error) => assert_errors_in_bounds(&error, &source),
            }
        });
}

/// Any `#rrggbb`, in any case, reads back as the same colour, and prints in
/// lowercase; every palette name reads back as itself.
#[test]
fn colours_round_trip() {
    check!()
        .with_type::<([u8; 3], bool)>()
        .for_each(|&([red, green, blue], upper)| {
            let written = format!("#{red:02x}{green:02x}{blue:02x}");
            let written = if upper {
                written.to_uppercase()
            } else {
                written
            };
            let colour = Colour::new(&written);
            assert_eq!(
                colour,
                Ok(Colour::Rgb(Rgb::new(red, green, blue))),
                "{written}"
            );
            assert_eq!(
                colour.map(|colour| colour.to_string()),
                Ok(written.to_lowercase())
            );
        });
    for named in Named::ALL {
        assert_eq!(Colour::new(named.name()), Ok(Colour::Named(named)));
    }
}

/// A `dri` line lists anyone, in any order, however it's spaced after the
/// commas, and they come back in that order; with an `avatar` each, they
/// come back as stickers too.
#[test]
fn dri_lists_keep_their_order() {
    check!()
        .with_type::<(Vec<u8>, Vec<bool>)>()
        .for_each(|(people, spaced)| {
            let mut handles: Vec<String> = Vec::new();
            for person in people.iter().take(8) {
                let handle = format!("@p{person}");
                if !handles.contains(&handle) {
                    handles.push(handle);
                }
            }
            if handles.is_empty() {
                return;
            }
            let mut line = String::from("dri ");
            for (index, handle) in handles.iter().enumerate() {
                if index > 0 {
                    line.push(',');
                    if spaced.get(index).copied().unwrap_or(true) {
                        line.push(' ');
                    }
                }
                line.push_str(handle);
            }
            let mut avatars = String::new();
            for handle in &handles {
                let login = handle.trim_start_matches('@');
                writeln!(avatars, "{handle}\n  avatar gh:{login}").expect("writes to a string");
            }
            let source = format!("roadmap\n{avatars}[ ] a\n  {line}\n");
            let plan = dagenda_core::parse(&source).expect("a dri list parses");
            let dri = plan.tasks().first().and_then(|task| task.dri.as_ref());
            assert_eq!(
                dri.map(|dri| dri.node.to_string()),
                Some(handles.join(", ")),
                "{source}"
            );
            let tree = TechTree::new(&plan);
            let stuck: Vec<String> = tree
                .nodes
                .first()
                .map(|node| {
                    node.stickers
                        .iter()
                        .map(|sticker| sticker.handle.to_string())
                        .collect()
                })
                .unwrap_or_default();
            assert_eq!(stuck, handles);
        });
}

/// Well-formed plans (a DAG of up to 14 tasks over nested tracks, under up
/// to three milestones) lay out, with every line routed apart from the rest.
#[test]
fn tech_trees_route_every_line_apart() {
    check!()
        .with_type::<(Vec<(u8, u8, u16)>, Vec<(u16, u16)>)>()
        .for_each(|(tasks, milestones)| {
            let source = dag(tasks, milestones);
            let plan = dagenda_core::parse(&source);
            assert!(plan.is_ok(), "{source}\n{plan:?}");
            if let Ok(plan) = plan {
                assert_laid_out(&plan);
            }
        });
}

/// A well-formed plan: each task in one of three tracks (one nested in
/// another) or none, with some status, needing some of the tasks before it;
/// each milestone needing some tasks (and maybe the milestone before it),
/// and wanting some others.
fn dag(tasks: &[(u8, u8, u16)], milestones: &[(u16, u16)]) -> String {
    let tasks = tasks.get(..tasks.len().min(14)).unwrap_or_default();
    let ids = |mask: u16, below: usize| -> Vec<String> {
        (0..below.min(15))
            .filter(|&bit| mask & (1 << bit) != 0)
            .map(|bit| format!("x{bit}"))
            .collect()
    };

    let mut source = String::from("roadmap\n");
    for (track, header, indent) in [
        (0, "track t0\n", 2),
        (1, "  track t1\n", 4),
        (2, "track t2\n", 2),
        (3, "", 0),
    ] {
        source.push_str(header);
        for (index, &(_, mark, needs)) in tasks
            .iter()
            .enumerate()
            .filter(|(_, (which, _, _))| which % 4 == track)
        {
            let mark = BOXES
                .get(usize::from(mark) % BOXES.len())
                .copied()
                .unwrap_or(' ');
            let pad = " ".repeat(indent);
            let _ = writeln!(source, "{pad}[{mark}] x{index}");
            let needs = ids(needs, index);
            if !needs.is_empty() {
                let _ = writeln!(source, "{pad}  needs {}", needs.join(", "));
            }
        }
    }

    for (index, &(needs, wants)) in milestones.iter().take(3).enumerate() {
        let _ = writeln!(source, "milestone m{index}");
        let mut members = ids(needs, tasks.len());
        if index > 0 && needs & (1 << 15) != 0 {
            members.push(format!("m{}", index - 1));
        }
        for member in &members {
            let _ = writeln!(source, "  {member}");
        }
        let wanted: Vec<String> = ids(wants & !needs, tasks.len());
        if !wanted.is_empty() {
            source.push_str("  nice-to-have\n");
            for member in wanted {
                let _ = writeln!(source, "    {member}");
            }
        }
    }
    source
}

/// Edits made on the picture leave a plan that resolves, changed only as
/// asked: a status box on any task; a need between any two tasks, unless
/// it's already there or would make a loop; any task onto any milestone,
/// unless that would make a loop.
#[test]
fn picture_edits_change_only_what_they_say() {
    check!()
        .with_type::<(Vec<(u8, u8, u8, u8)>, u8, u8)>()
        .for_each(|(lines, first, second)| {
            let text = build(lines);
            let Ok(plan) = dagenda_core::parse(&text) else {
                return;
            };
            let tasks: Vec<_> = plan.tasks().iter().map(|task| task.id.clone()).collect();
            let pick = |n: u8| tasks.get(usize::from(n) % tasks.len().max(1));
            let (Some(a), Some(b)) = (pick(*first), pick(*second)) else {
                return;
            };
            let sources = [Source {
                name: String::new(),
                text: text.clone(),
            }];
            let after = |edits: Vec<Edit>| {
                let text = edits
                    .into_iter()
                    .next()
                    .map_or_else(|| text.clone(), |edit| edit.text);
                dagenda_core::parse(&text).map_err(|error| format!("{error}\n{text}"))
            };

            let status = Status::ALL
                .get(usize::from(*second) % Status::ALL.len())
                .copied()
                .unwrap_or(Status::Done);
            let edited = edit::set_status(&sources, a, status)
                .map_err(|error| error.to_string())
                .and_then(after);
            assert!(
                edited
                    .as_ref()
                    .is_ok_and(
                        |edited| edited.task(a).map(|task| task.status) == Some(status)
                            && edited.tasks().len() == plan.tasks().len()
                    ),
                "{edited:?}"
            );

            match edit::add_need(&sources, b, a) {
                Ok(edits) => {
                    let edited = after(edits);
                    assert!(
                        edited.as_ref().is_ok_and(|edited| edited
                            .task(b)
                            .is_some_and(|task| task.needs.iter().any(|need| need.node == *a))
                            && edited.tasks().len() == plan.tasks().len()),
                        "{edited:?}"
                    );
                }
                Err(error) => assert!(
                    matches!(
                        error,
                        EditError::Itself(_)
                            | EditError::AlreadyNeeds { .. }
                            | EditError::Loop { .. }
                    ),
                    "{error}"
                ),
            }

            if let Some(milestone) = plan
                .milestones()
                .get(usize::from(*first) % plan.milestones().len().max(1))
            {
                match edit::rescope(&sources, a, None, Some(&milestone.id)) {
                    Ok(edits) => {
                        let edited = after(edits);
                        assert!(
                            edited
                                .as_ref()
                                .is_ok_and(|edited| waits(edited, &milestone.id, a)),
                            "{edited:?}"
                        );
                    }
                    Err(error) => assert!(matches!(error, EditError::Loop { .. }), "{error}"),
                }
            }
        });
}

/// Setting a detail leaves a plan that resolves, with that detail set and
/// every task still there: any people, or an estimate, on any task.
#[test]
fn setting_details_changes_only_what_it_says() {
    check!()
        .with_type::<(Vec<(u8, u8, u8, u8)>, u8, u8)>()
        .for_each(|(lines, first, second)| {
            let text = build(lines);
            let Ok(plan) = dagenda_core::parse(&text) else {
                return;
            };
            let tasks: Vec<_> = plan.tasks().iter().map(|task| task.id.clone()).collect();
            let Some(a) = tasks.get(usize::from(*first) % tasks.len().max(1)) else {
                return;
            };
            let sources = [Source {
                name: String::new(),
                text: text.clone(),
            }];
            let after = |edits: Vec<Edit>| {
                let text = edits
                    .into_iter()
                    .next()
                    .map_or_else(|| text.clone(), |edit| edit.text);
                dagenda_core::parse(&text).map_err(|error| format!("{error}\n{text}"))
            };

            let people = Handles::new(vec![
                Handle::new(&format!("@p{first}")).expect("a valid handle"),
                Handle::new(&format!("@q{second}")).expect("a valid handle"),
            ])
            .expect("two people");
            let est = Date::new("2026-Q4").expect("a valid date");
            for setting in [Setting::Dri(people.clone()), Setting::Est(est)] {
                let edited = edit::set(&sources, a, &setting)
                    .map_err(|error| error.to_string())
                    .and_then(after);
                assert!(
                    edited.as_ref().is_ok_and(|edited| {
                        let task = edited.task(a);
                        let set = match &setting {
                            Setting::Dri(_) => task
                                .and_then(|task| task.dri.as_ref())
                                .is_some_and(|dri| dri.node == people),
                            Setting::Est(_) | Setting::Target(_) => task
                                .and_then(|task| task.est.as_ref())
                                .is_some_and(|at| at.node == est),
                        };
                        set && edited.tasks().len() == plan.tasks().len()
                    }),
                    "{edited:?}"
                );
            }
        });
}

/// A date parses back to what was written, and covers at least a day, or
/// it's an error; never a panic.
#[test]
fn dates_round_trip() {
    check!()
        .with_type::<(u8, u16, u8, u8)>()
        .for_each(|&(form, year, month, day)| {
            let written = match form % 5 {
                0 => format!("{year:04}-{month:02}-{day:02}"),
                1 => format!("{year:04}-{month:02}"),
                2 => format!("{year:04}-Q{}", month % 6),
                3 => format!("{year:04}"),
                _ => format!("EOY{:02}", day % 100),
            };
            if let Ok(date) = Date::new(&written) {
                assert_eq!(date.to_string(), written);
                assert!(date.first() <= date.last(), "{written}");
            }
        });
    check!().with_type::<String>().for_each(|written| {
        let _ = Date::new(written);
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
const BOXES: [char; 10] = [' ', '>', 'x', '-', '^', '@', '?', '!', '*', '~'];
const MILESTONES: [&str; 2] = ["m", "n"];
const TASKS: [&str; 4] = ["a", "b", "c", "d"];
const TRACKS: [&str; 2] = ["s", "t"];

/// A source from the notation's own lines: each tuple picks an indent, a
/// kind of line, and what goes in it.
fn build(lines: &[(u8, u8, u8, u8)]) -> String {
    let mut source = String::from("roadmap\n");
    for &(indent, kind, first, second) in lines {
        let line = match kind % 11 {
            0 => {
                let mark = BOXES
                    .get(usize::from(second) % BOXES.len())
                    .copied()
                    .unwrap_or(' ');
                format!("[{mark}] {}", pick(&TASKS, first))
            }
            1 => format!("needs {}, {}", pick(&ANY, first), pick(&ANY, second)),
            2 => format!("milestone {}", pick(&MILESTONES, first)),
            3 => pick(&ANY, first).to_owned(),
            4 => String::from("nice-to-have"),
            5 => format!("track {}", pick(&TRACKS, first)),
            6 => format!("extend {}", pick(&TASKS, first)),
            7 => String::from("note something"),
            8 => format!("dri @{}", pick(&["alice", "bob", "carol"], first)),
            9 => format!("est 2026-{:02}", first % 14),
            _ => format!(
                "link https://github.com/o/r/pull/{first} [{}]",
                pick(&TASKS, second)
            ),
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

/// Every error lies within the source, in order.
fn assert_errors_in_bounds(error: &dagenda_core::Error, source: &str) {
    let spans: Vec<Span> = match error {
        dagenda_core::Error::Parse(errors) => errors
            .iter()
            .map(dagenda_core::parser::ParseError::span)
            .collect(),
        dagenda_core::Error::Resolve(errors) => errors.iter().map(|e| e.location().span).collect(),
    };
    for span in &spans {
        assert_in_bounds(*span, source);
    }
    assert!(
        spans.windows(2).all(|pair| match pair {
            [a, b] => a.start <= b.start,
            _ => true,
        }),
        "errors out of order: {spans:?}"
    );
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
/// top of its prerequisite to the bottom of what waits on it, never passes
/// behind any other card, and never runs along another line. The bands
/// stack, every must-have line points up, and everything a milestone needs
/// sits under its line.
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

    for edge in &tree.edges {
        assert_ne!(edge.from, edge.to);
        let (from, to) = (tree.nodes.get(edge.from), tree.nodes.get(edge.to));
        assert!(from.is_some() && to.is_some(), "a line to a missing node");
        let (Some(from), Some(to)) = (from, to) else {
            continue;
        };
        if !edge.wanted {
            assert!(from.row < to.row, "{} → {} points down", from.id, to.id);
        }
        assert_routed(&tree, edge, from, to);
    }
    assert_apart(&tree);

    for pair in tree.bands.windows(2) {
        let [below, above] = pair else { continue };
        assert_eq!(below.top, above.bottom, "bands stack without gaps");
    }
    for band in &tree.bands {
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
    assert!(
        route
            .first()
            .is_some_and(|p| p.y == from.y && from.x < p.x && p.x < from.x + from.width),
        "{} → {} starts on the top edge of {}",
        from.id,
        to.id,
        from.id
    );
    assert!(
        route
            .last()
            .is_some_and(|p| (to.y + to.height..=to.entry()).contains(&p.y)
                && to.x < p.x
                && p.x < to.x + to.width),
        "{} → {} ends on the bottom of {}",
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

/// No two lines run along each other, or closer than half a lane apart, so
/// every line can be followed from end to end.
fn assert_apart(tree: &TechTree) {
    let segments: Vec<(usize, Point, Point)> = tree
        .edges
        .iter()
        .enumerate()
        .flat_map(|(line, edge)| {
            edge.route.windows(2).filter_map(move |pair| match pair {
                [a, b] => Some((line, *a, *b)),
                _ => None,
            })
        })
        .collect();
    let name = |line: usize| {
        tree.edges.get(line).map_or_else(String::new, |edge| {
            let id = |node: usize| tree.nodes.get(node).map(|node| node.id.to_string());
            format!("{:?} → {:?}", id(edge.from), id(edge.to))
        })
    };
    for (index, &(line, a, b)) in segments.iter().enumerate() {
        for &(other, c, d) in segments.iter().skip(index + 1) {
            if line == other {
                continue;
            }
            let along = if a.x == b.x && c.x == d.x {
                a.x.abs_diff(c.x) < 4 && overlap((a.y, b.y), (c.y, d.y))
            } else if a.y == b.y && c.y == d.y {
                a.y.abs_diff(c.y) < 4 && overlap((a.x, b.x), (c.x, d.x))
            } else {
                false
            };
            assert!(
                !along,
                "{} runs along {}: {a:?}–{b:?} and {c:?}–{d:?}",
                name(line),
                name(other)
            );
        }
    }
}

/// Whether two spans meet, even at a point: two lines meeting end to end
/// read as one.
fn overlap((a, b): (u32, u32), (c, d): (u32, u32)) -> bool {
    a.max(b).min(c.max(d)) >= a.min(b).max(c.min(d))
}

/// What's derived agrees with itself: readiness with statuses and needs,
/// progress with what's finished, critical paths with what isn't.
/// Whether `from` needs `target`, directly or through other work.
fn waits(plan: &Plan, from: &dagenda_core::id::Id, target: &dagenda_core::id::Id) -> bool {
    let needs = |id: &dagenda_core::id::Id| -> Vec<dagenda_core::id::Id> {
        plan.task(id)
            .map(|task| &task.needs)
            .or_else(|| plan.milestone(id).map(|milestone| &milestone.needs))
            .map(|needs| needs.iter().map(|need| need.node.clone()).collect())
            .unwrap_or_default()
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut stack = needs(from);
    while let Some(next) = stack.pop() {
        if next == *target {
            return true;
        }
        if seen.insert(next.clone()) {
            stack.extend(needs(&next));
        }
    }
    false
}

/// How many distinct tasks are among `ids`.
fn tasks_among(plan: &Plan, ids: &[dagenda_core::plan::Located<dagenda_core::id::Id>]) -> usize {
    ids.iter()
        .filter(|id| plan.task(&id.node).is_some())
        .map(|id| &id.node)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}

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

        // All the work behind a task counts at least the tasks it needs
        // directly, and never itself
        let work = plan.work(&task.id);
        let direct = tasks_among(plan, &task.needs);
        assert!(
            work.is_some_and(|work| work.done <= work.total
                && work.total >= direct
                && work.total < plan.tasks().len()),
            "{}: {work:?}, {direct} direct",
            task.id
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
                && progress.work.done <= progress.work.total
                && progress.work.total >= tasks_among(plan, &milestone.needs)
                && progress.is_reached() == plan.is_finished(&milestone.id)
        }));

        let path = plan.critical_path(&milestone.id);
        assert!(path.iter().all(|id| !plan.is_finished(id)));
        assert_eq!(path.is_empty(), plan.is_finished(&milestone.id));
    }
}
