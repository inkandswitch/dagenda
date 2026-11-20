// Example plans for the preview's buttons.

/** A small plan to start from. */
export const BASIC = `roadmap
title A small plan

// Edit me: the tech tree redraws as you type.
// Hover a box to see what it needs and what waits on it;
// click one to jump to its line.

track core [Core]
  [x] task parser [Parser]
  [-] task resolver [Resolver]
    needs parser
  [ ] task views [Views]
    needs resolver

track web [Web]
  [ ] task wasm [Wasm bindings]
    needs resolver
  [ ] task preview [Preview page]
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

track core [Core]
  dri @alice
  note The track's own note.

  [x] task parser [Parser]
    pr gh:owner/repo#1
    est 2w

  [-] task resolver [Resolver]
    needs parser
    issue gh:owner/repo#2
    target 2026-Q4
    note
      Resolves ids across every document.

      \`\`\`
      // Not a comment: this is inside the note.
      \`\`\`
    [x] task duplicates [Duplicate ids]
    [ ] task cycles [Cycles]

  track views [Views \\[nested\\]]
      [ ] task techTree [Tech tree]
          needs resolver, itParses
      [?] task skillTree [Skill tree]
      [*] task layout [Layout research]
      [!] task fonts [Fonts]
      [>] task gantt [Gantt]
      [~] task mermaid [Mermaid export]

[ ] task untracked [A task outside any track]

milestone itParses [It parses]
  target EOY26
  parser
  resolver
  ---
  cycles

milestone firstPicture [First picture]
  note Waits on a milestone, through a task that needs it.
  itParses
  techTree
  ---
  skillTree
`
