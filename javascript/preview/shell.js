// The page around a plan's picture: theme, zoom, the tech tree and text
// views, export, and what the picture says (the status line and the list of
// problems). Shared by the preview (main.js) and the collaborative editor
// (../editor/), whose pages use the same ids.

/**
 * @param {object} options
 * @param {string} options.prefix  key prefix for per-viewer settings in localStorage
 * @param {() => { name: string, source: string }[]} options.documents  the plan's documents, now
 * @param {(document: number, line: number) => void} options.goTo  show a problem's line
 * @param {(problems: object[]) => void} [options.onRender]  after every render, with its problems
 * @param {{ renderPlanText: Function }} options.wasm  from dagenda_wasm, once initialized
 */
export const shell = ({ prefix, documents, goTo, onRender = () => {}, wasm }) => {
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
