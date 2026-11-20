// Who's here, and where, over each document's ephemeral messages
// (automerge-repo's Presence, which Subduction carries but never stores). A
// plan is several documents, so it comes in two parts: a roster on the
// workspace (who's here, and on which tab), and carets on each plan document. Selections travel as Automerge cursors, so a caret
// stays on its character as edits arrive from anyone.

import { getCursor, getCursorPosition } from "@automerge/automerge"
import { Presence } from "@automerge/automerge-repo"
import { StateEffect, StateField } from "@codemirror/state"
import { Decoration, EditorView, WidgetType } from "@codemirror/view"

// People's colours, by the name they go by: apart from the status colours,
// so nobody reads a caret as a status
const COLOURS = {
  blue: "#1f78b4",
  brown: "#8b4513",
  gold: "#b8860b",
  green: "#2e8b57",
  pink: "#c71585",
  purple: "#6a3d9a",
  red: "#d0473d",
  teal: "#008b8b",
}
const KNOWN = new Set(Object.values(COLOURS))

// What someone else says about themselves: anyone with the workspace's URL
// can send anything, and a colour lands in a style attribute. So take a
// colour only from our own list, and a name or a tab only as short text
const short = (text, fallback) => (typeof text === "string" && text.trim() ? text.trim().slice(0, 40) : fallback)
const parsePerson = (user) => ({
  name: short(user?.name, "someone"),
  colour: KNOWN.has(user?.colour) ? user.colour : "#888888",
})

const MOODS = ["brave", "calm", "cheeky", "clever", "cosy", "curious", "dapper", "eager", "fuzzy", "gentle", "giddy", "jolly", "lucky", "merry", "nimble", "plucky", "quiet", "silly", "sleepy", "sunny", "swift", "witty", "zesty"]
const ANIMALS = ["badger", "beaver", "crane", "duck", "ferret", "fox", "frog", "goose", "hare", "hedgehog", "heron", "koala", "lemur", "lynx", "moose", "newt", "otter", "owl", "panda", "puffin", "quokka", "raven", "seal", "stoat", "toad", "walrus", "wombat", "yak"]
const PATH = ["source"]
const HEARTBEAT = { heartbeatMs: 5_000, peerTtlMs: 15_000 }
const EVENTS = ["update", "snapshot", "goodbye", "pruning"]

/**
 * This person: a name like silly-teal-otter, in that colour, kept per
 * browser through `load` and `save`.
 */
export const identity = ({ load, save }) => {
  const random = (items) => items[Math.floor(Math.random() * items.length)]
  const hue = random(Object.keys(COLOURS))
  const saved = load("name", "")
  const user =
    saved && COLOURS[load("colour", "")]
      ? { name: saved, colour: COLOURS[load("colour", "")], hue: load("colour", "") }
      : { name: `${random(MOODS)}-${hue}-${random(ANIMALS)}`, colour: COLOURS[hue], hue }
  save("name", user.name)
  save("colour", user.hue)
  return user
}

/**
 * Who's here, on the workspace: listed in `list`, each with the tab they're
 * on. Clicking your own name renames you, and calls `onRename`.
 *
 * @param {object} options
 * @param {import("@automerge/automerge-repo").DocHandle} options.handle  the workspace
 * @param {{ name: string, colour: string }} options.user
 * @param {HTMLElement} options.list
 * @param {(key: string, value: string) => void} options.save
 * @param {() => void} [options.onRename]
 */
export const roster = ({ handle, user, list, save, onRename = () => {} }) => {
  const shared = new Presence({ handle })
  shared.start({ initialState: { user, tab: "" }, ...HEARTBEAT })

  const chip = ({ name, colour, tab }, you = false) => {
    const element = document.createElement(you ? "button" : "span")
    element.className = "person"
    element.style.setProperty("--person", colour)
    element.textContent = you ? `${name} (you)` : name
    if (tab) element.title = `on ${tab}`
    if (you) {
      element.type = "button"
      element.title = "Change your name"
      element.addEventListener("click", () => {
        const name = prompt("Your name, as others see it", user.name)?.trim()
        if (!name) return
        user.name = name
        save("name", name)
        shared.broadcast("user", { ...user })
        onRename()
        draw()
      })
    }
    return element
  }

  const draw = () => {
    const others = Object.values(shared.getPeerStates().value).map(({ value }) => ({
      ...parsePerson(value.user),
      tab: short(value.tab, ""),
    }))
    list.replaceChildren(chip(user, true), ...others.map((person) => chip(person)))
  }

  for (const event of EVENTS) shared.on(event, draw)
  const leave = () => shared.stop()
  addEventListener("pagehide", leave)
  draw()
  return {
    /** Say which tab this person is on. */
    on: (tab) => shared.broadcast("tab", tab),
    stop: () => {
      removeEventListener("pagehide", leave)
      for (const event of EVENTS) shared.off(event, draw)
      if (shared.running) shared.stop()
    },
  }
}

// --- Carets ----------------------------------------------------------------------

const others = StateEffect.define()

class Caret extends WidgetType {
  constructor(name, colour) {
    super()
    this.name = name
    this.colour = colour
  }

  eq(other) {
    return other.name === this.name && other.colour === this.colour
  }

  toDOM() {
    const caret = Object.assign(document.createElement("span"), { className: "cm-remote-caret" })
    caret.style.borderLeftColor = this.colour
    const label = Object.assign(document.createElement("span"), { className: "cm-remote-name", textContent: this.name })
    label.style.backgroundColor = this.colour
    caret.append(label)
    return caret
  }

  ignoreEvent() {
    return true
  }
}

const decorations = (people) =>
  Decoration.set(
    people.flatMap(({ name, colour, from, to, head }) => [
      ...(from < to ? [Decoration.mark({ attributes: { style: `background-color:${colour}33` } }).range(from, to)] : []),
      Decoration.widget({ widget: new Caret(name, colour), side: 1 }).range(head),
    ]),
    true,
  )

const field = StateField.define({
  create: () => Decoration.none,
  update: (value, tr) => {
    const next = tr.effects.findLast((e) => e.is(others))
    return next ? decorations(next.value) : value.map(tr.changes)
  },
  provide: (f) => EditorView.decorations.from(f),
})

const style = EditorView.baseTheme({
  ".cm-remote-caret": { position: "relative", borderLeft: "2px solid", marginLeft: "-1px", marginRight: "-1px" },
  ".cm-remote-name": {
    position: "absolute",
    bottom: "100%",
    left: "-2px",
    padding: "0 4px",
    borderRadius: "3px 3px 3px 0",
    color: "white",
    font: "600 10px/1.5 system-ui, sans-serif",
    whiteSpace: "nowrap",
    pointerEvents: "none",
    opacity: "0.9",
  },
})

/** CodeMirror extensions that draw other people's selections. */
export const remoteCarets = [field, style]

/**
 * Share this person's selection in `view`, a plan document's text, and draw
 * everyone else's.
 *
 * @param {object} options
 * @param {import("@automerge/automerge-repo").DocHandle} options.handle  the plan document
 * @param {EditorView} options.view
 * @param {{ name: string, colour: string }} options.user
 */
export const carets = ({ handle, view, user }) => {
  // Cursors at the selection, or none while the document hasn't caught up
  // with the view
  const selection = () => {
    const doc = handle.doc()
    const { anchor, head } = view.state.selection.main
    try {
      return { anchor: getCursor(doc, PATH, anchor), head: getCursor(doc, PATH, head) }
    } catch {
      return null
    }
  }

  const shared = new Presence({ handle })
  shared.start({ initialState: { user, selection: null }, ...HEARTBEAT })

  // Everyone else with a selection, resolved against this copy (a cursor
  // this copy hasn't seen yet just leaves the caret out for now)
  const people = () =>
    Object.values(shared.getPeerStates().value).flatMap(({ value }) => {
      if (!value.selection) return []
      try {
        const length = view.state.doc.length
        const at = (cursor) => Math.min(getCursorPosition(handle.doc(), PATH, cursor), length)
        const anchor = at(value.selection.anchor)
        const head = at(value.selection.head)
        return [{ ...parsePerson(value.user), from: Math.min(anchor, head), to: Math.max(anchor, head), head }]
      } catch {
        return []
      }
    })

  let live = true
  // Deferred: this can run from inside a CodeMirror update
  const draw = () => queueMicrotask(() => live && view.dispatch({ effects: others.of(people()) }))
  for (const event of EVENTS) shared.on(event, draw)
  handle.on("change", draw)

  // Share the selection once the view and the document agree, at most once
  // a frame
  let pending = false
  const share = () => {
    if (pending) return
    pending = true
    requestAnimationFrame(() => {
      pending = false
      if (!live) return
      const next = selection()
      if (next) shared.broadcast("selection", next)
    })
  }

  // Leaving says goodbye, so others drop this caret at once
  const leave = () => shared.stop()
  addEventListener("pagehide", leave)
  draw()
  return {
    /** For an EditorView update listener: share selection moves and edits. */
    update: (update) => {
      if (update.selectionSet || update.docChanged) share()
    },
    /** Tell everyone this person's (new) name. */
    announce: () => shared.broadcast("user", { ...user }),
    stop: () => {
      live = false
      removeEventListener("pagehide", leave)
      for (const event of EVENTS) shared.off(event, draw)
      handle.off("change", draw)
      if (shared.running) shared.stop()
    },
  }
}
