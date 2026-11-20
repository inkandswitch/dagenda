// <dagenda-view>: a roadmap's tech tree, drawn from the dagenda notation.
//
//   <script type="module" src="./dagenda-view.js"></script>
//   <dagenda-view><script type="text/dagenda">
//     roadmap
//     [x] task parser [Parser]
//     [ ] task resolver [Resolver]
//       needs parser
//   </script></dagenda-view>
//
// or <dagenda-view src="./plan.dagenda"></dagenda-view>, or set `.source`
// from script. The notation's indentation is relative, so inline source can
// be indented to match the page. Add theme="light" or theme="dark" to
// override the system's colour scheme, and zoom="1.5" to scale.
//
// Hover a node to light up everything it needs and everything waiting on it
// (all the way up and down, not just the lines drawn). Click a node to
// select it; click it again, or press Escape, to clear.
//
// Text that doesn't parse or resolve keeps the last good picture up, so the
// tree doesn't flicker while someone types.
//
// Events (all bubble):
//
//   dagenda-render  { problems, ok }  after every change of source
//   dagenda-select  { id }            a node was selected (id is null when cleared)
//
// All logic lives in the Wasm (`dagenda_wasm`); this file only wires the
// SVG's data-* hooks to the page.

import init, { check, renderSvg } from "./pkg/dagenda_wasm.js"

const ready = init()

const STYLE = `
  :host { display: block; overflow: auto; position: relative; }
  svg { display: block; max-width: none; }
  svg .node { cursor: pointer; transition: opacity 120ms; }
  svg .edge { transition: opacity 120ms; }
  svg.focusing .node:not(.lit), svg.focusing .edge:not(.lit) { opacity: 0.15; }
  svg .edge.lit { stroke-width: 2.5; }
  svg .node.focus :is(.card, .gate) { stroke-width: 3; }
  svg .node.selected :is(.card, .gate) { stroke: #2f6fd6; stroke-width: 3; }
  @media (prefers-reduced-motion: reduce) { svg .node, svg .edge { transition: none; } }
`

/** Everything reachable from `start` by following `next`, `start` included. */
const reach = (start, next) => {
  const seen = new Set([start])
  const stack = [start]
  while (stack.length > 0) {
    for (const id of next.get(stack.pop()) ?? []) {
      if (!seen.has(id)) {
        seen.add(id)
        stack.push(id)
      }
    }
  }
  return seen
}

/** A problem from the Wasm, as a plain object (the Wasm's own is freed). */
const plain = (problem) => {
  const copy = { line: problem.line, column: problem.column, severity: problem.severity, message: problem.message }
  problem.free()
  return copy
}

class DagendaView extends HTMLElement {
  static observedAttributes = ["src", "theme", "zoom"]

  #source = ""
  #problems = []
  #selected = null
  #canvas
  #svg = null
  // id → ids it waits on, and id → ids waiting on it, from the drawn lines.
  // The lines leave out what longer paths imply, but following them still
  // reaches everything upstream and downstream.
  #up = new Map()
  #down = new Map()

  constructor() {
    super()
    const shadow = this.attachShadow({ mode: "open" })
    shadow.innerHTML = `<style>${STYLE}</style><div part="canvas"></div>`
    this.#canvas = shadow.querySelector("div")

    this.#canvas.addEventListener("pointerover", (e) => {
      const node = e.target.closest?.(".node")
      if (node) this.#focus(node.dataset.id)
    })
    this.#canvas.addEventListener("pointerleave", () => this.#focus(null))
    this.#canvas.addEventListener("click", (e) => {
      // A card's links open their page; they don't select the card
      if (e.target.closest?.("a")) return
      const node = e.target.closest?.(".node")
      this.select(node && node.dataset.id !== this.#selected ? node.dataset.id : null)
    })
    this.addEventListener("keydown", (e) => {
      if (e.key === "Escape") this.select(null)
    })
  }

  connectedCallback() {
    if (!this.hasAttribute("tabindex")) this.tabIndex = 0
    if (this.hasAttribute("src") || this.#source) return
    const script = this.querySelector('script[type="text/dagenda"]')
    const text = (script ?? this).textContent ?? ""
    if (text.trim()) this.source = text
  }

  attributeChangedCallback(name) {
    if (name === "src") this.#load()
    else this.#style()
  }

  /** The plan's source text. Setting it redraws. */
  get source() {
    return this.#source
  }

  set source(text) {
    this.#source = text
    this.#render()
  }

  /** The problems with the current source: `{ line, column, severity, message }`. */
  get problems() {
    return this.#problems
  }

  /** The selected node's id, or null. */
  get selected() {
    return this.#selected
  }

  /** The drawn SVG document's markup, or "" before the first good draw. */
  get svg() {
    return this.#svg ? this.#svg.outerHTML : ""
  }

  /** Select a node by id (null clears), firing `dagenda-select`. */
  select(id) {
    if (id === this.#selected) return
    this.#selected = id
    this.#mark()
    this.dispatchEvent(new CustomEvent("dagenda-select", { bubbles: true, detail: { id } }))
  }

  async #load() {
    const url = this.getAttribute("src")
    if (!url) return
    const response = await fetch(url)
    this.source = await response.text()
  }

  async #render() {
    await ready
    const source = this.#source
    const problems = check(source).map(plain)
    // A newer source arrived while this one waited for the Wasm
    if (source !== this.#source) return

    const ok = !problems.some((problem) => problem.severity === "error")
    if (ok) this.#draw(renderSvg(source))
    this.#problems = problems
    this.dispatchEvent(new CustomEvent("dagenda-render", { bubbles: true, detail: { problems, ok } }))
  }

  #draw(markup) {
    this.#canvas.innerHTML = markup
    this.#svg = this.#canvas.querySelector("svg")
    this.#up = new Map()
    this.#down = new Map()
    for (const edge of this.#svg.querySelectorAll(".edge")) {
      const { from, to } = edge.dataset
      if (!this.#up.has(to)) this.#up.set(to, [])
      if (!this.#down.has(from)) this.#down.set(from, [])
      this.#up.get(to).push(from)
      this.#down.get(from).push(to)
    }

    const ids = new Set([...this.#svg.querySelectorAll(".node")].map((node) => node.dataset.id))
    if (this.#selected !== null && !ids.has(this.#selected)) this.select(null)
    this.#style()
    this.#mark()
  }

  /** Light up what `id` needs and what waits on it; null clears. */
  #focus(id) {
    if (!this.#svg) return
    this.#svg.classList.toggle("focusing", id !== null)
    for (const element of this.#svg.querySelectorAll(".lit, .focus")) element.classList.remove("lit", "focus")
    if (id === null) return

    const upstream = reach(id, this.#up)
    const downstream = reach(id, this.#down)
    for (const node of this.#svg.querySelectorAll(".node")) {
      const lit = upstream.has(node.dataset.id) || downstream.has(node.dataset.id)
      node.classList.toggle("lit", lit)
      node.classList.toggle("focus", node.dataset.id === id)
    }
    for (const edge of this.#svg.querySelectorAll(".edge")) {
      const { from, to } = edge.dataset
      const lit = (upstream.has(from) && upstream.has(to)) || (downstream.has(from) && downstream.has(to))
      edge.classList.toggle("lit", lit)
    }
  }

  #mark() {
    if (!this.#svg) return
    for (const node of this.#svg.querySelectorAll(".node")) {
      node.classList.toggle("selected", node.dataset.id === this.#selected)
    }
  }

  #style() {
    if (!this.#svg) return
    const theme = this.getAttribute("theme")
    this.#svg.classList.toggle("light", theme === "light")
    this.#svg.classList.toggle("dark", theme === "dark")

    const zoom = Number(this.getAttribute("zoom") ?? 1) || 1
    const [, , width, height] = (this.#svg.getAttribute("viewBox") ?? "0 0 0 0").split(" ").map(Number)
    this.#svg.setAttribute("width", String(Math.round(width * zoom)))
    this.#svg.setAttribute("height", String(Math.round(height * zoom)))
  }
}

customElements.define("dagenda-view", DagendaView)
