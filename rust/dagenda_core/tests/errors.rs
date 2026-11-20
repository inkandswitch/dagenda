//! Error messages for the mistakes people actually make. The messages are
//! part of the UX, so they're pinned here word for word, with their
//! positions.

/// `line:column: message` for `source`'s error, or `parsed`.
fn error(source: &str) -> String {
    match dagenda_core::parse(source) {
        Ok(_) => String::from("parsed"),
        Err(e) => {
            let (line, column) = e.span().line_col(source);
            format!("{line}:{column}: {e}")
        }
    }
}

/// `line:column: message` for every error in `source`, in order.
fn errors(source: &str) -> Vec<String> {
    let at = |span: dagenda_core::span::Span, message: String| {
        let (line, column) = span.line_col(source);
        format!("{line}:{column}: {message}")
    };
    match dagenda_core::parse(source) {
        Ok(_) => Vec::new(),
        Err(dagenda_core::Error::Parse(errors)) => {
            errors.iter().map(|e| at(e.span(), e.to_string())).collect()
        }
        Err(dagenda_core::Error::Resolve(errors)) => errors
            .iter()
            .map(|e| at(e.location().span, e.to_string()))
            .collect(),
    }
}

/// One run says everything that's wrong: the first mistake in each
/// top-level block (the rest of a broken block is skipped, so one mistake
/// doesn't set off others).
#[test]
fn every_malformed_block_is_reported_at_once() {
    let source = "roadmap
track a
  [q] x
  [?] fine
track b
\t[ ] y
[ ] z
  est 2w
  dri nobody
milestone m
  z";
    assert_eq!(
        errors(source),
        [
            "3:3: unknown status box `[q]`; use `[ ]`, `[>]`, `[x]`, `[-]`, `[?]`, `[!]`, `[*]`, or `[~]`",
            "6:1: indent with spaces, not tabs",
            "8:7: expected a date like `2026-11-19` or `2026-Q4`; a duration doesn't say when it lands",
        ]
    );
}

/// Every id that doesn't resolve is reported, in source order, along with
/// duplicates and references to the wrong kind of thing.
#[test]
fn every_resolve_error_is_reported_at_once() {
    let source = "roadmap
track t
[ ] a
  needs ghost
[ ] b
  needs t, phantom
[ ] a
milestone m
  spectre";
    assert_eq!(
        errors(source),
        [
            "4:9: nothing in the plan is called `ghost`",
            "6:9: `t` is a track, but `needs` points at tasks and milestones",
            "6:12: nothing in the plan is called `phantom`",
            "7:5: `a` is defined more than once",
            "9:3: nothing in the plan is called `spectre`",
        ]
    );
}

/// A loop is only reported once everything else resolves, so a typo can't
/// pass for one.
#[test]
fn a_loop_waits_until_every_id_resolves() {
    let looped = "roadmap\n[ ] a\n  needs b\n[ ] b\n  needs a, ghost";
    assert_eq!(
        errors(looped),
        ["5:12: nothing in the plan is called `ghost`"]
    );
    assert_eq!(
        errors(&looped.replace(", ghost", "")),
        ["3:9: these need each other, so none can finish: `a` → `b` → `a`"]
    );
}

#[test]
fn track_colours_say_what_to_do() {
    for (source, expected) in [
        (
            "roadmap\ntrack t\n  colour red",
            "3:10: there's no colour `red`; use teal, indigo, rose, ochre, sage, plum, sky, or terracotta, or a hex colour like `#2a9d8f`",
        ),
        (
            "roadmap\ntrack t\n  colour #12345",
            "3:10: a hex colour is `#` and six hex digits, as in `#2a9d8f`",
        ),
        (
            "roadmap\n[ ] a\n  colour teal",
            "3:3: `colour` can't go in a task",
        ),
        (
            "roadmap\ntrack t\n  colour teal\n  color sky",
            "4:9: a track can only have one `colour`",
        ),
        (
            "roadmap\ntrack t\n  colour",
            "3:3: expected a value after `colour`",
        ),
        ("roadmap\ntrack t\n  color Teal", "parsed"),
    ] {
        assert_eq!(error(source), expected, "{source:?}");
    }
}

#[test]
fn nice_to_haves_say_what_to_do() {
    for (source, expected) in [
        (
            "roadmap\n[ ] a\nmilestone m\n  a\n  ---\n  b",
            "5:3: `---` is now `nice-to-have`: write that, and indent the nice-to-haves under it",
        ),
        (
            "roadmap\n[ ] a\nmilestone m\n  nice-to-have a",
            "4:16: list the nice-to-haves on their own lines, indented under `nice-to-have`",
        ),
        (
            "roadmap\n[ ] a\n[ ] b\nmilestone m\n  nice-to-have\n    a\n  nice-to-have\n    b",
            "7:3: a milestone has only one `nice-to-have`; list them all under it",
        ),
        (
            "roadmap\n[ ] a\nmilestone m\n  a\n  nice-to-have\n    a",
            "6:5: `a` is already listed in this milestone",
        ),
        (
            "roadmap\nmilestone m\n  nice-to-have\n    target 2026",
            "4:5: `target` can't go in a milestone's nice-to-haves",
        ),
        (
            "roadmap\nnice-to-have\n  a",
            "2:1: `nice-to-have` can't go at the top level",
        ),
        (
            "roadmap\n[ ] nice-to-have",
            "2:5: `nice-to-have` is a keyword, so it can't be an id",
        ),
    ] {
        assert_eq!(error(source), expected, "{source:?}");
    }
}

/// A person is their handle and their picture; a task's or track's people
/// go on one `dri` line, each once.
#[test]
fn people_say_what_to_do() {
    for (source, expected) in [
        (
            "roadmap\n@alice",
            "2:1: give `@alice` a picture on the line below, indented, as in `avatar gh:octocat`",
        ),
        (
            "roadmap\n@alice [Alice]",
            "2:8: a person's line is just their handle; indent their `avatar` on the line below",
        ),
        ("roadmap\n@", "2:1: expected a name after `@`"),
        (
            "roadmap\n@alice\n  avatar octocat",
            "3:10: expected `gh:` and a GitHub login, as in `avatar gh:octocat`, or a picture's address",
        ),
        (
            "roadmap\n@alice\n  avatar gh:",
            "3:10: expected a GitHub login (letters, digits, `-`) after `gh:`, as in `gh:octocat`",
        ),
        (
            "roadmap\n@alice\n  avatar https://",
            "3:10: expected a host after `https://`, as in `https://example.com`",
        ),
        (
            "roadmap\n@alice\n  avatar gh:a\n  avatar gh:b",
            "4:10: a person can only have one `avatar`",
        ),
        (
            "roadmap\n@alice\n  dri @bob",
            "3:3: `dri` can't go in a person",
        ),
        (
            "roadmap\n[ ] a\n  avatar gh:a",
            "3:3: `avatar` can't go in a task",
        ),
        (
            "roadmap\ntrack t\n  @alice\n    avatar gh:a",
            "3:3: a person can't go in a track",
        ),
        (
            "roadmap\n[ ] a\n  dri @a @b",
            "3:7: separate the people with commas, as in `dri @alice, @bob`",
        ),
        (
            "roadmap\n[ ] a\n  dri @a,, @b",
            "3:10: expected a person between the commas",
        ),
        (
            "roadmap\n[ ] a\n  dri @a, @a",
            "3:11: `@a` is already listed",
        ),
        (
            "roadmap\n@a\n  avatar gh:a\n@a\n  avatar gh:b",
            "4:1: `@a` is defined more than once",
        ),
        (
            "roadmap\n[ ] avatar",
            "2:5: `avatar` is a keyword, so it can't be an id",
        ),
    ] {
        assert_eq!(error(source), expected, "{source:?}");
    }
}

/// A task is its status box and its id: say so when either is missing, or
/// when `task` is still written between them.
#[test]
fn task_lines_say_what_to_do() {
    for (source, expected) in [
        (
            "roadmap\ntask x",
            "2:1: a task is a status box and an id, as in `[ ] parser`",
        ),
        (
            "roadmap\n[q] x",
            "2:1: unknown status box `[q]`; use `[ ]`, `[>]`, `[x]`, `[-]`, `[?]`, `[!]`, `[*]`, or `[~]`",
        ),
        (
            "roadmap\n[x]",
            "2:4: expected an id after the status box, as in `[ ] parser`",
        ),
        (
            "roadmap\n[ ] [Label]",
            "2:4: expected an id after the status box, as in `[ ] parser`",
        ),
        (
            "roadmap\n[ ] task x",
            "2:5: `task` is no longer written: a task is just its status box and id, as in `[ ] parser`",
        ),
    ] {
        assert_eq!(error(source), expected, "{source:?}");
    }
}

#[test]
fn parse_errors_say_what_to_do() {
    for (source, expected) in [
        ("", "1:1: missing header; start with `roadmap`"),
        (
            "track x",
            "1:1: expected `roadmap` on the first line, not \"track x\"",
        ),
        ("roadmap\n\t[ ] x", "2:1: indent with spaces, not tabs"),
        (
            "roadmap\ntrack a\n    [ ] x\n  [ ] y",
            "4:3: this line's indent doesn't line up with the lines above it",
        ),
        (
            "roadmap\nmilestone m\n  a\n    b",
            "4:5: this line is indented further than the one above it, which can't hold anything",
        ),
        (
            "roadmap\ntrack a [A",
            "2:11: expected `]` to close the label",
        ),
        (
            "roadmap\ntrack a [A] b",
            "2:12: unexpected text after the label",
        ),
        ("roadmap\ntrack [A]", "2:7: expected an id after `track`"),
        (
            "roadmap\ntrack note",
            "2:7: `note` is a keyword, so it can't be an id",
        ),
        (
            "roadmap\ntrack 1a",
            "2:7: an id starts with a letter, not '1'",
        ),
        (
            "roadmap\nextend x [X]",
            "2:10: `extend` takes no label: the task it extends already has one",
        ),
        (
            "roadmap\ntitle A\ntitle B",
            "3:1: a document can only have one title",
        ),
        (
            "roadmap\n[ ] x\n  dri brooke",
            "3:7: a person is written with `@`, as in `@alice`",
        ),
        (
            "roadmap\n[ ] x\n  pr #12",
            "3:6: expected a GitHub reference like `gh:owner/repo#123`",
        ),
        (
            "roadmap\n[ ] x\n  est 2026-11\n  est 2026-12",
            "4:7: a task can only have one `est`",
        ),
        (
            "roadmap\ntrack a\n  est 2026-11",
            "3:3: `est` can't go in a track",
        ),
        (
            "roadmap\nmilestone m\n  [ ] a",
            "3:3: a task can't go in a milestone",
        ),
        (
            "roadmap\nmilestone1\n  myTask",
            "2:1: `milestone1` on its own means nothing here; did you mean `milestone milestone1`?",
        ),
        (
            "roadmap\n[ ] x\n  needs a b",
            "3:9: separate the ids with commas, as in `needs a, b`",
        ),
        (
            "roadmap\n[ ] x\n  needs a,",
            "3:11: expected an id between the commas",
        ),
        (
            "roadmap\n[ ] x\n  note",
            "3:3: write the note after `note`, or indent it on the lines below",
        ),
        (
            "roadmap\n[ ] a\nmilestone m\n  a\n  a",
            "5:3: `a` is already listed in this milestone",
        ),
        (
            "roadmap\nfoo bar",
            "2:1: unknown line starting `foo`; expected `track`, a task (`[ ] id`), `milestone`, `extend`, a person (`@alice`), or a detail like `needs`",
        ),
    ] {
        assert_eq!(error(source), expected, "{source:?}");
    }
}

#[test]
fn resolve_errors_name_the_ids() {
    for (source, expected) in [
        (
            "roadmap\n[ ] a\ntrack a",
            "3:7: `a` is defined more than once",
        ),
        (
            "roadmap\n[ ] a\n  needs b",
            "3:9: nothing in the plan is called `b`",
        ),
        (
            "roadmap\ntrack t\n[ ] a\n  needs t",
            "4:9: `t` is a track, but `needs` points at tasks and milestones",
        ),
        (
            "roadmap\ntrack t\nmilestone m\n  t",
            "4:3: `t` is a track, but a milestone lists tasks and milestones",
        ),
        (
            "roadmap\n[ ] a\n  needs b\n[ ] b\n  needs a",
            "3:9: these need each other, so none can finish: `a` → `b` → `a`",
        ),
        (
            "roadmap\n[ ] a\n  needs a",
            "3:9: `a` needs itself, so it can never finish",
        ),
        (
            "roadmap\n[ ] a\n  [ ] b\n    needs a",
            "3:7: these need each other, so none can finish: `a` → `b` → `a`",
        ),
        (
            "roadmap\n[ ] a\n  needs m\nmilestone m\n  a",
            "3:9: these need each other, so none can finish: `a` → `m` → `a`",
        ),
        (
            "roadmap\nextend x\n  dri @a",
            "2:8: nothing in the plan is called `x`",
        ),
        (
            "roadmap\n[ ] a\nmilestone m\n  a\nextend m\n  dri @a",
            "5:8: `m` is a milestone, but `extend` adds detail to tasks",
        ),
        (
            "roadmap\n[ ] a\n  dri @x\nextend a\n  dri @y",
            "5:7: `dri` is already set for `a`; each detail has one owner",
        ),
    ] {
        assert_eq!(error(source), expected, "{source:?}");
    }
}

#[test]
fn dates_and_links_say_what_to_do() {
    for (source, expected) in [
        (
            "roadmap\n[ ] x\n  est 2w",
            "3:7: expected a date like `2026-11-19` or `2026-Q4`; a duration doesn't say when it lands",
        ),
        (
            "roadmap\n[ ] x\n  est next week",
            "3:7: expected a date like `2026-11-19`, `2026-11`, `2026-Q4`, `2026`, or `EOY26`",
        ),
        (
            "roadmap\n[ ] x\n  target 2026-13",
            "3:10: there's no month 13; months run from 01 to 12",
        ),
        (
            "roadmap\n[ ] x\n  est 2027-02-29",
            "3:7: 2027-02 has no day 29",
        ),
        (
            "roadmap\nmilestone m\n  target 2026-Q5",
            "3:10: quarters run from Q1 to Q4",
        ),
        (
            "roadmap\n[ ] x\n  link example.com/design",
            "3:8: expected a web address starting with `https://`",
        ),
        (
            "roadmap\n[ ] x\n  link https://example.com [Design",
            "3:35: expected `]` to close the label",
        ),
        (
            "roadmap\nmilestone m\n  link https://example.com",
            "3:3: `link` can't go in a milestone",
        ),
    ] {
        assert_eq!(error(source), expected, "{source:?}");
    }
}

#[test]
fn editors_line_endings_and_marks_are_fine() {
    for source in [
        "roadmap\r\n[ ] x\r\n  est 2026-11-19\r\n",
        "\u{feff}roadmap\n[ ] x\n",
        "  roadmap\n    [ ] x\n",
        "roadmap\n\n// a comment\n[ ] x\n   \n",
    ] {
        assert_eq!(error(source), "parsed", "{source:?}");
    }
}
