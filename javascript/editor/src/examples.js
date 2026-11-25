// Example plans for the preview's buttons.

/** A small plan to start from. */
export const BASIC = `roadmap
title A small plan

// Edit me: the tech tree redraws as you type.
// Hover a box to see what it needs and what waits on it;
// click one to jump to its line.

track core [Core]
  [x] parser [Parser]
  [-] resolver [Resolver]
    needs parser
  [ ] views [Views]
    needs resolver

track web [Web]
  [ ] wasm [Wasm bindings]
    needs resolver
  [ ] preview [Preview page]
    needs wasm, views

milestone firstLook [A first look]
  target 2026-Q4
  views
  preview
`

/**
 * Every form in the notation. The same text as the golden test
 * rust/dagenda_core/tests/golden/tour.dagenda (examples.test.mjs checks).
 */
export const TOUR = `roadmap
title A tour of the notation

// Comments are whole lines that start with \`//\`.

// A person, and the picture for the stickers on their cards.
@alice
  avatar gh:octocat

track core [Core]
  dri @alice
  note The track's own note.

  [x] parser [Parser]
    pr gh:owner/repo#1
    est 2026-11-19

  [-] resolver [Resolver]
    needs parser
    dri @alice, @bob
    issue gh:owner/repo#2
    link https://github.com/owner/repo/issues/2#issuecomment-3
    link https://example.com/resolver-design [Design doc]
    est 2026-12
    target 2026-Q4
    note
      Resolves ids across every document.

      \`\`\`
      // Not a comment: this is inside the note.
      \`\`\`
    [x] duplicates [Duplicate ids]
      dri @bob
    [ ] cycles [Cycles]

  track views [Views \\[nested\\]]
      colour plum
      [ ] techTree [Tech tree]
          needs resolver, itParses
      [?] skillTree [Skill tree]
      [*] layout [Layout research]
      [!] fonts [Fonts]
      [>] gantt [Gantt]
      [~] mermaid [Mermaid export]

[ ] untracked [A task outside any track]

milestone itParses [It parses]
  target EOY26
  parser
  resolver
  nice-to-have
    cycles

milestone firstPicture [First picture]
  note Waits on a milestone, through a task that needs it.
  itParses
  techTree
  nice-to-have
    skillTree
`
