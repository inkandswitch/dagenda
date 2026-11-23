---
name: dagenda
license: MIT OR Apache-2.0
description: Read, write, check, and render dagenda roadmaps, either as `.dagenda` files or as a shared plan workspace (an `automerge:` URL that the dagenda editor opens). Use when asked to make or update a roadmap or plan in dagenda; to add tasks, milestones, people, avatars, or pull requests to one; to check one for errors or render its tech tree; or to edit a shared dagenda document that other people have open.
---

# dagenda

A dagenda plan is plain text: tracks of tasks, milestones that gate on them, and `needs` between them. It's one dependency graph, drawn as a tech tree that grows upward. A plan can span several documents (a public one and a private one, say) that refer to each other's ids.

## Tools

Run these from the repo root (or anywhere inside `nix develop`):

| Command                                            | Does                                                                 |
|----------------------------------------------------|----------------------------------------------------------------------|
| `nix run .#dagenda -- check a.dagenda b.dagenda`   | Check documents as one plan; prints every problem as `path:line:column: severity: message` |
| `nix run .#dagenda -- render a.dagenda`            | The plan as text (milestones and tracks)                              |
| `nix run .#dagenda -- render a.dagenda -o x.svg`   | Its tech tree as SVG (`-o x.txt` for text); never overwrites a plan file |
| `nix run .#dagenda -- edit status <task> done a.dagenda b.dagenda` | Change a plan in place: `status`, `need`, `scope`, `order`, `dri`, `est`, `target` (see `edit --help`) |
| `nix run .#collab -- pull <dir> <automerge:URL>`   | First pull of a shared workspace: each document into `<dir>/<name>.dagenda` |
| `nix run .#collab -- pull <dir>`                   | Pull again (the URL is saved in `<dir>/workspace.url`)                |
| `nix run .#collab -- status <dir>`                 | Each document, and whether it has edits to push or to pull           |
| `nix run .#collab -- diff <dir>`                   | What changed since your pull, and who changed it; and your unpushed edits |
| `nix run .#collab -- push <dir> --as=<you>`        | Push edited files back, merged with what others typed since the pull |
| `nix run .#collab -- undo <dir> --as=<you>`        | Take back your last push, keeping what others typed since            |
| `nix run .#collab -- new <dir> --as=<you>`         | A new workspace from the `.dagenda` files in `<dir>` (`plan` first)  |

`cargo run -q -p dagenda_cli -- check …` works without Nix. The editor for a workspace is `https://inkandswitch.github.io/dagenda/collab/#automerge:…`; `pull` and `new` print the link. Give it to the user when you make a workspace.

## The notation

```
roadmap
title Product Roadmap

// A comment: a whole line starting with //

@alice
  avatar gh:octocat

track sync [Sync]
  dri @alice
  colour teal
  [-] engine [New sync engine]
    needs protocol
    dri @alice, @bob
    est 2026-12
    pr gh:owner/repo#123
    link https://example.com/design [Design doc]
    note One line of markdown.
    [x] protocol [Wire protocol]
  [ ] offline [Offline mode]

milestone v2 [Version 2]
  target EOY26
  engine
  nice-to-have
    offline
```

| Form                                 | Meaning                                                                 | Goes in                     |
|--------------------------------------|-------------------------------------------------------------------------|-----------------------------|
| `roadmap`                            | The header; always the first line                                       | Top                         |
| `title …`                            | The document's title                                                    | Top                         |
| `track <id> [<label>]`               | A bucket of tasks. Tracks nest                                          | Top, track                  |
| `<box> <id> [<label>]`               | A task. One written under another task is one that task needs           | Top, track, task, `extend`  |
| `milestone <id> [<label>]`           | A gate: the ids under it are its must-haves                             | Top                         |
| `nice-to-have`                       | In a milestone: the ids indented under it are wanted, not required      | Milestone                   |
| `extend <id>`                        | Details and tasks for a task defined in another document                | Top                         |
| `@<handle>` + `avatar gh:<login>` or `avatar <url>` | A person, and the picture for the stickers on their cards | Top                         |
| `needs <id>, <id>`                   | This can't finish until those do; lines add up                          | Task, `extend`              |
| `est <date>` / `target <date>`       | When it's expected / when it should land                                | Task (`target` also milestone) |
| `dri @<handle>, …`                   | The people responsible: one line, each person once                      | Task, track, `extend`       |
| `pr gh:<owner>/<repo>#<n>` / `issue …` | A pull request / an issue                                             | Task, `extend`              |
| `link <url> [<label>]`               | Any other page: a comment, a doc, a branch                              | Task, `extend`              |
| `note <text>`, or `note` + indented block | Markdown; a block runs until the indent comes back                 | Track, task, milestone, `extend` |
| `colour <name or #rrggbb>`           | A track's colour: teal, indigo, rose, ochre, sage, plum, sky, terracotta | Track                       |

Status boxes; only `[x]` counts as finished:

| Box   | Status           | Box   | Status                       |
|-------|------------------|-------|------------------------------|
| `[ ]` | Not started      | `[?]` | Idea, or needs clarification |
| `[-]` | In progress      | `[!]` | Needs attention              |
| `[x]` | Done             | `[*]` | Research                     |
| `[>]` | Deferred         | `[~]` | Dropped                      |
| `[^]` | In review (a PR) | `[@]` | Awaiting a reply             |

Rules that trip people up:

- _Indentation is structure,_ and how far is up to the writer, but siblings must line up. Tabs are an error.
- _Ids_ are an ASCII letter, then letters, digits, `-`, `_`. Keywords can't be ids: `avatar`, `color`, `colour`, `dri`, `est`, `extend`, `issue`, `link`, `milestone`, `needs`, `nice-to-have`, `note`, `pr`, `roadmap`, `target`, `task`, `title`, `track`.
- _A task is its box and id:_ `[ ] parser`, never `[ ] task parser` (the old form is an error).
- _Dates_ are `2026-11-19`, `2026-11`, `2026-Q4`, `2026`, or `EOY26`. Durations (`2w`) are an error.
- _One owner per detail:_ `est`, `target`, and `dri` can be set once per task across every document; a second is an error.
- _A label_ is display text in brackets. Leave it off and the card shows the id. Escape a bracket inside one as `\[` or `\]`.
- _A sticker_ needs an explicit person block with an `avatar`. Nothing is inferred from a handle or a pull request's author.

The crate docs (`rust/dagenda_core/src/lib.rs`, `parser.rs`) have the full grammar.

## Editing a plan

1. Read the whole plan first, every document of it: a task's `dri` or notes may live in another document's `extend`.
2. Make the smallest edit that says it. Keep ids stable: other documents and milestones refer to them. Change a label, not an id, to rename. For a status, a need, a milestone's scope or order, a `dri`, `est`, or `target`, use `dagenda edit`: it changes the text exactly where that's said (in whichever document), can't break the syntax, and refuses a loop or an unknown id. Give it every document of the plan.

   ```console
   $ nix run .#dagenda -- edit status signing in-progress plan.dagenda private.dagenda
   $ nix run .#dagenda -- edit need betaCriteria cryptoReview plan.dagenda
   $ nix run .#dagenda -- edit dri sdnFixups @alexjg,@expede plan.dagenda
   ```
3. Check every document together, and fix everything it reports (it reports all problems at once):

   ```console
   $ nix run .#dagenda -- check plan.dagenda private.dagenda --deny-warnings
   ```

4. Render the text form to read back what changed: `nix run .#dagenda -- render plan.dagenda`.

When you move things between documents or restructure (merging documents, reindenting, moving details into `extend`), render the text and the SVG before and after, and compare them: a pure move leaves both byte-identical.

## A shared workspace

A workspace is an Automerge document listing the plan's documents; each is a tab in the editor. Other people may be typing in it while you work, so:

1. `pull` before every edit. Edit only the pulled files. `pull` refuses to write over edits you haven't pushed; push them, or `--discard` them. When `status` says a document is behind, `diff` shows what people changed since your last pull, and who: read it, and don't undo their work.
2. `check` them all together.
3. `push --as=<you>` (say who you are: "claude", "codex"). It checks the whole plan with the repo's parser and with the deployed editor's, and refuses if either finds errors, since people would see them. It applies your edits against the version you pulled, so concurrent typing merges instead of being overwritten, waits until the sync server has everything, and pulls the merged result.
4. Run `status` to confirm: it shows each document and the last push.

If a push was wrong, `undo` takes it back. It makes the inverse edit at your push's version, so anything typed since stays. If the deployed editor is older than the syntax you used, `push` says so; push anyway only with `--ahead-of-site` and when the user agrees, since their editor will show errors until the site is redeployed.

A new `<name>.dagenda` in the directory becomes a new document (a new tab) on `push`. Emptying a document to `roadmap` leaves its tab; nothing deletes one.

> [!NOTE]
> The editor people use is the deployed site, which can lag the repo. Syntax that's newer than the last deploy shows as an error there, even though the repo's `check` passes. `push` catches this; say so when it does.

## Writing a good plan

- _Tasks are outcomes_ ("Signed changes"), small enough that "done" is clear. A big one gets its parts written under it.
- _Say dependencies with `needs`,_ not in prose. A milestone lists what it can't ship without. Use `nice-to-have` for the rest.
- _Be honest with boxes:_ `[?]` for an open question, `[*]` for research, `[!]` for something blocked or at risk, `[^]` once it's up as a pull request, `[@]` while the next move is someone else's.
- _Never invent facts._ Check pull requests and issues with `gh` (that they exist, whether they're open, who wrote them) before adding them. Leave `dri` off rather than guess who owns something. Ask when it matters.
- _Link the evidence:_ `pr`, `issue`, and `link` lines, and a short `note` saying why, not a long essay.
- _Keep what's private private:_ team detail, risks, and names can go in a separate document that `extend`s the public one. Until Keyhive lands, a document is only as private as its URL.
