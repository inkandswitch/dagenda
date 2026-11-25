// The page around a plan's picture: theme, zoom, the tech tree and text
// views, export, help, and what the picture says (the status line and the list of
// problems). Shared by the preview (main.js) and the collaborative editor
// (../editor/), whose pages use the same ids.

/**
 * @param {object} options
 * @param {string} options.prefix  key prefix for per-viewer settings in localStorage
 * @param {() => { name: string, source: string }[]} options.documents  the plan's documents, now
 * @param {(document: number, line: number) => void} options.goTo  show a problem's line
 * @param {(problems: object[]) => void} [options.onRender]  after every render, with its problems
 * @param {string} [options.help]  this page's own help, as `<h3>` sections, after the shared ones
 * @param {{ renderPlanText: Function }} options.wasm  from dagenda_wasm, once initialized
 */
export const shell = ({ prefix, documents, goTo, onRender = () => {}, help = "", wasm }) => {
  const $ = (id) => document.getElementById(id)
  const picture = $("picture")

  // --- Settings, kept per browser ----------------------------------------------

  const load = (name, fallback) => {
    try {
      return localStorage.getItem(`${prefix}:${name}`) ?? fallback
    } catch {
      return fallback
    }
  }
  const save = (name, value) => {
    try {
      localStorage.setItem(`${prefix}:${name}`, String(value))
    } catch {}
  }

  // --- What the picture says ---------------------------------------------------

  // `line:column`, or `name:line:column` when the plan has several documents
  const place = (problem) => {
    const all = documents()
    const at = `${problem.line}:${problem.column}`
    return all.length > 1 ? `${all[problem.document]?.name ?? "?"}:${at}` : at
  }

  picture.addEventListener("dagenda-render", ({ detail: { problems, ok } }) => {
    const error = problems.find((problem) => problem.severity === "error")
    const status = $("status")
    const warnings = problems.filter((problem) => problem.severity === "warning").length
    if (error) {
      status.className = "error"
      status.textContent = `✗ ${place(error)}: ${error.message}`
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
        button.textContent = `${place(problem)}  ${problem.message}`
        button.addEventListener("click", () => goTo(problem.document, problem.line))
        item.append(button)
        return item
      }),
    )

    onRender(problems)
    if (ok && view === "text") showText()
  })

  // --- Views ---------------------------------------------------------------------

  let view = load("view", "tree")
  const showText = () => {
    try {
      $("text").textContent = wasm.renderPlanText(documents())
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

  // --- Help ----------------------------------------------------------------------

  const dialog = document.createElement("dialog")
  dialog.id = "help-dialog"
  dialog.setAttribute("aria-labelledby", "help-title")
  dialog.innerHTML = `
    <form method="dialog"><button type="submit" aria-label="Close">✕</button></form>
    <h2 id="help-title">How to use dagenda</h2>
    ${HELP}
    ${help}
    <p class="note">Press <kbd>?</kbd> to open or close this.</p>
  `
  document.body.append(dialog)
  // A click on the backdrop (outside the box) closes it, as Escape does
  dialog.addEventListener("click", (e) => {
    const { left, right, top, bottom } = dialog.getBoundingClientRect()
    if (e.target === dialog && (e.clientX < left || e.clientX > right || e.clientY < top || e.clientY > bottom)) dialog.close()
  })
  $("help").addEventListener("click", () => dialog.showModal())
  // "?" opens it, unless it's being typed
  document.addEventListener("keydown", (e) => {
    const typing = e.composedPath().some((element) => element.matches?.("input, textarea, [contenteditable]"))
    if (e.key === "?" && !typing && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault()
      dialog.open ? dialog.close() : dialog.showModal()
    }
  })

  // Sent by the preview's dev server (serve.mjs) when the Wasm rebuild fails;
  // the page keeps running the last good build
  window.addEventListener("dagenda:build-error", ({ detail }) => {
    $("status").className = "error"
    $("status").textContent = "✗ The Wasm build failed; still running the last good build"
    console.error(detail)
  })

  return {
    load,
    save,
    /** Apply the saved theme, view, and zoom. */
    start: () => {
      setTheme(load("theme", "system"))
      setView(view)
      setZoom(Number(load("zoom", "1")) || 1)
    },
  }
}

/** What both pages share: moving around, looking, and editing the picture. */
const HELP = `
  <p>Write the plan as text on the left; its tech tree draws on the right. Text and picture are one: an edit on the picture lands in the text.</p>

  <h3>Moving around</h3>
  <table>
    <tr><th>Pan</th><td>Scroll (<kbd>Shift</kbd> + wheel across), drag the background, or drag anywhere with the middle button. On touch, drag with one finger.</td></tr>
    <tr><th>Zoom</th><td><kbd>Ctrl</kbd>/<kbd>⌘</kbd> + wheel, or pinch on a trackpad. Or use <b>−</b>, <b>+</b>, and <b>Fit</b>; click the percentage to go back to 100%.</td></tr>
  </table>

  <h3>Looking around</h3>
  <ul>
    <li><b>Hover</b> a card to light up everything it needs and everything waiting on it.</li>
    <li><b>Click</b> a card to select it and show its line in the text. Click it again, or press <kbd>Esc</kbd>, to clear.</li>
    <li><b>Click someone's sticker</b>, or their <code>@name</code> on a card, to see only what they're on. Click again, click the background, or press <kbd>Esc</kbd> to see everything.</li>
    <li>A card's pull requests, issues, and links open in a new tab.</li>
    <li>Click a problem under the picture to go to its line.</li>
  </ul>

  <h3>Editing the picture</h3>
  <table>
    <tr><th>Set a status</th><td>Click a card's badge, then pick a box.</td></tr>
    <tr><th>Add a need</th><td>Drag from a card's badge onto the card it needs.</td></tr>
    <tr><th>Re-scope a card</th><td>Drag it into another milestone's band (the top layer: no milestone).</td></tr>
    <tr><th>Move a milestone</th><td>Drag its dashed line or its gate: up past cards brings them in, down hands them to the era above. Past another milestone's line, the two swap.</td></tr>
  </table>
  <p class="note">Picture edits need a mouse or a trackpad for now. An edit that makes no sense says why, where you let go.</p>

  <h3>The text</h3>
  <ul>
    <li><kbd>Tab</kbd> and <kbd>Shift</kbd>+<kbd>Tab</kbd> indent and outdent the selected lines.</li>
    <li><b>Small plan</b> and <b>Tour</b> load examples; <b>Tour</b> uses every form.</li>
    <li><b>Text</b> shows the plan as an outline. <b>Copy</b> and <b>Download</b> export what's showing, as SVG or text.</li>
  </ul>

`
