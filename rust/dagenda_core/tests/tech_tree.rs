//! The tech tree's layout: which lines it draws, which branches and banners
//! it makes, where things land, and how lines are routed.

use dagenda_core::{
    colour::{Colour, Named, Rgb},
    tech_tree::{BranchKind, Fact, Shape, TechTree},
};
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
        tree.bands.len(),
        1,
        "no layer for work on no milestone: there's none"
    );
    assert_eq!(
        (row(&tree, "a"), row(&tree, "b"), row(&tree, "c")),
        (Some(0), Some(1), Some(2))
    );
    assert_eq!(row(&tree, "m"), Some(3));
    Ok(())
}

#[test]
fn a_path_through_a_band_doesnt_hide_a_line() -> TestResult {
    let tree = tree(
        "roadmap
[ ] task x
milestone m
  x
[ ] task y
  needs x, m",
    )?;
    assert_eq!(
        lines(&tree),
        ["x → y", "m → y"],
        "x → m isn't drawn, so it can't stand in for x → y"
    );
    assert_eq!(named(&tree, &tree.implied), ["x → m"]);
    Ok(())
}

#[test]
fn a_nice_to_have_the_milestone_needs_anyway_has_no_line() -> TestResult {
    let tree = tree(
        "roadmap
[ ] task part
[ ] task whole
  needs part
milestone m
  whole
  nice-to-have
    part",
    )?;
    assert_eq!(
        lines(&tree),
        ["part → whole"],
        "no dashed part → m: m needs part anyway, through whole"
    );
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
  nice-to-have
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
  nice-to-have
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
        [Some(0), Some(0), Some(1), Some(2), Some(2)],
        "work no milestone needs sits in its own layer on top, done or not"
    );
    assert_eq!(tree.bands.last().and_then(|band| band.milestone), None);
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
fn a_task_written_under_another_is_one_it_needs() -> TestResult {
    let tree = tree(
        "roadmap
track core
  [ ] task setup
  [-] task parent
    [x] task first
    [ ] task second
      needs setup",
    )?;
    assert_eq!(tree.nodes.len(), 4, "every task is a card");
    assert_eq!(
        lines(&tree),
        ["setup → second", "first → parent", "second → parent"]
    );
    assert!(row(&tree, "parent") > row(&tree, "first"));
    assert!(row(&tree, "parent") > row(&tree, "second"));
    assert!(
        tree.nodes.iter().all(|node| node.branch == 0),
        "all in core's branch"
    );

    let parent = node(&tree, "parent").ok_or("no parent")?;
    assert!(
        matches!(parent.shape, Shape::Task { locked: true, .. }),
        "it waits on its open parts"
    );
    assert_eq!(
        parent.caption.as_deref(),
        Some("1 of 3 done below"),
        "how close it is to unlocking: first, second, and setup through second"
    );
    let svg = dagenda_core::parse("roadmap\n[-] task a\n  [ ] task b\n")?.to_svg();
    assert!(
        svg.contains(r#"class="node task in-progress waiting""#),
        "under way, so not dimmed as locked"
    );
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

    let hue = |colour: Option<Colour>| match colour {
        Some(Colour::Named(named)) => Some(named.index()),
        Some(Colour::Rgb(_)) | None => None,
    };
    assert_eq!(
        tree.branches
            .iter()
            .map(|branch| hue(branch.colour))
            .collect::<Vec<_>>(),
        [Some(1), Some(2), Some(3), None, None],
        "each track its own colour, in the order written"
    );
    assert_eq!(
        tree.banners
            .iter()
            .map(|banner| hue(banner.colour))
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
fn cards_say_who_is_on_them_when_and_where_to_read_more() -> TestResult {
    let tree = tree(
        "roadmap
track core [Core]
  dri @alice
  [-] task sync [Sync]
    dri @bob
    est 2026-11-19
    pr gh:owner/repo#7
    link https://github.com/owner/repo/issues/3#issuecomment-9
    note Ask carol first.
    [ ] task part
      dri @carol",
    )?;
    let card = node(&tree, "sync").ok_or("no sync")?;
    let facts: Vec<_> = card.facts.iter().flatten().map(Fact::text).collect();
    assert_eq!(facts, ["@bob", "est 2026-11-19", "PR #7", "#3 comment"]);
    let part = node(&tree, "part").ok_or("no part")?;
    assert_eq!(
        part.facts
            .iter()
            .flatten()
            .map(Fact::text)
            .collect::<Vec<_>>(),
        ["@carol"],
        "a task written under another has a card of its own"
    );
    let links: Vec<_> = card.facts.iter().flatten().filter_map(Fact::url).collect();
    assert_eq!(
        links,
        [
            "https://github.com/owner/repo/pull/7",
            "https://github.com/owner/repo/issues/3#issuecomment-9"
        ]
    );
    assert_eq!(card.notes.len(), 1);

    let svg =
        dagenda_core::parse("roadmap\n[ ] task a\n  link https://example.com/a?b=1&c=2 [Spec]\n")?
            .to_svg();
    assert!(svg.contains(r#"<a href="https://example.com/a?b=1&amp;c=2" target="_blank">"#));

    let banner = tree.banners.first().ok_or("no banner")?;
    assert_eq!(
        banner.dri.as_ref().map(ToString::to_string).as_deref(),
        Some("@alice")
    );
    Ok(())
}

#[test]
fn banners_sit_over_their_branches() -> TestResult {
    let tree = tree(
        "roadmap
track sync [Sync]
  track repo [Repo]
    [ ] task a",
    )?;
    let (sync, repo) = (
        tree.banners.first().ok_or("no Sync")?,
        tree.banners.get(1).ok_or("no Repo")?,
    );
    let branch = tree.branches.first().ok_or("no branch")?;
    assert!(sync.y + sync.height <= repo.y, "Sync is over Repo");
    assert!(repo.y + repo.height <= branch.y, "Repo is over its branch");
    Ok(())
}

/// Lines out of one card, or into one, never run together: each leaves
/// from a port of its own and arrives at a port of its own, so Resolver's
/// and It parses' lines into Tech tree stay apart, as do It parses' lines to
/// Tech tree and First picture, and Skill tree's beside them.
#[test]
fn every_line_has_ports_of_its_own() -> TestResult {
    let tree = tree(
        "roadmap
[x] task parser
[-] task resolver
  needs parser
[ ] task techTree
  needs resolver, itParses
[?] task skillTree
milestone itParses
  parser
  resolver
milestone firstPicture
  itParses
  techTree
  nice-to-have
    skillTree",
    )?;
    assert_eq!(
        lines(&tree),
        [
            "parser → resolver",
            "resolver → techTree",
            "skillTree → firstPicture (wanted)",
            "itParses → techTree",
            "itParses → firstPicture",
        ]
    );
    for (index, edge) in tree.edges.iter().enumerate() {
        for other in tree.edges.iter().skip(index + 1) {
            if edge.from == other.from {
                assert_ne!(
                    edge.route.first(),
                    other.route.first(),
                    "{edge:?} and {other:?} leave together"
                );
            }
            if edge.to == other.to {
                assert_ne!(
                    edge.route.last(),
                    other.route.last(),
                    "{edge:?} and {other:?} arrive together"
                );
            }
        }
    }
    Ok(())
}

/// A track can set its colour, by the palette's name or as `#rrggbb`; the
/// rest take the palette in order, skipping any colour a track claimed.
#[test]
fn tracks_take_the_colour_they_set() -> TestResult {
    let tree = tree(
        "roadmap
track a
  colour plum
  [ ] task one
track b
  [ ] task two
track c
  color #2A9D8F
  [ ] task three
track d
  [ ] task four",
    )?;
    let colours: Vec<Option<Colour>> = tree.banners.iter().map(|banner| banner.colour).collect();
    assert_eq!(
        colours,
        [
            Some(Colour::Named(Named::Plum)),
            Some(Colour::Named(Named::Teal)),
            Some(Colour::Rgb(Rgb::new(0x2a, 0x9d, 0x8f))),
            Some(Colour::Named(Named::Indigo)),
        ]
    );

    let svg = tree.to_svg();
    assert!(
        svg.contains(r#"class="banner track hue-5 "#),
        "plum is the palette's sixth"
    );
    assert!(
        svg.contains(r#"class="branch track colour-2a9d8f""#),
        "{svg}"
    );
    assert!(
        svg.contains(".dagenda .branch.colour-2a9d8f, .dagenda .banner.colour-2a9d8f rect { fill: #eaf5f4; stroke: #bfe2dd; }"),
        "light tints derived from the colour"
    );
    assert!(
        svg.contains(".dagenda.dark .banner.colour-2a9d8f text { fill: #aad8d2; }"),
        "and dark ones"
    );
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
