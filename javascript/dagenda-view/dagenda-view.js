// <dagenda-view>: a roadmap's tech tree, drawn from the dagenda notation.
//
//   <script type="module" src="./dagenda-view.js"></script>
//   <dagenda-view><script type="text/dagenda">
//     roadmap
//     [x] parser [Parser]
//     [ ] resolver [Resolver]
//       needs parser
//   </script></dagenda-view>
//
// or <dagenda-view src="./plan.dagenda"></dagenda-view>, or set `.source`
// from script. A plan of several documents (public and private, say) is
// `.documents = [{ name, source }, …]`: one picture from all of them. The
// notation's indentation is relative, so inline source can be indented to
// match the page. Add theme="light" or theme="dark" to override the system's
// colour scheme, and zoom="1.5" to scale.
//
// With `editable`, the picture can be edited (editing.js): click a badge to
// set a status, drag from a badge to a card to add a need, drag a card into
// a milestone's band to re-scope it, and drag a milestone's line (or gate)
// past cards to move its boundary, or past another milestone's to reorder. The
// picture never changes the text itself: it fires `dagenda-edit` with the
// new text, and the page applies it.
//
// Hover a node to light up everything it needs and everything waiting on it
// (all the way up and down, not just the lines drawn). Click a node to
// select it; click it again, or press Escape, to clear.
//
// Click someone's sticker, or their @name on a card, to light up everything
// they're assigned to, as hovering lights up a chain: their cards, the lines
// between them, and the banners of tracks they run, with everything else
// faded. Their cards show at full strength, even ones that are locked (dimmed
// until what they wait on is done). It stays, whatever's hovered, until you
// click them again, click the background, or press Escape.
//
// Text that doesn't parse or resolve keeps the last good picture up, so the
// tree doesn't flicker while someone types.
//
// Events (all bubble):
//
//   dagenda-render  { problems, ok }  after every change of source
//   dagenda-select  { id }            a node was selected (id is null when cleared)
//   dagenda-person  { handle }        a person was picked, as in "@alice" (null when cleared)
//   dagenda-edit    { kind, edits }   an edit made on the picture: each changed
//                                     document's index, new `source`, and the
//                                     `base` text it was made from
//
// All logic lives in the Wasm (`dagenda_wasm`); this file only wires the
// SVG's data-* hooks to the page.

import { STYLE as EDITING, attachEditing } from "./editing.js"
import init, { checkPlan, renderPlanSvg } from "./pkg/dagenda_wasm.js"

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
  svg :is(.sticker, .fact.dri) { cursor: pointer; }
  svg.picking :is(.node, .edge):not(.assigned) { opacity: 0.15; }
  svg.picking .node.assigned { opacity: 1; }
  svg.picking .node.assigned :is(.card, .gate) { stroke-width: 3; }
  svg.picking .edge.assigned { stroke-width: 2.5; }
  svg.picking .banner.assigned rect { stroke: #2f6fd6; stroke-width: 2; }
  svg .sticker.picked .ring { stroke: #2f6fd6; stroke-width: 2.5; }
  svg .fact.dri.picked { text-decoration: underline; }
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

class DagendaView extends HTMLElement {
  static observedAttributes = ["src", "theme", "zoom"]

  #documents = [{ name: "", source: "" }]
  #problems = []
  #selected = null
  // The person picked, as in "@alice", or null
  #person = null
  #canvas
  #svg = null
  // id → ids it waits on, and id → ids waiting on it, from the drawn lines.
  // The lines leave out what longer paths imply, but following them still
  // reaches everything upstream and downstream.
  #up = new Map()
  #down = new Map()
  // Where the pointer is over the picture, in client coordinates, or null
  #pointer = null

  constructor() {
    super()
    const shadow = this.attachShadow({ mode: "open" })
    shadow.innerHTML = `<style>${STYLE}${EDITING}</style><div part="canvas"></div>`
    this.#canvas = shadow.querySelector("div")

    this.#canvas.addEventListener("pointerover", (e) => {
      // While someone's picked, their cards stay lit, wherever the pointer goes
      const node = e.target.closest?.(".node")
      if (node && this.#person === null) this.#focus(node.dataset.id)
    })
    this.#canvas.addEventListener("pointerleave", () => this.#focus(null))
    // Zoom keeps the point under the pointer still, so track where it is
    const track = (e) => {
      this.#pointer = { x: e.clientX, y: e.clientY }
    }
    this.addEventListener("pointerenter", track)
    this.addEventListener("pointermove", track)
    this.addEventListener("pointerleave", () => {
      this.#pointer = null
    })
    this.#canvas.addEventListener("click", (e) => {
      // A card's links open their page; they don't select the card
      if (e.target.closest?.("a")) return
      // A person's sticker, or their name on a card, picks them instead
      const person = e.target.closest?.(".sticker[data-person]")?.dataset.person ?? e.target.closest?.(".fact.dri")?.textContent.trim()
      if (person) {
        this.person = person === this.#person ? null : person
        return
      }
      const node = e.target.closest?.(".node")
      if (!node) this.person = null
      this.select(node && node.dataset.id !== this.#selected ? node.dataset.id : null)
    })
    this.addEventListener("keydown", (e) => {
      if (e.key !== "Escape") return
      this.select(null)
      this.person = null
    })
    attachEditing({
      host: this,
      root: shadow,
      canvas: this.#canvas,
      svg: () => this.#svg,
      documents: () => this.#documents,
    })
  }

  connectedCallback() {
    if (!this.hasAttribute("tabindex")) this.tabIndex = 0
    if (this.hasAttribute("src") || this.#documents.some((document) => document.source)) return
    const script = this.querySelector('script[type="text/dagenda"]')
    const text = (script ?? this).textContent ?? ""
    if (text.trim()) this.source = text
  }

  attributeChangedCallback(name, before, after) {
    if (name === "src") this.#load()
    else if (name === "zoom" && before !== after) this.#zoom()
    else this.#style()
  }

  /**
   * Resize to the new zoom, keeping one point of the picture still: the one
   * under the pointer if it's over the picture (a wheel or a pinch), else
   * the middle of what's showing (the page's zoom buttons). Whatever
   * scrolls the picture (this element, or the page's pane around it) scrolls
   * by however far that point moved.
   */
  #zoom() {
    const svg = this.#svg
    if (!svg || !this.isConnected) return this.#style()
    const before = svg.getBoundingClientRect()
    if (before.width === 0 || before.height === 0) return this.#style()
    const anchor = this.#pointer ?? this.#middle()
    // Where the anchor is across the picture, as fractions of its size
    const across = (anchor.x - before.left) / before.width
    const down = (anchor.y - before.top) / before.height

    this.#style()
    const after = svg.getBoundingClientRect()
    let dx = after.left + across * after.width - anchor.x
    let dy = after.top + down * after.height - anchor.y
    for (const scroller of this.#scrollers()) {
      if (Math.abs(dx) < 0.5 && Math.abs(dy) < 0.5) break
      const [left, top] = [scroller.scrollLeft, scroller.scrollTop]
      scroller.scrollLeft += dx
      scroller.scrollTop += dy
      dx -= scroller.scrollLeft - left
      dy -= scroller.scrollTop - top
    }
  }

  /** This element, then each ancestor that clips or scrolls it, then the page. */
  #scrollers() {
    const found = []
    for (let element = this; element; element = element.parentElement) {
      const { overflowX, overflowY } = getComputedStyle(element)
      if (element === this || overflowX !== "visible" || overflowY !== "visible") found.push(element)
    }
    if (document.scrollingElement && !found.includes(document.scrollingElement)) found.push(document.scrollingElement)
    return found
  }

  /** The middle of the part of the picture that's showing, in client coordinates. */
  #middle() {
    let { left, top, right, bottom } = this.getBoundingClientRect()
    for (const clip of [...this.#scrollers().slice(1).map((element) => element.getBoundingClientRect()), { left: 0, top: 0, right: innerWidth, bottom: innerHeight }]) {
      left = Math.max(left, clip.left)
      top = Math.max(top, clip.top)
      right = Math.min(right, clip.right)
      bottom = Math.min(bottom, clip.bottom)
    }
    return { x: (left + right) / 2, y: (top + bottom) / 2 }
  }

  /** The plan's source text, for a plan of one document. Setting it redraws. */
  get source() {
    return this.#documents[0]?.source ?? ""
  }

  set source(text) {
    this.documents = [{ name: "", source: text }]
  }

  /** The plan's documents, `[{ name, source }]`, in order. Setting them redraws. */
  get documents() {
    return this.#documents.map((document) => ({ ...document }))
  }

  set documents(documents) {
    this.#documents = documents.map(({ name = "", source = "" }) => ({ name, source }))
    this.#render()
  }

  /** The problems with the plan: `{ document, line, column, severity, message }`. */
  get problems() {
    return this.#problems
  }

  /** The selected node's id, or null. */
  get selected() {
    return this.#selected
  }

  /** The person picked, as in "@alice", or null. */
  get person() {
    return this.#person
  }

  /**
   * Pick a person, as in "@alice", to see only what they're assigned to (null
   * shows everything), firing `dagenda-person`.
   */
  set person(handle) {
    const person = handle || null
    if (person === this.#person) return
    this.#person = person
    this.#focus(null)
    this.#pick()
    this.dispatchEvent(new CustomEvent("dagenda-person", { bubbles: true, detail: { handle: person } }))
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
    const documents = this.#documents
    const problems = checkPlan(documents)
    // Newer documents arrived while these waited for the Wasm
    if (documents !== this.#documents) return

    const ok = !problems.some((problem) => problem.severity === "error")
    if (ok) this.#draw(renderPlanSvg(documents))
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
    this.#pick()
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

  /**
   * Mark what the picked person is assigned to: cards and banners that list
   * them, and the lines between two of their cards. The rest fades.
   */
  #pick() {
    if (!this.#svg) return
    const person = this.#person
    this.#svg.classList.toggle("picking", person !== null)
    const theirs = (element) => person !== null && (element.dataset.people ?? "").split(" ").includes(person)
    const assigned = new Set()
    for (const node of this.#svg.querySelectorAll(".node")) {
      node.classList.toggle("assigned", theirs(node))
      if (theirs(node)) assigned.add(node.dataset.id)
    }
    for (const banner of this.#svg.querySelectorAll(".banner")) banner.classList.toggle("assigned", theirs(banner))
    for (const edge of this.#svg.querySelectorAll(".edge")) {
      edge.classList.toggle("assigned", assigned.has(edge.dataset.from) && assigned.has(edge.dataset.to))
    }
    for (const sticker of this.#svg.querySelectorAll(".sticker")) {
      sticker.classList.toggle("picked", person !== null && sticker.dataset.person === person)
    }
    for (const name of this.#svg.querySelectorAll(".fact.dri")) {
      name.classList.toggle("picked", person !== null && name.textContent.trim() === person)
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
