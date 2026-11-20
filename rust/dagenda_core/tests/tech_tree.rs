//! The tech tree's layout: which lines it draws, which branches and banners
//! it makes, where things land, and how lines are routed.

use dagenda_core::tech_tree::{BranchKind, Hue, Shape, TechTree};
use testresult::TestResult;

fn tree(source: &str) -> Result<TechTree, dagenda_core::Error> {
    Ok(TechTree::new(&dagenda_core::parse(source)?))
}

/// `from → to` for every line, by id.
fn lines(tree: &TechTree) -> Vec<String> {
    named(tree, &tree.edges)
}

/// `from → to` for each of `edges`, by id.
fn named(tree: &TechTree, edges: &[dagenda_core::tech_tree::Edge]) -> Vec<String> {
    edges
        .iter()
        .filter_map(|edge| {
            let from = tree.nodes.get(edge.from)?;
            let to = tree.nodes.get(edge.to)?;
            let wanted = if edge.wanted { " (wanted)" } else { "" };
            Some(format!("{} → {}{wanted}", from.id, to.id))
        })
        .collect()
}

fn node<'a>(tree: &'a TechTree, id: &str) -> Option<&'a dagenda_core::tech_tree::Node> {
    tree.nodes.iter().find(|node| node.id.as_str() == id)
}

fn row(tree: &TechTree, id: &str) -> Option<usize> {
    node(tree, id).map(|node| node.row)
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
    assert_eq!(lines(&tree), ["a → b", "b → c"], "c sits under m's line");
    assert_eq!(
        named(&tree, &tree.implied),
        ["c → m"],
        "but it's still there to follow"
    );
    assert_eq!(
        (row(&tree, "a"), row(&tree, "b"), row(&tree, "c")),
        (Some(0), Some(1), Some(2))
    );
    assert_eq!(row(&tree, "m"), Some(3));
    Ok(())
}

#[test]
fn prerequisites_sit_below_what_they_unlock() -> TestResult {
    let tree = tree(
        "roadmap
[ ] task a
[ ] task b
  needs a",
    )?;
    let (a, b) = (
        node(&tree, "a").ok_or("no a")?,
        node(&tree, "b").ok_or("no b")?,
    );
    assert!(b.y + b.height < a.y, "a should be below b");
    Ok(())
}

#[test]
fn nice_to_haves_are_dashed_and_sit_under_their_milestone() -> TestResult {
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
        ["a → later", "later → muchLater", "muchLater → m (wanted)"]
    );
    assert_eq!(
        row(&tree, "m"),
        Some(3),
        "a, later, and muchLater sit under it"
    );
    Ok(())
}

#[test]
fn nice_to_haves_sit_in_the_era_that_wants_them() -> TestResult {
    let tree = tree(
        "roadmap
[ ] task must
[ ] task groundwork
[ ] task polish
  needs groundwork
milestone first
  must
  ---
  polish
milestone second
  must
[ ] task after
  needs first",
    )?;
    let gate = |band: usize| {
        let band = tree.bands.get(band)?;
        tree.nodes.get(band.milestone?).map(|gate| gate.id.as_str())
    };
    assert_eq!((gate(0), gate(1)), (Some("first"), Some("second")));

    let line = tree.bands.first().map(|band| band.top).ok_or("no bands")?;
    for id in ["must", "groundwork", "polish"] {
        assert!(
            node(&tree, id).is_some_and(|card| card.y > line),
            "{id} is under first's line"
        );
    }
    assert!(node(&tree, "after").is_some_and(|card| card.y + card.height < line));
    assert!(lines(&tree).contains(&String::from("polish → first (wanted)")));
    Ok(())
}

#[test]
fn milestones_are_eras_holding_the_work_they_need() -> TestResult {
    let tree = tree(
        "roadmap
[x] task a
[ ] task b
  needs a
[ ] task c
  needs b
[ ] task beyond
[x] task aside
  needs a
milestone first
  b
milestone second
  c",
    )?;
    assert_eq!(lines(&tree), ["a → b", "a → aside", "b → c"]);

    let eras: Vec<_> = tree
        .bands
        .iter()
        .map(|band| {
            band.milestone
                .and_then(|gate| tree.nodes.get(gate))
                .map(|gate| gate.id.to_string())
        })
        .collect();
    assert_eq!(
        eras,
        [
            Some(String::from("first")),
            Some(String::from("second")),
            None
        ]
    );

    let band_of = |id: &str| {
        let card = node(&tree, id)?;
        tree.bands
            .iter()
            .position(|band| band.top < card.y && card.y + card.height <= band.bottom)
    };
    assert_eq!(
        ["a", "b", "c", "aside", "beyond"].map(band_of),
        [Some(0), Some(0), Some(1), Some(0), Some(2)],
        "finished work no milestone needs sits as early as it can; unfinished, beyond them all"
    );
    Ok(())
}

#[test]
fn a_milestone_needing_all_another_needs_goes_above_it() -> TestResult {
    let tree = tree(
        "roadmap
[ ] task a
[ ] task b
  needs a
milestone big
  b
milestone small
  a",
    )?;
    let first = tree.bands.first().and_then(|band| band.milestone);
    assert_eq!(
        first
            .and_then(|gate| tree.nodes.get(gate))
            .map(|gate| gate.id.as_str()),
        Some("small")
    );
    assert_eq!(lines(&tree), ["a → b"]);
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

    let parent = node(&tree, "parent").ok_or("no parent")?;
    let Shape::Task {
        subtasks, locked, ..
    } = parent.shape
    else {
        return Err("parent isn't a task".into());
    };
    assert_eq!((subtasks.done, subtasks.total), (1, 2));
    assert!(!locked, "parent itself waits on nothing");
    assert_eq!(parent.caption.as_deref(), Some("1 of 2 subtasks"));
    Ok(())
}

#[test]
fn tasks_waiting_on_unfinished_work_are_locked() -> TestResult {
    let tree = tree(
        "roadmap
[x] task done
[ ] task open
[ ] task afterDone
  needs done
[ ] task afterOpen
  needs open",
    )?;
    let locked = |id: &str| {
        node(&tree, id).map(|node| matches!(node.shape, Shape::Task { locked: true, .. }))
    };
    assert_eq!(locked("afterDone"), Some(false));
    assert_eq!(locked("afterOpen"), Some(true));
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
fn tracks_are_branches_under_banners_then_everything_else() -> TestResult {
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

    let kinds: Vec<String> = tree
        .branches
        .iter()
        .map(|branch| match &branch.kind {
            BranchKind::Track(id) => id.to_string(),
            BranchKind::Untracked => String::from("(untracked)"),
            BranchKind::Milestones => String::from("(milestones)"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "repo",
            "subduction",
            "keyhive",
            "(untracked)",
            "(milestones)"
        ],
        "sync has no tasks of its own, so only a banner"
    );

    let banners: Vec<_> = tree
        .banners
        .iter()
        .map(|banner| (banner.heading.as_str(), banner.depth))
        .collect();
    assert_eq!(
        banners,
        [
            ("Sync", 0),
            ("Repo", 1),
            ("subduction", 1),
            ("Keyhive", 0),
            ("Other tasks", 0),
        ]
    );

    let hue = |hue: Option<Hue>| hue.map(Hue::index);
    assert_eq!(
        tree.branches
            .iter()
            .map(|branch| hue(branch.hue))
            .collect::<Vec<_>>(),
        [Some(1), Some(2), Some(3), None, None],
        "each track its own colour, in the order written"
    );
    assert_eq!(
        tree.banners
            .iter()
            .map(|banner| hue(banner.hue))
            .collect::<Vec<_>>(),
        [Some(0), Some(1), Some(2), Some(3), None]
    );

    let sync = tree.banners.first().ok_or("no Sync banner")?;
    let (repo, subduction) = (
        tree.branches.first().ok_or("no repo branch")?,
        tree.branches.get(1).ok_or("no subduction branch")?,
    );
    assert_eq!(sync.x, repo.x, "Sync's banner spans its nested tracks");
    assert_eq!(sync.x + sync.width, subduction.x + subduction.width);
    Ok(())
}

#[test]
fn a_long_heading_widens_its_branch() -> TestResult {
    let tree = tree(
        "roadmap
track comms [Communications (writing and demos), and then some more]
track other
  [ ] task a",
    )?;
    let (comms, other) = (
        tree.branches.first().ok_or("no comms branch")?,
        tree.branches.get(1).ok_or("no other branch")?,
    );
    assert!(
        comms.width > other.width + 200,
        "{} vs {}",
        comms.width,
        other.width
    );
    Ok(())
}

#[test]
fn a_crowded_row_wraps_onto_two_staggered_lines() -> TestResult {
    let tree = tree(
        "roadmap
[ ] task a
[ ] task b
[ ] task c
[ ] task d
[ ] task e",
    )?;
    let tops: std::collections::BTreeSet<u32> = tree.nodes.iter().map(|node| node.y).collect();
    assert_eq!(tops.len(), 2, "five cards wrap onto two lines");

    let (upper, lower): (Vec<_>, Vec<_>) = tree
        .nodes
        .iter()
        .partition(|node| tops.first() == Some(&node.y));
    for climbing in &lower {
        for card in &upper {
            assert!(
                climbing.middle() < card.x || climbing.middle() > card.x + card.width,
                "a line climbing from {} passes behind {}",
                climbing.id,
                card.id
            );
        }
    }
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
