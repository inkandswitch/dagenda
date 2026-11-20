// The preview: type a plan on the left, and its tech tree redraws on the
// right. Single-user, with no sync; the source is kept in this browser.
// _Collaborate_ hands the text to the collaborative editor (../collab/).

import init, { locate, renderPlanText } from "../dagenda-view/pkg/dagenda_wasm.js"
import "../dagenda-view/dagenda-view.js"
import { BASIC, TOUR } from "./examples.js"
import { highlight } from "./highlight.js"
import { indentLines } from "./indent.js"
import { shell } from "./shell.js"

const $ = (id) => document.getElementById(id)
const source = $("source")
const layer = $("highlight")
const picture = $("picture")

let errorLine = null
const page = shell({
  prefix: "dagenda-preview",
  documents: () => [{ name: "", source: source.value }],
  goTo: (_, line) => goTo(line),
  onRender: (problems) => {
    errorLine = problems.find((problem) => problem.severity === "error")?.line ?? null
    paint()
  },
  wasm: { renderPlanText },
})

// --- Source ------------------------------------------------------------------

const syncScroll = () => {
  layer.scrollTop = source.scrollTop
  layer.scrollLeft = source.scrollLeft
}
const paint = () => {
  layer.innerHTML = highlight(source.value, errorLine)
  syncScroll()
}

let queued = false
const update = () => {
  page.save("source", source.value)
  paint()
  if (queued) return
  queued = true
  requestAnimationFrame(() => {
    queued = false
    picture.source = source.value
    // The collaborative editor makes a shared document from the text it's given
    $("collaborate").href = `../collab/#source=${encodeURIComponent(source.value)}`
  })
}

/** Replace the text from `start` to `end`, keeping the browser's own undo. */
const insert = (text, start = source.selectionStart, end = source.selectionEnd) => {
  source.focus()
  source.setSelectionRange(start, end)
  if (!document.execCommand?.("insertText", false, text)) {
    source.setRangeText(text, start, end, "end")
    update()
  }
}

// Indentation is structure, so Tab indents with two spaces (every line a
// selection touches), Shift+Tab outdents, and Enter keeps the indent of the
// line above
source.addEventListener("keydown", (e) => {
  if (e.key === "Tab" && !e.metaKey && !e.ctrlKey && !e.altKey) {
    e.preventDefault()
    const { selectionStart: start, selectionEnd: end, value } = source
    if (!e.shiftKey && start === end) return insert("  ")
    const { from, to, text, start: after, end: until } = indentLines(value, start, end, e.shiftKey)
    if (text !== value.slice(from, to)) insert(text, from, to)
    source.setSelectionRange(after, until)
  } else if (e.key === "Enter" && !e.shiftKey && !e.metaKey && !e.ctrlKey) {
    const before = source.value.slice(0, source.selectionStart)
    const indent = /^ */.exec(before.slice(before.lastIndexOf("\n") + 1))[0]
    e.preventDefault()
    insert(`\n${indent}`)
  }
})

/** Select line `line` (1-based) and scroll it into view. */
const goTo = (line) => {
  const lines = source.value.split("\n")
  const start = lines.slice(0, line - 1).reduce((offset, text) => offset + text.length + 1, 0)
  source.focus()
  source.setSelectionRange(start, start + (lines[line - 1] ?? "").length)
  const height = parseFloat(getComputedStyle(source).lineHeight) || 20
  source.scrollTop = Math.max(0, (line - 4) * height)
  syncScroll()
}

for (const [id, text] of [
  ["example-basic", BASIC],
  ["example-tour", TOUR],
]) {
  $(id).addEventListener("click", () => {
    insert(text, 0, source.value.length)
    source.setSelectionRange(0, 0)
    source.scrollTop = 0
    syncScroll()
  })
}

/** The smallest change from `from` to `to`: where, how much goes, and what comes in. */
const diff = (from, to) => {
  let start = 0
  while (start < from.length && start < to.length && from[start] === to[start]) start++
  let end = 0
  while (end < from.length - start && end < to.length - start && from[from.length - 1 - end] === to[to.length - 1 - end]) {
    end++
  }
  return { at: start, removed: from.length - start - end, inserted: to.slice(start, to.length - end) }
}

// Edits made on the picture arrive as new text: applied as the smallest
// change, through the textarea's own undo. One made from older text (the
// picture hadn't caught up) is left out
picture.addEventListener("dagenda-edit", ({ detail: { edits } }) => {
  for (const { document, source: next, base } of edits) {
    if (document !== 0 || base !== source.value) continue
    const { at, removed, inserted } = diff(base, next)
    insert(inserted, at, at + removed)
  }
})

// Clicking a box puts the caret on its line
picture.addEventListener("dagenda-select", ({ detail: { id } }) => {
  if (id === null) return
  const line = locate(source.value, id)
  if (line !== undefined) goTo(line)
})

// --- Start -----------------------------------------------------------------------

await init()
source.value = page.load("source", BASIC)
source.addEventListener("input", update)
source.addEventListener("scroll", syncScroll)
page.start()
update()
