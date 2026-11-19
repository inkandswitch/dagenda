//! `dagenda check`'s report: what it says, and where.

use dagenda_cli::check::{Severity, Source, check};

fn source(name: &str, text: &str) -> Source {
    Source {
        name: name.into(),
        text: text.into(),
    }
}

fn report(sources: &[Source]) -> Vec<String> {
    check(sources).1.iter().map(ToString::to_string).collect()
}

#[test]
fn a_clean_plan_has_nothing_to_say() {
    let (plan, problems) = check(&[source("a.dagenda", "roadmap\n[ ] task x\n")]);
    assert!(plan.is_some());
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn every_document_that_fails_to_parse_is_reported() {
    assert_eq!(
        report(&[
            source("a.dagenda", "roadmap\ntask x\n"),
            source("b.dagenda", ""),
        ]),
        [
            "a.dagenda:2:1: error: a task starts with a status box, as in `[ ] task …`",
            "b.dagenda:1:1: error: missing header; start with `roadmap`",
        ]
    );
}

#[test]
fn documents_checked_together_are_one_plan() {
    let public = source("public.dagenda", "roadmap\n[ ] task a\n");
    let private = source(
        "private.dagenda",
        "roadmap\nextend a\n  dri @alice\n[ ] task b\n  needs a\n",
    );
    let (plan, problems) = check(&[public, private]);
    assert!(problems.is_empty(), "{problems:?}");
    assert!(plan.is_some());
}

#[test]
fn a_duplicate_says_where_the_first_one_is() {
    assert_eq!(
        report(&[
            source("a.dagenda", "roadmap\n[ ] task x\n"),
            source("b.dagenda", "roadmap\ntrack x\n"),
        ]),
        ["b.dagenda:2:7: error: `x` is defined more than once (first at a.dagenda:2:1)"]
    );
}

#[test]
fn warnings_come_with_the_plan() {
    let (plan, problems) = check(&[source(
        "w.dagenda",
        "roadmap\n[~] task x\nmilestone m\n  x\n",
    )]);
    assert!(plan.is_some());
    let found: Vec<_> = problems
        .iter()
        .map(|problem| (problem.severity, problem.line, problem.column))
        .collect();
    assert_eq!(found, [(Severity::Warning, 4, 3)]);
}
