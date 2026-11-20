// The preview: type a plan on the left, and its tech tree redraws on the
// right. Single-user, with no sync; the source is kept in this browser.
// _Collaborate_ hands the text to the collaborative editor (../collab/).

import init, { locate, renderPlanText } from "../dagenda-view/pkg/dagenda_wasm.js"
import "../dagenda-view/dagenda-view.js"
import { BASIC, TOUR } from "./examples.js"
import { highlight } from "./highlight.js"
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

// Indentation is structure, so Tab indents with spaces and Enter keeps the
// indent of the line above
source.addEventListener("keydown", (e) => {
  if (e.key === "Tab" && !e.shiftKey && !e.metaKey && !e.ctrlKey) {
    e.preventDefault()
    insert("  ")
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
