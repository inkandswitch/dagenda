//! The tech tree's layout: which lines it draws, which lanes it makes, and
//! where things land.

use dagenda_core::tech_tree::{LaneKind, Shape, TechTree};
use testresult::TestResult;

fn tree(source: &str) -> Result<TechTree, dagenda_core::Error> {
    Ok(TechTree::new(&dagenda_core::parse(source)?))
}

/// `from → to` for every line, by id.
fn lines(tree: &TechTree) -> Vec<String> {
    tree.edges
        .iter()
        .filter_map(|edge| {
            let from = tree.nodes.get(edge.from)?;
            let to = tree.nodes.get(edge.to)?;
            let wanted = if edge.wanted { " (wanted)" } else { "" };
            Some(format!("{} → {}{wanted}", from.id, to.id))
        })
        .collect()
}

fn column(tree: &TechTree, id: &str) -> Option<usize> {
    tree.nodes
        .iter()
        .find(|node| node.id.as_str() == id)
        .map(|node| node.column)
}

#[test]
fn lines_a_longer_path_implies_are_left_out() -> TestResult {
    let tree = tree(
        "roadmap
[ ] task a
[ ] task b
  needs a
[ ] task c
  needs a, b
milestone m
  a
  c",
    )?;
    assert_eq!(lines(&tree), ["a → b", "b → c", "c → m"]);
    assert_eq!(
        (column(&tree, "a"), column(&tree, "b"), column(&tree, "c")),
        (Some(0), Some(1), Some(2))
    );
    assert_eq!(column(&tree, "m"), Some(3));
    Ok(())
}

#[test]
fn nice_to_haves_are_dashed_and_dont_push_the_milestone_right() -> TestResult {
    let tree = tree(
        "roadmap
[ ] task a
[ ] task later
  needs a
[ ] task muchLater
  needs later
milestone m
  a
  ---
  muchLater",
    )?;
    assert_eq!(
        lines(&tree),
        [
            "a → later",
            "a → m",
            "later → muchLater",
            "muchLater → m (wanted)"
        ]
    );
    assert_eq!(column(&tree, "m"), Some(1));
    Ok(())
}

#[test]
fn subtasks_fold_into_their_task() -> TestResult {
    let tree = tree(
        "roadmap
[ ] task setup
[ ] task parent
  [x] task first
  [ ] task second
    needs setup",
    )?;
    assert_eq!(tree.nodes.len(), 2);
    assert_eq!(lines(&tree), ["setup → parent"]);

    let parent = tree
        .nodes
        .iter()
        .find(|node| node.id.as_str() == "parent")
        .ok_or("no parent")?;
    let Shape::Task { subtasks, .. } = parent.shape else {
        return Err("parent isn't a task".into());
    };
    assert_eq!((subtasks.done, subtasks.total), (1, 2));
    assert_eq!(parent.caption.as_deref(), Some("1 of 2 subtasks"));
    Ok(())
}

#[test]
fn a_loop_made_by_folding_subtasks_still_lays_out() -> TestResult {
    let tree = tree(
        "roadmap
[ ] task parent
  [ ] task child
    needs other
[ ] task other
  needs parent",
    )?;
    assert_eq!(lines(&tree).len(), 2);
    assert_eq!(tree.nodes.len(), 2);
    Ok(())
}

#[test]
fn milestones_come_first_then_tracks_in_order_then_everything_else() -> TestResult {
    let tree = tree(
        "roadmap
track sync [Sync]
  track repo [Repo]
    [ ] task a
  track subduction
    [ ] task b
track keyhive [Keyhive]
[ ] task loose
milestone m
  a",
    )?;
    let lanes: Vec<_> = tree
        .lanes
        .iter()
        .map(|lane| (lane.heading.as_str(), lane.depth))
        .collect();
    assert_eq!(
        lanes,
        [
            ("Milestones", 0),
            ("Sync", 0),
            ("Repo", 1),
            ("subduction", 1),
            ("Keyhive", 0),
            ("Other tasks", 0),
        ]
    );
    assert!(matches!(
        tree.lanes.first().map(|l| &l.kind),
        Some(LaneKind::Milestones)
    ));
    assert!(matches!(
        tree.lanes.last().map(|l| &l.kind),
        Some(LaneKind::Untracked)
    ));
    Ok(())
}

#[test]
fn long_labels_wrap() -> TestResult {
    let tree = tree(
        "roadmap
[ ] task long [A label long enough that it has to wrap onto a second line, and maybe a third]",
    )?;
    let node = tree.nodes.first().ok_or("no node")?;
    assert!(node.lines.len() > 1, "{:?}", node.lines);
    assert_eq!(
        node.lines.join(" "),
        "A label long enough that it has to wrap onto a second line, and maybe a third"
    );
    Ok(())
}

#[test]
fn svg_escapes_what_it_quotes() -> TestResult {
    let plan = dagenda_core::parse(
        "roadmap
title Ampersands & <angles>
track t [\"Quotes\" & <tags>]
  [ ] task a [Fish & chips <3]",
    )?;
    let svg = plan.to_svg();
    assert!(svg.contains("Ampersands &amp; &lt;angles&gt;"));
    assert!(svg.contains("&quot;Quotes&quot; &amp; &lt;tags&gt;"));
    assert!(svg.contains("Fish &amp; chips &lt;3"));
    assert!(!svg.contains("<3"));
    Ok(())
}
