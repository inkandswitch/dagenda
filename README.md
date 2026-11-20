# dagenda

> [!WARNING]
> Early. The notation parses and resolves across documents, `dagenda check` reports what's wrong with a plan, `dagenda render` draws it as text or as a tech tree, the preview draws it in the browser as you type, and the collaborative editor lets a team edit a plan's documents together.

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

The indentation says where each thing lives: a track holds tasks, and a task holds its details. The plan is one graph of what needs what: `needs` lines, by id, and milestone members. A task written under another is shorthand for one that task needs, in the same track, so a big task can be broken down in place. A milestone is reached when everything above its cut line (`---`) is done; what's below is nice to have.

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
    est 2026-12
    link https://github.com/owner/repo/issues/12#issuecomment-345
```

`est` and `target` take a date at whatever precision you know: `2026-11-19`, `2026-11`, `2026-Q4`, `2026`, or `EOY26`. A duration like `2w` is an error, since it doesn't say when anything lands.

The full set of forms, the grammar, and the indentation rules are in the [`dagenda_core`](rust/dagenda_core/) crate docs.

## Checking and Rendering

```console
$ dagenda check ROADMAP.dagenda
$ dagenda check public.dagenda private.dagenda --deny-warnings
$ dagenda render ROADMAP.dagenda                 # milestones and tracks, as text
$ dagenda render ROADMAP.dagenda -o roadmap.svg  # the tech tree
```

Files given together are one plan, so they can refer to each other's ids. Problems come out as `path:line:column: severity: message`. Errors are things that don't resolve (an unknown id, a task that needs itself); warnings are things that resolve but don't mean what they look like (a dropped must-have, a done task that needs unfinished work).

The tech tree grows upward, like a game's: everything a task needs sits below it, and each track is a branch in its own colour, with its name at the top. Milestones are eras. Each one is a dashed line across the tree with its gate at the end, and the work it needs sits in the band under it. Nice-to-haves sit in the era that wants them. Work no milestone needs or wants, done or not, sits in a shaded layer of its own at the top, "Not on a milestone", so no era claims it. A card says who's on it, when it's expected, and links to its pull requests, issues, and anything else (a comment, a doc); a track's DRI is on its banner. A milestone's gate counts all the work behind it, not just its own members, and a waiting task says how much of what it needs is done ("2 of 4 done below"). Lines a longer path already implies are left out. Here's [`tour.dagenda`](rust/dagenda_core/tests/golden/tour.dagenda), which uses every form:

![A tech tree that grows upward. At the bottom, in the Core branch, a done Parser (with a check) and the two tasks written under Resolver lead up to the in-progress Resolver, under the dashed line of "It parses" (1 of 2, EOY26). Above that line, in the nested Views branch, Tech tree waits on Resolver and on It parses, and Skill tree, a nice-to-have, has a dashed line to "First picture" (0 of 2). In the shaded layer on top, not on a milestone, sit a research task, one that needs attention, a deferred one, a dropped one, and a task outside any track](rust/dagenda_core/tests/golden/tour.svg)

## In the Browser

The preview is rain's, for roadmaps: type a plan on one side, and its tech tree redraws on the other. Hover a box to light up everything it needs and everything waiting on it; click one to jump to its line. Text that doesn't parse keeps the last good picture up, with the problem marked. It's single-user, with no sync, and keeps the source in the browser.

```console
$ nix run .#preview   # http://127.0.0.1:8080, rebuilding the Wasm when Rust changes
$ nix run .#site      # the preview as a static site in _site/, as GitHub Pages deploys it
```

The picture is [`<dagenda-view>`](javascript/dagenda-view/), a web component any page can use; everything it draws comes from the Rust core compiled to Wasm (about 180 KB, 75 KB gzipped).

## Edit Together

Both pages edit the picture too: click a card's badge to set its status, drag from a badge onto a card to add a need, drag a card into another milestone's band to re-scope it, or drag a gate to reorder the eras. Each edit lands in the text as the smallest splice, so text and picture stay one.

The [collaborative editor](javascript/editor/) is the preview with sync, as in rain. A plan's documents (public, private, …) live in Automerge, synced over [Subduction], and a _workspace_ document lists them: a tab per document, with presence, per-person undo, and offline use, and one picture drawn from all of them. The preview's _Collaborate_ button opens its text there as a new shared plan.

```console
$ nix run .#editor    # http://localhost:5173; share the URL it gives you
```

Until Keyhive, a private document is one whose URL isn't linked from anything public.

## Planned

- _Two more views._ A Gantt chart, and a skill tree in the spirit of Final Fantasy X's Sphere Grid.
- _More picture edits._ Inserting a task, moving one to another track, and removing a need from the picture.
- _A public site._ Static, built from the plan files in the repo and from snapshots of public Automerge documents.
- _Links to the work._ GitHub issues and pull requests, resolved by [sans-effort] routines.
- _Access control with [Keyhive]_, later.

## Layout

```
rust/
├── dagenda_cli/    the `dagenda` command: check and render
├── dagenda_core/   the notation, its parser, the plan model, and its views (no_std)
└── dagenda_wasm/   JS bindings
javascript/
├── dagenda-view/   <dagenda-view> web component
├── editor/         the collaborative editor (Vite, CodeMirror, automerge-repo over Subduction)
└── preview/        the preview page, its page shell (shared with the editor), and its hot-reloading dev server
```

## Development

```console
$ nix develop                       # toolchain and a command menu
$ cargo test --workspace
$ nix run .#dagenda -- check plan.dagenda
$ nix run .#preview                 # the preview, with hot reload
$ nix run .#editor                  # the collaborative editor, with Vite
$ nix run .#ci-clippy               # also: ci-bolero, ci-deny, ci-doc, ci-editor, ci-fmt, ci-js, ci-msrv, ci-mutants, ci-no-std, ci-test, ci-typos
```

Every CI job is one of these flake apps, so a local run checks exactly what CI does. The goldens in `rust/dagenda_core/tests/golden/` pin everything the core derives from a document; after an intended change, review the diff and re-bless with `BLESS=1 cargo test --test golden`.

## License

Licensed under either of [Apache License, Version 2.0](./LICENSE-APACHE) or [MIT license](./LICENSE-MIT) at your option.

[Keyhive]: https://github.com/inkandswitch/keyhive
[rain]: https://github.com/inkandswitch/rain
[sans-effort]: https://github.com/inkandswitch/sans-effort
[Subduction]: https://github.com/inkandswitch/subduction
