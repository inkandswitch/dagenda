// Edits made on the picture, for <dagenda-view editable>. The picture never
// changes the plan itself: each gesture asks the Wasm for an edit, which
// comes back as the changed documents' new text, and fires `dagenda-edit`
// for the page to apply (as the smallest splice, so it joins that person's
// undo and merges with typing). An edit that makes no sense (a loop, a
// milestone below one it builds on) shows its reason instead.
//
//   click a card's badge          a menu of the eight status boxes
//   drag from a badge to a card    "this needs that"
//   drag a card into a band        re-scope it to that milestone (the top layer: none)
//   drag a milestone (its gate or  move its boundary: up past cards brings them in,
//     its line)                    down past cards hands them to the era above;
//                                  past another milestone's line, reorder the two

import { addNeed, reorder, rescope, setStatus } from "./pkg/dagenda_wasm.js"

/** The status boxes, in the menu's order. */
const STATUSES = [
  [" ", "Not started"],
  ["-", "In progress"],
  ["x", "Done"],
  ["!", "Needs attention"],
  ["*", "Research"],
  ["?", "Idea"],
  [">", "Deferred"],
  ["~", "Dropped"],
]

/** How far a press moves before it's a drag, in CSS pixels. */
const SLOP = 4

export const STYLE = `
  :host([editable]) svg .node.task { cursor: grab; }
  :host([editable]) svg :is(.node.milestone, .era) { cursor: ns-resize; }
  :host([editable]) svg .badge { cursor: pointer; }
  :host([editable]) svg :is(.mark, .check) { pointer-events: none; }
  svg .dragging { opacity: 0.8; pointer-events: none; cursor: grabbing; }
  svg .rubber-band { stroke: #2f6fd6; stroke-width: 2; stroke-dasharray: 4 3; pointer-events: none; }
  svg.dagenda .node.drop-target :is(.card, .gate) { stroke: #2f6fd6; stroke-width: 3; }
  svg .drop-band { fill: #2f6fd6; fill-opacity: 0.08; pointer-events: none; }
  .menu {
    position: absolute; z-index: 2; display: grid; gap: 2px; padding: 4px;
    border: 1px solid #c9c4bb; border-radius: 8px; background: #fff; color: #1d1c1a;
    box-shadow: 0 4px 16px rgb(0 0 0 / 0.15); font: 13px system-ui, sans-serif;
  }
  .menu button {
    display: flex; gap: 8px; padding: 3px 8px; border: 0; border-radius: 5px;
    background: none; color: inherit; font: inherit; text-align: left; cursor: pointer;
  }
  .menu button:hover, .menu button:focus-visible { background: rgb(47 111 214 / 0.12); outline: none; }
  .menu button[aria-pressed="true"] { font-weight: 650; }
  .menu code { font: 12px ui-monospace, "SF Mono", Menlo, Consolas, monospace; }
  .toast {
    position: absolute; z-index: 3; width: max-content; max-width: 360px;
    padding: 6px 10px; border-radius: 6px; box-shadow: 0 2px 8px rgb(0 0 0 / 0.15);
    background: #fdeceb; color: #c62f27; font: 13px system-ui, sans-serif; pointer-events: none;
  }
  @media (prefers-color-scheme: dark) {
    :host(:not([theme="light"])) .menu { border-color: #3a3834; background: #1f1e1c; color: #ecebe8; }
    :host(:not([theme="light"])) .toast { background: #3a1f1f; color: #e5615a; }
  }
  :host([theme="dark"]) .menu { border-color: #3a3834; background: #1f1e1c; color: #ecebe8; }
  :host([theme="dark"]) .toast { background: #3a1f1f; color: #e5615a; }
`

/**
 * Wire picture edits into a <dagenda-view>.
 *
 * @param {object} view
 * @param {HTMLElement} view.host  the element (for `editable`, and events)
 * @param {ShadowRoot} view.root
 * @param {HTMLElement} view.canvas  holds the SVG
 * @param {() => SVGSVGElement | null} view.svg  the picture on show
 * @param {() => { name: string, source: string }[]} view.documents  the plan's documents now
 */
export const attachEditing = ({ host, root, canvas, svg, documents }) => {
  let gesture = null
  let dragged = false
  let menu = null
  let toast = null

  const editable = () => host.hasAttribute("editable")

  /** A point on the page, in the SVG's own units. */
  const at = (clientX, clientY) => {
    const picture = svg()
    const matrix = picture?.getScreenCTM()?.inverse()
    return matrix ? new DOMPoint(clientX, clientY).matrixTransform(matrix) : new DOMPoint(clientX, clientY)
  }

  /** Each milestone's line, top to bottom. */
  const eras = () =>
    [...(svg()?.querySelectorAll(".era") ?? [])]
      .map((era) => ({ id: era.dataset.milestone, y: Number(era.querySelector("line")?.getAttribute("y1")) }))
      .sort((a, b) => a.y - b.y)

  /** The band at height `y`: the milestone whose line is the nearest above it, or null on top. */
  const bandAt = (y) => eras().filter((era) => era.y < y).at(-1)?.id ?? null

  /** Task cards whose middles lie between heights `top` and `bottom`, top first. */
  const cardsBetween = (top, bottom) =>
    [...(svg()?.querySelectorAll(".node.task") ?? [])]
      .map((node) => {
        const card = node.querySelector(".card")
        const middle = Number(card?.getAttribute("y")) + Number(card?.getAttribute("height")) / 2
        return { node, id: node.dataset.id, era: node.dataset.era ?? null, middle }
      })
      .filter((card) => card.middle > top && card.middle < bottom)
      .sort((a, b) => a.middle - b.middle)

  /**
   * What dragging milestone `id`'s line to height `y` does: past another
   * milestone's line, reorder the two; else move its boundary over the cards
   * between, up to bring them in, down to hand them to the era above.
   */
  const boundary = (id, y) => {
    const lines = eras()
    const own = lines.find((era) => era.id === id)
    if (!own) return { lines }
    const passedUp = lines.filter((era) => era.id !== id && era.y < own.y && era.y > y)
    const passedDown = lines.filter((era) => era.id !== id && era.y > own.y && era.y < y)
    if (passedUp.length > 0) return { lines, reorder: { other: passedUp[0].id, above: true } }
    if (passedDown.length > 0) return { lines, reorder: { other: passedDown.at(-1).id, above: false } }
    const above = lines.filter((era) => era.y < own.y).at(-1)?.id ?? null
    return y < own.y
      ? { lines, into: cardsBetween(y, own.y) }
      : { lines, out: cardsBetween(own.y, y).filter((card) => card.era === id), above }
  }

  // --- Making an edit ----------------------------------------------------------

  /** Say why, where the gesture ended (the picture may be scrolled anywhere). */
  const say = (message, { clientX, clientY }) => {
    toast?.remove()
    toast = Object.assign(document.createElement("div"), { className: "toast", textContent: message })
    toast.setAttribute("role", "status")
    const frame = host.getBoundingClientRect()
    toast.style.left = `${Math.max(8, clientX - frame.left + host.scrollLeft - 24)}px`
    toast.style.top = `${clientY - frame.top + host.scrollTop + 16}px`
    root.append(toast)
    // Kept inside what's on show (the window, or the pane the picture
    // scrolls in, less its scrollbar): a drop by the gates is near its edge
    const box = toast.getBoundingClientRect()
    let pane = host.parentElement
    while (pane && !/auto|scroll/.test(getComputedStyle(pane).overflowX)) pane = pane.parentElement
    const right =
      Math.min(document.documentElement.clientWidth, pane ? pane.getBoundingClientRect().left + pane.clientWidth : Infinity) - 8
    if (box.right > right) toast.style.left = `${parseFloat(toast.style.left) - Math.min(box.right - right, box.left - 8)}px`
    const shown = toast
    setTimeout(() => shown.remove(), 4000)
  }

  /**
   * Run each of `makes` (Wasm edits) in turn on the plan's documents, each on
   * the text the one before left, and hand the result to the page as one
   * edit. Any refused say why, and the rest still go.
   */
  const edit = (kind, where, ...makes) => {
    const drawn = documents()
    const plan = drawn.map(({ name, source }) => ({ name, source }))
    const touched = new Set()
    const refused = []
    for (const make of makes) {
      try {
        for (const made of make(plan)) {
          plan[made.document] = { ...plan[made.document], source: made.source }
          touched.add(made.document)
        }
      } catch (error) {
        refused.push(error.message ?? String(error))
      }
    }
    if (refused.length > 0) say([...new Set(refused)].join("; "), where)
    if (touched.size === 0) return
    const edits = [...touched].map((document) => ({ document, source: plan[document].source, base: drawn[document]?.source ?? "" }))
    host.dispatchEvent(new CustomEvent("dagenda-edit", { bubbles: true, detail: { kind, edits } }))
  }

  // --- The status menu -----------------------------------------------------------

  const closeMenu = () => {
    menu?.remove()
    menu = null
  }

  const openMenu = (node, badge) => {
    closeMenu()
    const id = node.dataset.id
    const current = node.querySelector(".mark")?.textContent ?? (node.classList.contains("done") ? "x" : " ")
    const place = badge.getBoundingClientRect()
    const frame = host.getBoundingClientRect()
    menu = document.createElement("div")
    menu.className = "menu"
    menu.setAttribute("role", "menu")
    menu.style.left = `${place.left - frame.left + host.scrollLeft}px`
    menu.style.top = `${place.bottom - frame.top + host.scrollTop + 4}px`
    for (const [mark, label] of STATUSES) {
      const button = document.createElement("button")
      button.type = "button"
      button.setAttribute("role", "menuitem")
      button.setAttribute("aria-pressed", String(mark === current))
      button.innerHTML = `<code>[${mark === " " ? "&nbsp;" : mark}]</code>`
      button.append(label)
      button.addEventListener("click", () => {
        closeMenu()
        edit("status", { clientX: place.left, clientY: place.bottom }, (plan) => setStatus(plan, id, mark))
      })
      menu.append(button)
    }
    root.append(menu)
    menu.querySelector('[aria-pressed="true"]')?.focus() ?? menu.querySelector("button")?.focus()
  }

  // --- Dragging --------------------------------------------------------------------

  const clear = () => {
    const picture = svg()
    picture?.querySelectorAll(".rubber-band, .drop-band").forEach((element) => element.remove())
    picture?.querySelectorAll(".drop-target").forEach((element) => element.classList.remove("drop-target"))
    picture?.querySelectorAll(".dragging").forEach((element) => {
      element.classList.remove("dragging")
      element.removeAttribute("transform")
    })
  }

  const nodeAt = (clientX, clientY, except) => {
    const node = root.elementFromPoint(clientX, clientY)?.closest?.(".node")
    return node && node !== except ? node : null
  }

  const svgElement = (name, attributes) => {
    const element = document.createElementNS("http://www.w3.org/2000/svg", name)
    for (const [key, value] of Object.entries(attributes)) element.setAttribute(key, String(value))
    return element
  }

  /** Show where a drag would land. */
  const follow = (e) => {
    const picture = svg()
    if (!picture) return
    clear()
    const here = at(e.clientX, e.clientY)
    const { kind, node, from } = gesture

    if (kind === "badge") {
      const badge = node.querySelector(".badge")
      const line = svgElement("line", {
        class: "rubber-band",
        x1: badge.getAttribute("cx"),
        y1: badge.getAttribute("cy"),
        x2: here.x,
        y2: here.y,
      })
      picture.append(line)
      nodeAt(e.clientX, e.clientY, node)?.classList.add("drop-target")
      return
    }

    const dy = here.y - from.y
    if (kind === "card") {
      node.classList.add("dragging")
      node.setAttribute("transform", `translate(${here.x - from.x} ${dy})`)
      // The band it would land in, from its line down to the next
      const lines = eras()
      const band = bandAt(here.y)
      const { width, height: tall } = picture.viewBox.baseVal
      const top = band === null ? 0 : (lines.find((era) => era.id === band)?.y ?? 0)
      const bottom = band === null ? (lines[0]?.y ?? tall) : (lines.find((era) => era.y > top)?.y ?? tall)
      picture.insertBefore(
        svgElement("rect", { class: "drop-band", x: 0, y: top, width, height: Math.max(0, bottom - top) }),
        picture.querySelector(".node"),
      )
      return
    }

    // A gate moves with its line, and the cards it would move light up
    node.classList.add("dragging")
    node.setAttribute("transform", `translate(0 ${dy})`)
    const line = picture.querySelector(`.era[data-milestone="${CSS.escape(node.dataset.id)}"]`)
    line?.classList.add("dragging")
    line?.setAttribute("transform", `translate(0 ${dy})`)
    const { into = [], out = [] } = boundary(node.dataset.id, Number(line?.querySelector("line")?.getAttribute("y1")) + dy)
    for (const card of [...into, ...out]) card.node.classList.add("drop-target")
  }

  /** Make the edit a drag asks for. */
  const drop = (e) => {
    const { kind, node, from } = gesture
    const id = node.dataset.id
    const here = at(e.clientX, e.clientY)

    if (kind === "badge") {
      const target = nodeAt(e.clientX, e.clientY, node)
      if (target) edit("need", e, (plan) => addNeed(plan, id, target.dataset.id))
      return
    }

    if (kind === "card") {
      if (eras().length === 0) return
      const was = node.dataset.era ?? null
      const to = bandAt(here.y)
      if (to !== was) edit("rescope", e, (plan) => rescope(plan, id, was, to))
      return
    }

    // A milestone's line, moved: past another, reorder; else move the
    // boundary over the cards it crossed
    const own = eras().find((era) => era.id === id)
    if (!own) return
    const moved = boundary(id, own.y + (here.y - from.y))
    if (moved.reorder) {
      const { other, above } = moved.reorder
      edit("reorder", e, (plan) => reorder(plan, id, other, above))
    } else if (moved.into?.length > 0) {
      // Brought in: this milestone needs them now (a later one still may)
      edit("rescope", e, ...moved.into.map((card) => (plan) => rescope(plan, card.id, null, id)))
    } else if (moved.out?.length > 0) {
      // Handed up: off this milestone, onto the one above (or none)
      edit(
        "rescope",
        e,
        ...moved.out.map((card) => (plan) => rescope(plan, card.id, id, moved.above)),
      )
    } else if (moved.lines.length < 2) {
      say("Drag the line past cards to bring them into this milestone, or to hand them to the era above", e)
    }
  }

  // --- Wiring -----------------------------------------------------------------------

  canvas.addEventListener("pointerdown", (e) => {
    if (!editable() || e.button !== 0 || e.target.closest?.("a")) return
    // A milestone's line (or its label) moves it as its gate does
    const era = e.target.closest?.(".era")
    const node =
      e.target.closest?.(".node") ??
      (era && svg()?.querySelector(`.node.milestone[data-id="${CSS.escape(era.dataset.milestone)}"]`))
    if (!node) return
    const kind = node.classList.contains("milestone")
      ? "gate"
      : e.target.closest?.(".badge")
        ? "badge"
        : "card"
    gesture = { kind, node, start: { x: e.clientX, y: e.clientY }, from: at(e.clientX, e.clientY) }
    dragged = false
    e.preventDefault()
  })

  window.addEventListener("pointermove", (e) => {
    if (!gesture) return
    if (!dragged && Math.hypot(e.clientX - gesture.start.x, e.clientY - gesture.start.y) < SLOP) return
    dragged = true
    follow(e)
  })

  window.addEventListener("pointerup", (e) => {
    if (!gesture) return
    const ended = gesture
    if (dragged) {
      clear()
      drop(e)
    } else if (ended.kind === "badge") {
      openMenu(ended.node, ended.node.querySelector(".badge"))
      dragged = true // the click that follows shouldn't select the card
    }
    gesture = null
  })

  window.addEventListener("pointercancel", () => {
    clear()
    gesture = null
  })

  // After a drag (or a badge click), the click that follows isn't a selection
  canvas.addEventListener(
    "click",
    (e) => {
      if (!dragged) return
      dragged = false
      e.stopImmediatePropagation()
    },
    { capture: true },
  )

  // Escape or a click elsewhere closes the menu
  host.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && menu) {
      e.stopPropagation()
      closeMenu()
    }
  })
  window.addEventListener("pointerdown", (e) => {
    if (menu && !e.composedPath().includes(menu)) closeMenu()
  })
  host.addEventListener("scroll", closeMenu)
}
