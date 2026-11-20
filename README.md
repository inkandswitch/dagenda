# dagenda

> [!WARNING]
> Early. The notation parses and resolves across documents, `dagenda check` reports what's wrong with a plan, `dagenda render` draws it as text or as a tech tree, the preview draws it in the browser as you type, and the collaborative editor lets a team edit a plan's documents together.

Roadmaps as dependency graphs. The world should be able to see the milestones and the features that lead to them. The day-to-day tasks, who's on them, and the notes stay with the team.

## The Notation

```
roadmap
title Product Roadmap

track sync [Sync]
  [-] engine [New sync engine]
    needs protocol
    dri @alice
    pr gh:owner/repo#123
  [x] protocol [Wire protocol]
  [ ] offline [Offline mode]

milestone v2 [Version 2]
  target EOY26
  engine
  nice-to-have
    offline
```

The indentation says where each thing lives: a track holds tasks, and a task holds its details. The plan is one graph of what needs what: `needs` lines, by id, and milestone members. A task written under another is shorthand for one that task needs, in the same track, so a big task can be broken down in place. A milestone is reached when its members are done; the ones indented under its `nice-to-have` are wanted, not required.

Every task starts with a status box, and only `[x]` counts as finished:

| Box   | Status                       | In the picture  |
|-------|------------------------------|-----------------|
| `[ ]` | Not started                  | ⚪              |
| `[>]` | Deferred                     | ⏸️              |
| `[x]` | Done                         | ✅              |
| `[-]` | In progress                  | 🚧, and a glow  |
| `[?]` | Idea, or needs clarification | 💡              |
| `[!]` | Needs attention              | ⚠️              |
| `[*]` | Research                     | 🔬              |
| `[~]` | Dropped                      | 🗑️              |

Which document a task is written in decides who can read it, so there's no `private` flag. A private plan adds to a public task with `extend`:

```
roadmap

extend engine
  note Waiting on the storage review; ask @bob before merging.
  [ ] inbox [Change inbox]
    dri @bob
    est 2026-12
    link https://github.com/owner/repo/issues/12#issuecomment-345
```

`est` and `target` take a date at whatever precision you know: `2026-11-19`, `2026-11`, `2026-Q4`, `2026`, or `EOY26`. A duration like `2w` is an error, since it doesn't say when anything lands.

A `dri` line can name several people: `dri @alice, @bob`. To put someone's face on their cards, give them a block of their own at the top level, with an `avatar`: `gh:` and a GitHub login for that account's picture, or the address of any picture.

```
@alice
  avatar gh:octocat

@bob
  avatar https://example.com/bob.png
```

Everyone on a task who has an avatar gets a round sticker down the right side of its card, hanging a little off the edge, where no lines run; when more people are on it than fit (up to four), the last sticker says how many more there are. Nobody gets a picture by default: only a person block with an `avatar` makes a sticker. In the preview and the editor, click someone's sticker, or their `@name` on a card, to see only what they're assigned to; click again, or press Escape, to see everything. People can be defined in a private plan, so their pictures stay with the team.

> [!NOTE]
> Browsers load the pictures when the SVG is part of the page (as in the preview and the editor) or opened on its own, but not when it's shown through an `<img>` tag, the way GitHub shows a README's images. There, each sticker shows the person's initial instead.

The full set of forms, the grammar, and the indentation rules are in the [`dagenda_core`](rust/dagenda_core/) crate docs.

## Checking and Rendering

```console
$ dagenda check ROADMAP.dagenda
$ dagenda check public.dagenda private.dagenda --deny-warnings
$ dagenda render ROADMAP.dagenda                 # milestones and tracks, as text
$ dagenda render ROADMAP.dagenda -o roadmap.svg  # the tech tree
```

Files given together are one plan, so they can refer to each other's ids. Problems come out as `path:line:column: severity: message`, all of them in one run: the first mistake in each malformed block, and every id that doesn't resolve. `render -o` takes the format from the extension (`.svg` or `.txt`) or from `--format`, refuses to guess at any other, and never writes over one of the plan's own files. Errors are things that don't resolve (an unknown id, a task that needs itself); warnings are things that resolve but don't mean what they look like (a dropped must-have, a done task that needs unfinished work).

The tech tree grows upward, like a game's: everything a task needs sits below it, and each track is a branch in its own colour, with its name at the top. A track takes the next colour in the palette, or the one it sets with `colour`: `teal`, `indigo`, `rose`, `ochre`, `sage`, `plum`, `sky`, `terracotta`, or any `#rrggbb`. Milestones are eras. Each one is a dashed line across the tree with its gate at the end, and the work it needs sits in the band under it. Nice-to-haves sit in the era that wants them. Work no milestone needs or wants, done or not, sits in a shaded layer of its own at the top, "Not on a milestone", so no era claims it. A card says who's on it, when it's expected, and links to its pull requests, issues, and anything else (a comment, a doc), with a sticker for each person on it who has an avatar; a track's DRIs are on its banner. A task in progress glows, and its badge turns, so it stands apart from one not started. A milestone's gate counts all the work behind it, not just its own members, and a waiting task says how much of what it needs is done ("2 of 4 done below"). Lines a longer path already implies are left out. Here's [`tour.dagenda`](rust/dagenda_core/tests/golden/tour.dagenda), which uses every form:

![A tech tree that grows upward. At the bottom, in the Core branch, a done Parser (with a check) and the two tasks written under Resolver lead up to the in-progress Resolver, with Alice's sticker on its side, under the dashed line of "It parses" (1 of 2, EOY26). Above that line, in the nested Views branch, Tech tree waits on Resolver and on It parses, and Skill tree, a nice-to-have, has a dashed line to "First picture" (0 of 2). In the shaded layer on top, not on a milestone, sit a research task, one that needs attention, a deferred one, a dropped one, and a task outside any track](rust/dagenda_core/tests/golden/tour.svg)

## In the Browser

The preview is a live editor for roadmaps: type a plan on one side, and its tech tree redraws on the other. Hover a box to light up everything it needs and everything waiting on it; click one to jump to its line. Text that doesn't parse keeps the last good picture up, with the problem marked. It's single-user, with no sync, and keeps the source in the browser.

```console
$ nix run .#preview   # http://127.0.0.1:8080, rebuilding the Wasm when Rust changes
$ nix run .#site      # the preview as a static site in _site/, as GitHub Pages deploys it
```

The picture is [`<dagenda-view>`](javascript/dagenda-view/), a web component any page can use; everything it draws comes from the Rust core compiled to Wasm (about 180 KB, 75 KB gzipped).

## Edit Together

Both pages edit the picture too: click a card's badge to set its status, drag from a badge onto a card to add a need, drag a card into another milestone's band to re-scope it, or drag a milestone's line to move its boundary over cards (or past another milestone's, to reorder them). Each edit lands in the text as the smallest splice, so text and picture stay one.

The [collaborative editor](javascript/editor/) is the preview with sync. A plan's documents (public, private, …) live in Automerge, synced over [Subduction], and a _workspace_ document lists them: a tab per document, with presence, per-person undo, and offline use, and one picture drawn from all of them. The preview's _Collaborate_ button opens its text there as a new shared plan.

```console
$ nix run .#editor    # http://localhost:5173; share the URL it gives you
```

A shared plan can be edited from the command line too, as files, alongside the people in the editor. `pull` writes each document to `<name>.dagenda`; `push` merges your edits with whatever was typed since, instead of overwriting it:

```console
$ nix run .#collab -- pull plan/ automerge:…   # the first time; the URL is kept in plan/workspace.url
$ nix run .#collab -- status plan/             # what has edits to push, or to pull
$ nix run .#collab -- push plan/ --as=claude   # named, so people know whose edits these are
$ nix run .#collab -- undo plan/               # take back the last push, keeping what's been typed since
$ nix run .#collab -- new plan/                # a new workspace from the .dagenda files there
```

It's built so nothing gets lost or broken: `pull` won't overwrite edits you haven't pushed, and `push` checks the plan with this repo's parser and with the deployed editor's, refusing if either finds errors (`--ahead-of-site` to push syntax the deployed editor doesn't know yet).

Until Keyhive, a private document is one whose URL isn't linked from anything public.

Agents get a [skill](.agents/skills/dagenda/SKILL.md) for all of this: the notation, checking and rendering, and editing a shared plan safely.

## Planned

- _Two more views._ A Gantt chart, and a skill tree: a board of linked nodes, where finishing one opens up the paths around it.
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
├── editor/         the collaborative editor (Vite, CodeMirror, automerge-repo over Subduction), and `cli/collab.mjs`
└── preview/        the preview page, its page shell (shared with the editor), and its hot-reloading dev server
```

## Development

```console
$ nix develop                       # toolchain and a command menu
$ cargo test --workspace
$ nix run .#dagenda -- check plan.dagenda
$ nix run .#preview                 # the preview, with hot reload
$ nix run .#editor                  # the collaborative editor, with Vite
$ nix run .#collab -- pull <dir>     # a shared plan as files (also push, status, new)
$ nix run .#ci-clippy               # also: ci-bolero, ci-deny, ci-doc, ci-editor, ci-fmt, ci-js, ci-msrv, ci-mutants, ci-no-std, ci-test, ci-typos
```

Every CI job is one of these flake apps, so a local run checks exactly what CI does. The goldens in `rust/dagenda_core/tests/golden/` pin everything the core derives from a document; after an intended change, review the diff and re-bless with `BLESS=1 cargo test --test golden`.

## License

Licensed under either of [Apache License, Version 2.0](./LICENSE-APACHE) or [MIT license](./LICENSE-MIT) at your option.

[Keyhive]: https://github.com/inkandswitch/keyhive
[sans-effort]: https://github.com/inkandswitch/sans-effort
[Subduction]: https://github.com/inkandswitch/subduction
