# dagenda

> [!WARNING]
> Early. The notation parses and resolves across documents, `dagenda check` reports what's wrong with a plan, and `dagenda render` draws it as text or as a tech tree. The web views come next.

Roadmaps as dependency graphs, for the Ink & Switch Department of Data: Automerge, Subduction, Keyhive, and their neighbours. The world should be able to see the milestones and the features that lead to them. The day-to-day tasks, who's on them, and the notes stay with the team.

## The Notation

```
roadmap
title Department of Data

track sync [Sync]
  [-] task newRepo [New repo architecture]
    needs fragments
    dri @alice
    pr gh:owner/repo#123
  [x] task fragments [Fragments]
  [ ] task docTypes [Custom doc types]

milestone repo3 [Repo 3]
  target EOY26
  newRepo
  ---
  docTypes
```

The indentation is a tree that says where each thing lives: a track holds tasks, and a task holds its details and subtasks. `needs` lines and milestone members make the dependency graph on top of it, by id. A milestone is reached when everything above its cut line (`---`) is done; what's below is nice to have.

Every task starts with a status box, and only `[x]` counts as finished:

| Box   | Status                       |
|-------|------------------------------|
| `[ ]` | Not started                  |
| `[>]` | Deferred                     |
| `[x]` | Done                         |
| `[-]` | In progress                  |
| `[?]` | Idea, or needs clarification |
| `[!]` | Needs attention              |
| `[*]` | Research                     |
| `[~]` | Dropped                      |

Which document a task is written in decides who can read it, so there's no `private` flag. A private plan adds to a public task with `extend`:

```
roadmap

extend newRepo
  dri @bob
  [ ] task inbox [Document inbox]
    est 1w
```

The full set of forms, the grammar, and the indentation rules are in the [`dagenda_core`](rust/dagenda_core/) crate docs.

## Checking and Rendering

```console
$ dagenda check ROADMAP.dagenda
$ dagenda check public.dagenda private.dagenda --deny-warnings
$ dagenda render ROADMAP.dagenda                 # milestones and tracks, as text
$ dagenda render ROADMAP.dagenda -o roadmap.svg  # the tech tree
```

Files given together are one plan, so they can refer to each other's ids. Problems come out as `path:line:column: severity: message`. Errors are things that don't resolve (an unknown id, a task that needs itself); warnings are things that resolve but don't mean what they look like (a dropped must-have, a done task with open subtasks).

The tech tree runs left to right: everything a task needs sits in a column to its left. Milestones take the top lane, each track a lane below, and subtasks fold into their task. Lines a longer path already implies are left out. Here's [`tour.dagenda`](rust/dagenda_core/tests/golden/tour.dagenda), which uses every form:

<!-- Regenerate with: BLESS=1 cargo test --test golden -->
![A tech tree: a Milestones lane with "It parses" (1 of 2, EOY26) and "First picture" (0 of 2); a Core lane where a done Parser leads to an in-progress Resolver (1 of 2 subtasks); a nested Views lane whose Tech tree waits on Resolver and on It parses, beside an idea, a research task, one that needs attention, a deferred one, and a dropped one; and a lane for a task outside any track](rust/dagenda_core/tests/golden/tour.svg)

## Planned

- _Two more views._ A Gantt chart, and a skill tree in the spirit of Final Fantasy X's Sphere Grid.
- _In the browser._ A preview in rain's style: type on one side, and the picture redraws on the other.
- _Text and picture in step._ A WYSIWYG editor over the same source: change either and the other follows, as in [rain].
- _A public site and a collaborative editor._ The site is static. The editor keeps its documents in Automerge, synced over [Subduction].
- _Links to the work._ GitHub issues and pull requests, resolved by [sans-effort] routines.
- _Access control with [Keyhive]_, later.

## Layout

```
rust/
├── dagenda_cli/    the `dagenda` command: check and render
└── dagenda_core/   the notation, its parser, the plan model, and its views (no_std)
```

## Development

```console
$ nix develop                       # toolchain and a command menu
$ cargo test --workspace
$ nix run .#dagenda -- check plan.dagenda
$ nix run .#ci-clippy               # also: ci-bolero, ci-deny, ci-doc, ci-fmt, ci-msrv, ci-mutants, ci-no-std, ci-test, ci-typos
```

Every CI job is one of these flake apps, so a local run checks exactly what CI does. The goldens in `rust/dagenda_core/tests/golden/` pin everything the core derives from a document; after an intended change, review the diff and re-bless with `BLESS=1 cargo test --test golden`.

## License

Licensed under either of [Apache License, Version 2.0](./LICENSE-APACHE) or [MIT license](./LICENSE-MIT) at your option.

[Keyhive]: https://github.com/inkandswitch/keyhive
[rain]: https://github.com/inkandswitch/rain
[sans-effort]: https://github.com/inkandswitch/sans-effort
[Subduction]: https://github.com/inkandswitch/subduction
