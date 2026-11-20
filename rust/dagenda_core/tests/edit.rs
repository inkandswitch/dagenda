//! Edits made on the picture: the text each writes, and what each refuses.

use dagenda_core::{
    check::Source,
    date::Date,
    edit::{self, Edit, EditError, Setting},
    handle::{Handle, Handles},
    id::Id,
    status::Status,
};
use testresult::TestResult;

fn one(text: &str) -> Vec<Source> {
    vec![Source {
        name: String::new(),
        text: text.into(),
    }]
}

fn id(text: &str) -> Result<Id, Box<dyn std::error::Error>> {
    Ok(Id::new(text)?)
}

/// Each edit's document and new text.
fn texts(edits: Vec<Edit>) -> Vec<(usize, String)> {
    edits
        .into_iter()
        .map(|edit| (edit.document, edit.text))
        .collect()
}

#[test]
fn a_status_click_swaps_the_box_and_nothing_else() -> TestResult {
    let plan = one("roadmap\ntrack t\n  [ ] a [A]\n    est 2026-12\n");
    assert_eq!(
        texts(edit::set_status(&plan, &id("a")?, Status::InProgress)?),
        [(
            0,
            String::from("roadmap\ntrack t\n  [-] a [A]\n    est 2026-12\n")
        )]
    );
    assert!(
        edit::set_status(&plan, &id("a")?, Status::NotStarted)?.is_empty(),
        "already so"
    );
    Ok(())
}

#[test]
fn a_status_click_edits_the_document_that_defines_the_task() -> TestResult {
    let plan = vec![
        Source {
            name: "public".into(),
            text: "roadmap\n[ ] a\n".into(),
        },
        Source {
            name: "private".into(),
            text: "roadmap\nextend a\n  dri @alice\n".into(),
        },
    ];
    assert_eq!(
        texts(edit::set_status(&plan, &id("a")?, Status::Done)?),
        [(0, String::from("roadmap\n[x] a\n"))]
    );
    Ok(())
}

#[test]
fn a_new_need_gets_a_line_in_the_block_s_own_indent() -> TestResult {
    assert_eq!(
        texts(edit::add_need(
            &one("roadmap\ntrack t\n  [ ] a\n  [ ] b\n      est 2026-12\n"),
            &id("b")?,
            &id("a")?
        )?),
        [(
            0,
            String::from("roadmap\ntrack t\n  [ ] a\n  [ ] b\n      needs a\n      est 2026-12\n")
        )]
    );
    assert_eq!(
        texts(edit::add_need(
            &one("roadmap\n[ ] a\n[ ] b"),
            &id("b")?,
            &id("a")?
        )?),
        [(0, String::from("roadmap\n[ ] a\n[ ] b\n  needs a\n"))],
        "a last line with no line break"
    );
    Ok(())
}

#[test]
fn a_new_need_joins_the_last_needs_line() -> TestResult {
    assert_eq!(
        texts(edit::add_need(
            &one("roadmap\n[ ] a\n[ ] c\n[ ] b\n  needs c\n"),
            &id("b")?,
            &id("a")?
        )?),
        [(
            0,
            String::from("roadmap\n[ ] a\n[ ] c\n[ ] b\n  needs c, a\n")
        )]
    );
    Ok(())
}

#[test]
fn needs_that_make_no_sense_are_refused() -> TestResult {
    let plan = one("roadmap\n[ ] a\n  needs b\n[ ] b\n  [ ] c\nmilestone m\n  a\n");
    let need = |dependent: &str, prerequisite: &str| -> Result<_, Box<dyn std::error::Error>> {
        Ok(edit::add_need(&plan, &id(dependent)?, &id(prerequisite)?)
            .map_err(|error| error.to_string()))
    };
    assert_eq!(need("a", "a")?, Err(String::from("`a` can't need itself")));
    assert_eq!(need("a", "b")?, Err(String::from("`a` already needs `b`")));
    assert_eq!(
        need("b", "c")?,
        Err(String::from("`b` already needs `c`")),
        "written under it"
    );
    assert_eq!(
        need("b", "a")?,
        Err(String::from(
            "`a` already waits on `b`, so `b` can't need it"
        ))
    );
    assert_eq!(
        need("c", "a")?,
        Err(String::from(
            "`a` already waits on `c`, so `c` can't need it"
        ))
    );
    assert_eq!(need("m", "a")?, Err(String::from("`m` isn't a task")));
    assert_eq!(
        need("a", "nowhere")?,
        Err(String::from("nothing in the plan is called `nowhere`"))
    );

    let broken = one("roadmap\n[ ] a\n  needs missing\n[ ] b\n");
    assert_eq!(
        edit::add_need(&broken, &id("b")?, &id("a")?),
        Err(EditError::Unresolved)
    );
    Ok(())
}

const ERAS: &str = "roadmap
[ ] a
[ ] b
[ ] c
  needs a

milestone first
  target 2026-Q4
  a
  nice-to-have
    b

milestone second
  c
";

#[test]
fn rescoping_moves_a_member_between_milestones() -> TestResult {
    let plan = one(ERAS);
    let [(_, moved)] = <[_; 1]>::try_from(texts(edit::rescope(
        &plan,
        &id("c")?,
        Some(&id("second")?),
        Some(&id("first")?),
    )?))
    .map_err(|_| "one edit")?;
    assert!(
        moved.contains("milestone first\n  target 2026-Q4\n  a\n  c\n  nice-to-have\n    b\n"),
        "{moved}"
    );
    assert!(moved.contains("milestone second\n"), "{moved}");
    assert!(!moved.contains("milestone second\n  c"), "{moved}");

    let [(_, promoted)] = <[_; 1]>::try_from(texts(edit::rescope(
        &plan,
        &id("b")?,
        None,
        Some(&id("first")?),
    )?))
    .map_err(|_| "one edit")?;
    assert!(
        promoted.contains("  target 2026-Q4\n  a\n  b\n\nmilestone second"),
        "a nice-to-have becomes a must-have, and the empty block goes: {promoted}"
    );
    Ok(())
}

#[test]
fn rescoping_to_no_milestone_takes_it_off_the_list() -> TestResult {
    let plan = one(ERAS);
    let [(_, off)] = <[_; 1]>::try_from(texts(edit::rescope(
        &plan,
        &id("c")?,
        Some(&id("second")?),
        None,
    )?))
    .map_err(|_| "one edit")?;
    assert!(off.ends_with("milestone second\n"), "{off}");

    let looped = one("roadmap\n[ ] a\n  needs m\n[ ] b\nmilestone m\n  b\n");
    assert_eq!(
        edit::rescope(&looped, &id("a")?, None, Some(&id("m")?)).map_err(|error| error.to_string()),
        Err(String::from(
            "`a` already waits on `m`, so `m` can't need it"
        ))
    );

    let already = one("roadmap\n[ ] a\n[ ] b\n  needs a\nmilestone m\n  b\n");
    assert!(
        edit::rescope(&already, &id("a")?, None, Some(&id("m")?))?.is_empty(),
        "m needs a through b already"
    );

    let gone = one("roadmap\n[ ] a\n[ ] b\n  needs a\nmilestone m\n  a\n");
    assert!(
        edit::rescope(&gone, &id("b")?, Some(&id("m")?), None)?.is_empty(),
        "m doesn't need b at all: nothing to take off"
    );

    let through = one("roadmap\n[ ] a\n[ ] b\n  needs a\nmilestone m\n  b\n");
    assert_eq!(
        edit::rescope(&through, &id("a")?, Some(&id("m")?), None)
            .map_err(|error| error.to_string()),
        Err(String::from(
            "`m` needs `a` through other work, so it stays in that era"
        ))
    );
    Ok(())
}

#[test]
fn a_milestone_with_no_members_gets_its_first() -> TestResult {
    assert_eq!(
        texts(edit::rescope(
            &one("roadmap\n[ ] a\nmilestone m [M]\n  target 2026-Q4\n"),
            &id("a")?,
            None,
            Some(&id("m")?)
        )?),
        [(
            0,
            String::from("roadmap\n[ ] a\nmilestone m [M]\n  target 2026-Q4\n  a\n")
        )]
    );
    assert_eq!(
        texts(edit::rescope(
            &one("roadmap\n[ ] a\nmilestone m"),
            &id("a")?,
            None,
            Some(&id("m")?)
        )?),
        [(0, String::from("roadmap\n[ ] a\nmilestone m\n  a\n"))]
    );
    Ok(())
}

const ORDER: &str = "roadmap
[ ] x
[ ] y

milestone a
  x

milestone b
  y
";

#[test]
fn reordering_moves_a_milestone_block_whole() -> TestResult {
    let plan = one(ORDER);
    assert_eq!(
        texts(edit::reorder(&plan, &id("a")?, &id("b")?, true)?),
        [(
            0,
            String::from("roadmap\n[ ] x\n[ ] y\n\nmilestone b\n  y\n\nmilestone a\n  x\n")
        )]
    );
    assert_eq!(
        texts(edit::reorder(&plan, &id("b")?, &id("a")?, false)?),
        [(
            0,
            String::from("roadmap\n[ ] x\n[ ] y\n\nmilestone b\n  y\n\nmilestone a\n  x\n")
        )]
    );
    assert!(
        edit::reorder(&plan, &id("a")?, &id("b")?, false)?.is_empty(),
        "already so"
    );
    Ok(())
}

#[test]
fn an_order_one_milestone_builds_on_is_fixed() -> TestResult {
    let plan = one("roadmap\n[ ] x\n\nmilestone a\n  x\n\nmilestone b\n  a\n");
    assert_eq!(
        edit::reorder(&plan, &id("b")?, &id("a")?, false).map_err(|error| error.to_string()),
        Err(String::from("`b` builds on `a`, so it stays above it"))
    );

    let split = vec![
        Source {
            name: "one".into(),
            text: "roadmap\n[ ] x\nmilestone a\n  x\n".into(),
        },
        Source {
            name: "two".into(),
            text: "roadmap\nmilestone b\n  x\n".into(),
        },
    ];
    assert_eq!(
        edit::reorder(&split, &id("a")?, &id("b")?, true),
        Err(EditError::DifferentDocuments {
            milestone: id("a")?,
            other: id("b")?
        })
    );
    Ok(())
}

#[test]
fn a_document_that_doesn_t_parse_is_named() -> TestResult {
    let plan = vec![
        Source {
            name: "public".into(),
            text: "roadmap\n[ ] a\n".into(),
        },
        Source {
            name: "private".into(),
            text: "roadmap\n  oops\n".into(),
        },
    ];
    assert_eq!(
        edit::set_status(&plan, &id("a")?, Status::Done).map_err(|error| error.to_string()),
        Err(String::from(
            "`private` doesn't parse, so the picture can't edit it"
        ))
    );
    Ok(())
}

fn people(names: &[&str]) -> Result<Handles, Box<dyn std::error::Error>> {
    let handles = names
        .iter()
        .map(|name| Handle::new(name))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Handles::new(handles).ok_or("no one")?)
}

#[test]
fn setting_a_detail_rewrites_its_value_in_place() -> TestResult {
    let edits = edit::set(
        &one("roadmap\n[ ] a\n  est 2026-11\n  dri @ann\n  note Keep me.\n"),
        &id("a")?,
        &Setting::Dri(people(&["@ann", "@bo"])?),
    )?;
    assert_eq!(
        texts(edits),
        [(
            0,
            String::from("roadmap\n[ ] a\n  est 2026-11\n  dri @ann, @bo\n  note Keep me.\n")
        )]
    );
    Ok(())
}

/// The detail stays with the document that says it: a private `extend`
/// keeps its DRI private.
#[test]
fn setting_a_detail_another_document_says_edits_that_one() -> TestResult {
    let sources = [
        Source {
            name: "plan".into(),
            text: "roadmap\n[ ] a\n".into(),
        },
        Source {
            name: "private".into(),
            text: "roadmap\nextend a\n  dri @ann\n".into(),
        },
    ];
    let edits = edit::set(&sources, &id("a")?, &Setting::Dri(people(&["@bo"])?))?;
    assert_eq!(
        texts(edits),
        [(1, String::from("roadmap\nextend a\n  dri @bo\n"))]
    );
    Ok(())
}

#[test]
fn setting_a_new_detail_puts_it_at_the_top_of_the_block() -> TestResult {
    let plan =
        one("roadmap\ntrack t\n    [ ] a [A]\n        needs b\n    [ ] b\nmilestone m\n  a\n");
    let est = edit::set(&plan, &id("b")?, &Setting::Est(Date::new("2026-Q4")?))?;
    assert_eq!(
        texts(est),
        [(
            0,
            String::from(
                "roadmap\ntrack t\n    [ ] a [A]\n        needs b\n    [ ] b\n      est 2026-Q4\nmilestone m\n  a\n"
            )
        )],
        "a block with nothing in it yet gets the usual step in"
    );
    let target = edit::set(&plan, &id("m")?, &Setting::Target(Date::new("EOY26")?))?;
    assert_eq!(
        texts(target),
        [(
            0,
            String::from(
                "roadmap\ntrack t\n    [ ] a [A]\n        needs b\n    [ ] b\nmilestone m\n  target EOY26\n  a\n"
            )
        )],
        "a milestone takes a target"
    );
    Ok(())
}

#[test]
fn setting_what_it_already_says_changes_nothing() -> TestResult {
    let plan = one("roadmap\n[ ] a\n  target 2026-Q4\n");
    assert!(edit::set(&plan, &id("a")?, &Setting::Target(Date::new("2026-Q4")?))?.is_empty());
    Ok(())
}

#[test]
fn only_a_target_goes_on_a_milestone() -> TestResult {
    let plan = one("roadmap\n[ ] a\nmilestone m\n  a\n");
    assert_eq!(
        edit::set(&plan, &id("m")?, &Setting::Dri(people(&["@ann"])?)),
        Err(EditError::NotATask(id("m")?))
    );
    assert_eq!(
        edit::set(&plan, &id("z")?, &Setting::Est(Date::new("2026")?)),
        Err(EditError::Unknown(id("z")?))
    );
    Ok(())
}
