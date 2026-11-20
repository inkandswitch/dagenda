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

#[test]
fn nice_to_haves_say_what_to_do() {
    for (source, expected) in [
        (
            "roadmap\n[ ] task a\nmilestone m\n  a\n  ---\n  b",
            "5:3: `---` is now `nice-to-have`: write that, and indent the nice-to-haves under it",
        ),
        (
            "roadmap\n[ ] task a\nmilestone m\n  nice-to-have a",
            "4:16: list the nice-to-haves on their own lines, indented under `nice-to-have`",
        ),
        (
            "roadmap\n[ ] task a\n[ ] task b\nmilestone m\n  nice-to-have\n    a\n  nice-to-have\n    b",
            "7:3: a milestone has only one `nice-to-have`; list them all under it",
        ),
        (
            "roadmap\n[ ] task a\nmilestone m\n  a\n  nice-to-have\n    a",
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
            "roadmap\n[ ] task nice-to-have",
            "2:10: `nice-to-have` is a keyword, so it can't be an id",
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
        (
            "roadmap\ntask x",
            "2:1: a task starts with a status box, as in `[ ] task …`",
        ),
        (
            "roadmap\n[q] task x",
            "2:1: unknown status box `[q]`; use `[ ]`, `[>]`, `[x]`, `[-]`, `[?]`, `[!]`, `[*]`, or `[~]`",
        ),
        (
            "roadmap\n[x] tusk x",
            "2:5: expected `task` after the status box",
        ),
        ("roadmap\n\t[ ] task x", "2:1: indent with spaces, not tabs"),
        (
            "roadmap\ntrack a\n    [ ] task x\n  [ ] task y",
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
            "roadmap\n[ ] task x\n  dri brooke",
            "3:7: a person is written with `@`, as in `@alice`",
        ),
        (
            "roadmap\n[ ] task x\n  pr #12",
            "3:6: expected a GitHub reference like `gh:owner/repo#123`",
        ),
        (
            "roadmap\n[ ] task x\n  est 2026-11\n  est 2026-12",
            "4:7: a task can only have one `est`",
        ),
        (
            "roadmap\ntrack a\n  est 2026-11",
            "3:3: `est` can't go in a track",
        ),
        (
            "roadmap\nmilestone m\n  [ ] task a",
            "3:3: a task can't go in a milestone",
        ),
        (
            "roadmap\nmilestone1\n  myTask",
            "2:1: `milestone1` on its own means nothing here; did you mean `milestone milestone1`?",
        ),
        (
            "roadmap\n[ ] task x\n  needs a b",
            "3:9: separate the ids with commas, as in `needs a, b`",
        ),
        (
            "roadmap\n[ ] task x\n  needs a,",
            "3:11: expected an id between the commas",
        ),
        (
            "roadmap\n[ ] task x\n  note",
            "3:3: write the note after `note`, or indent it on the lines below",
        ),
        (
            "roadmap\n[ ] task a\nmilestone m\n  a\n  a",
            "5:3: `a` is already listed in this milestone",
        ),
        (
            "roadmap\nfoo bar",
            "2:1: unknown line starting `foo`; expected `track`, `[ ] task`, `milestone`, `extend`, or a detail like `needs`",
        ),
    ] {
        assert_eq!(error(source), expected, "{source:?}");
    }
}

#[test]
fn resolve_errors_name_the_ids() {
    for (source, expected) in [
        (
            "roadmap\n[ ] task a\ntrack a",
            "3:7: `a` is defined more than once",
        ),
        (
            "roadmap\n[ ] task a\n  needs b",
            "3:9: nothing in the plan is called `b`",
        ),
        (
            "roadmap\ntrack t\n[ ] task a\n  needs t",
            "4:9: `t` is a track, but `needs` points at tasks and milestones",
        ),
        (
            "roadmap\ntrack t\nmilestone m\n  t",
            "4:3: `t` is a track, but a milestone lists tasks and milestones",
        ),
        (
            "roadmap\n[ ] task a\n  needs b\n[ ] task b\n  needs a",
            "3:9: these need each other, so none can finish: `a` → `b` → `a`",
        ),
        (
            "roadmap\n[ ] task a\n  needs a",
            "3:9: `a` needs itself, so it can never finish",
        ),
        (
            "roadmap\n[ ] task a\n  [ ] task b\n    needs a",
            "3:12: these need each other, so none can finish: `a` → `b` → `a`",
        ),
        (
            "roadmap\n[ ] task a\n  needs m\nmilestone m\n  a",
            "3:9: these need each other, so none can finish: `a` → `m` → `a`",
        ),
        (
            "roadmap\nextend x\n  dri @a",
            "2:8: nothing in the plan is called `x`",
        ),
        (
            "roadmap\n[ ] task a\nmilestone m\n  a\nextend m\n  dri @a",
            "5:8: `m` is a milestone, but `extend` adds detail to tasks",
        ),
        (
            "roadmap\n[ ] task a\n  dri @x\nextend a\n  dri @y",
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
            "roadmap\n[ ] task x\n  est 2w",
            "3:7: expected a date like `2026-11-19` or `2026-Q4`; a duration doesn't say when it lands",
        ),
        (
            "roadmap\n[ ] task x\n  est next week",
            "3:7: expected a date like `2026-11-19`, `2026-11`, `2026-Q4`, `2026`, or `EOY26`",
        ),
        (
            "roadmap\n[ ] task x\n  target 2026-13",
            "3:10: there's no month 13; months run from 01 to 12",
        ),
        (
            "roadmap\n[ ] task x\n  est 2027-02-29",
            "3:7: 2027-02 has no day 29",
        ),
        (
            "roadmap\nmilestone m\n  target 2026-Q5",
            "3:10: quarters run from Q1 to Q4",
        ),
        (
            "roadmap\n[ ] task x\n  link example.com/design",
            "3:8: expected a web address starting with `https://`",
        ),
        (
            "roadmap\n[ ] task x\n  link https://example.com [Design",
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
        "roadmap\r\n[ ] task x\r\n  est 2026-11-19\r\n",
        "\u{feff}roadmap\n[ ] task x\n",
        "  roadmap\n    [ ] task x\n",
        "roadmap\n\n// a comment\n[ ] task x\n   \n",
    ] {
        assert_eq!(error(source), "parsed", "{source:?}");
    }
}
