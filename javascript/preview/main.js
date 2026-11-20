// The preview: type a plan on the left, and its tech tree redraws on the
// right. Single-user, with no sync; the source is kept in this browser.

import init, { locate, renderText } from "../dagenda-view/pkg/dagenda_wasm.js"
import "../dagenda-view/dagenda-view.js"
import { BASIC, TOUR } from "./examples.js"
import { highlight } from "./highlight.js"

const $ = (id) => document.getElementById(id)
const source = $("source")
const layer = $("highlight")
const picture = $("picture")

// --- Settings, kept per browser ---------------------------------------------

const KEY = "dagenda-preview"
const load = (name, fallback) => {
  try {
    return localStorage.getItem(`${KEY}:${name}`) ?? fallback
  } catch {
    return fallback
  }
}
const save = (name, value) => {
  try {
    localStorage.setItem(`${KEY}:${name}`, String(value))
  } catch {}
}

// --- Source ------------------------------------------------------------------

let errorLine = null
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
  save("source", source.value)
  paint()
  if (queued) return
  queued = true
  requestAnimationFrame(() => {
    queued = false
    picture.source = source.value
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

// --- What the picture says ---------------------------------------------------

picture.addEventListener("dagenda-render", ({ detail: { problems, ok } }) => {
  const error = problems.find((problem) => problem.severity === "error")
  errorLine = error?.line ?? null
  paint()

  const status = $("status")
  const warnings = problems.filter((problem) => problem.severity === "warning").length
  if (error) {
    status.className = "error"
    status.textContent = `✗ ${error.line}:${error.column}: ${error.message}`
  } else if (warnings > 0) {
    status.className = "warning"
    status.textContent = `⚠ resolves, with ${warnings} warning${warnings === 1 ? "" : "s"}`
  } else {
    status.className = "ok"
    status.textContent = "✓ resolves"
  }

  $("problems").replaceChildren(
    ...problems.map((problem) => {
      const item = document.createElement("li")
      const button = document.createElement("button")
      button.type = "button"
      button.className = problem.severity
      button.textContent = `${problem.line}:${problem.column}  ${problem.message}`
      button.addEventListener("click", () => goTo(problem.line))
      item.append(button)
      return item
    }),
  )

  if (ok && view === "text") showText()
})

// Clicking a box puts the caret on its line
picture.addEventListener("dagenda-select", ({ detail: { id } }) => {
  if (id === null) return
  const line = locate(source.value, id)
  if (line !== undefined) goTo(line)
})

// --- Views ---------------------------------------------------------------------

let view = load("view", "tree")
const showText = () => {
  try {
    $("text").textContent = renderText(source.value)
  } catch {
    // Keep the last good text, as the picture does
  }
}
const setView = (next) => {
  view = next === "text" ? "text" : "tree"
  save("view", view)
  picture.hidden = view !== "tree"
  $("text").hidden = view !== "text"
  for (const tab of document.querySelectorAll("[data-view]")) {
    tab.setAttribute("aria-selected", String(tab.dataset.view === view))
  }
  $("zoom-out").parentElement.hidden = view !== "tree"
  if (view === "text") showText()
}
for (const tab of document.querySelectorAll("[data-view]")) {
  tab.addEventListener("click", () => setView(tab.dataset.view))
}

// --- Zoom ----------------------------------------------------------------------

let zoom = 1
const setZoom = (next) => {
  zoom = Math.min(4, Math.max(0.2, Math.round(next * 100) / 100))
  picture.setAttribute("zoom", String(zoom))
  $("zoom-level").textContent = `${Math.round(zoom * 100)}%`
  save("zoom", zoom)
}
$("zoom-in").addEventListener("click", () => setZoom(zoom * 1.25))
$("zoom-out").addEventListener("click", () => setZoom(zoom / 1.25))
$("zoom-level").addEventListener("click", () => setZoom(1))
$("zoom-fit").addEventListener("click", () => {
  const svg = picture.shadowRoot?.querySelector("svg")
  const width = Number(svg?.getAttribute("viewBox")?.split(" ")[2])
  if (width > 0) setZoom((picture.clientWidth - 24) / width)
})
picture.addEventListener(
  "wheel",
  (e) => {
    if (!e.ctrlKey && !e.metaKey) return
    e.preventDefault()
    setZoom(zoom * (e.deltaY < 0 ? 1.1 : 1 / 1.1))
  },
  { passive: false },
)

// --- Theme -----------------------------------------------------------------------

const setTheme = (choice) => {
  const theme = choice === "light" || choice === "dark" ? choice : "system"
  save("theme", theme)
  if (theme === "system") {
    delete document.documentElement.dataset.theme
    picture.removeAttribute("theme")
  } else {
    document.documentElement.dataset.theme = theme
    picture.setAttribute("theme", theme)
  }
  for (const button of document.querySelectorAll("[data-theme-choice]")) {
    button.setAttribute("aria-pressed", String(button.dataset.themeChoice === theme))
  }
}
for (const button of document.querySelectorAll("[data-theme-choice]")) {
  button.addEventListener("click", () => setTheme(button.dataset.themeChoice))
}

// --- Export ------------------------------------------------------------------------

const exported = () =>
  view === "text"
    ? { text: $("text").textContent, type: "text/plain", name: "roadmap.txt" }
    : { text: picture.svg, type: "image/svg+xml", name: "roadmap.svg" }

$("copy").addEventListener("click", async () => {
  try {
    await navigator.clipboard.writeText(exported().text)
    $("copy").textContent = "Copied"
  } catch {
    $("copy").textContent = "Can't copy"
  }
  setTimeout(() => ($("copy").textContent = "Copy"), 1200)
})
$("download").addEventListener("click", () => {
  const { text, type, name } = exported()
  const link = document.createElement("a")
  link.href = URL.createObjectURL(new Blob([text], { type }))
  link.download = name
  link.click()
  URL.revokeObjectURL(link.href)
})

// Sent by the dev server (serve.mjs) when the Wasm rebuild fails; the page
// keeps running the last good build
window.addEventListener("dagenda:build-error", ({ detail }) => {
  $("status").className = "error"
  $("status").textContent = "✗ The Wasm build failed; still running the last good build"
  console.error(detail)
})

// --- Start -----------------------------------------------------------------------

await init()
source.value = load("source", BASIC)
source.addEventListener("input", update)
source.addEventListener("scroll", syncScroll)
setTheme(load("theme", "system"))
setView(view)
setZoom(Number(load("zoom", "1")) || 1)
update()
