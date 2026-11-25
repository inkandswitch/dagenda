// The editor both pages share: a plan's documents, a tab each, their text in
// CodeMirror, and one picture drawn from all of them, in the page shell
// (shell.js). Where the documents live is the page's: a `store`. The
// preview's is one document in this browser (preview.js); the
// collaborative editor's is an Automerge workspace (collab.js).

import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete"
import {
  defaultKeymap,
  history,
  historyKeymap,
  indentLess,
  indentMore,
  redo,
  redoDepth,
  undo,
  undoDepth,
} from "@codemirror/commands"
import { highlightSelectionMatches, searchKeymap } from "@codemirror/search"
import { ChangeSet, EditorState, Transaction } from "@codemirror/state"
import {
  EditorView,
  drawSelection,
  dropCursor,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  lineNumbers,
} from "@codemirror/view"
import init, { locatePlan, renderPlanText } from "../../dagenda-view/pkg/dagenda_wasm.js"
import "../../dagenda-view/dagenda-view.js"
import { BASIC, TOUR } from "./examples.js"
import { highlighting, paint } from "./highlighting.js"
import { shell } from "./shell.js"

/**
 * Where a plan's documents live.
 *
 * @typedef {object} Store
 * @property {() => { id: string, name: string }[]} list  the documents, in order
 * @property {(onChange: () => void) => void} watch  call `onChange` when the list changes
 * @property {(id: string) => Promise<Document>} open  a document, to show in a tab
 * @property {(id: string, name: string) => void} [rename]  without it, tabs can't be renamed
 * @property {(name: string) => Promise<string>} [create]  a new document's id; without it, there's no _+_
 * @property {(page: ReturnType<typeof shell>) => Presence} [presence]  who else is here
 *
 * One document's text.
 *
 * @typedef {object} Document
 * @property {() => string} source  its text, now
 * @property {import("@codemirror/state").Extension} extensions  what keeps the text and the store in step
 * @property {(onChange: () => void) => () => void} subscribe  call `onChange` on every change; returns how to stop
 *
 * Everyone else here: their carets in each tab, and which tab this person is on.
 *
 * @typedef {object} Presence
 * @property {import("@codemirror/state").Extension} extensions
 * @property {(document: Document, view: EditorView) => { update: (update: object) => void, stop: () => void }} attach
 * @property {(name: string) => void} on  this person is on the tab called `name`
 */

// CodeMirror's basic setup, less what needs a language (folding, its
// highlighting: ours is highlighting.js) or has nothing to do here
// (autocomplete, lint)
const setup = [
  lineNumbers(),
  highlightActiveLineGutter(),
  highlightSpecialChars(),
  history(),
  drawSelection(),
  dropCursor(),
  EditorState.allowMultipleSelections.of(true),
  closeBrackets(),
  highlightActiveLine(),
  highlightSelectionMatches(),
  EditorState.tabSize.of(2),
  keymap.of([...closeBracketsKeymap, ...defaultKeymap, ...searchKeymap, ...historyKeymap]),
]

// Indentation is structure: Enter keeps the line's indent; Tab indents with
// two spaces (every line a selection touches), and Shift+Tab outdents
const indentation = keymap.of([
  {
    key: "Enter",
    run: (view) => {
      const { head } = view.state.selection.main
      const indent = /^ */.exec(view.state.doc.lineAt(head).text)[0]
      view.dispatch(view.state.replaceSelection(`\n${indent}`), { userEvent: "input" })
      return true
    },
  },
  {
    key: "Tab",
    run: (view) => {
      if (!view.state.selection.ranges.every((range) => range.empty)) return indentMore(view)
      view.dispatch(view.state.replaceSelection("  "), { userEvent: "input" })
      return true
    },
    shift: indentLess,
  },
])

// The text in the page's colours, so it follows the theme
const theme = EditorView.theme({
  "&": { color: "var(--fg)", backgroundColor: "transparent", height: "100%" },
  ".cm-content": { font: '13px/20px ui-monospace, "SF Mono", Menlo, Consolas, monospace', caretColor: "var(--fg)" },
  ".cm-cursor": { borderLeftColor: "var(--fg)" },
  ".cm-gutters": { backgroundColor: "var(--panel)", color: "var(--muted)", borderRight: "1px solid var(--border)" },
  ".cm-activeLine, .cm-activeLineGutter": { backgroundColor: "rgb(127 127 127 / 0.08)" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": { backgroundColor: "rgb(47 111 214 / 0.25)" },
})

// Undo is per person: only this person's edits (typed, or an example
// loaded) go into the history. Remote changes carry no user event, so they
// stay out of it, and undo never reverts someone else
const ownEditsOnly = EditorState.transactionExtender.of((tr) =>
  tr.docChanged && !tr.annotation(Transaction.userEvent) ? { annotations: Transaction.addToHistory.of(false) } : null,
)

// Positions are Automerge's, so a `\r` stays a character, as it is there
// (CodeMirror would otherwise read `\r\n` as one line break, one short)
const sameLines = EditorState.lineSeparator.of("\n")

/** The smallest change from `from` to `to`, as one splice. */
const splice = (from, to) => {
  let start = 0
  while (start < from.length && start < to.length && from[start] === to[start]) start++
  let end = 0
  while (end < from.length - start && end < to.length - start && from.at(-1 - end) === to.at(-1 - end)) end++
  return ChangeSet.of({ from: start, to: from.length - end, insert: to.slice(start, to.length - end) }, from.length)
}

/**
 * Mount the editor on `store`, in the page's ids (src/panes.html).
 *
 * @param {object} options
 * @param {string} options.prefix  key prefix for per-viewer settings in localStorage
 * @param {(page: ReturnType<typeof shell>) => Store} options.store  given the page, for its settings
 * @param {string} [options.help]  this page's own help, as `<h3>` sections
 * @returns {Promise<{ page: ReturnType<typeof shell>, active: () => object | undefined, tabs: () => object[] }>}
 */
export const mountEditor = async ({ prefix, store: makeStore, help = "" }) => {
  const $ = (id) => document.getElementById(id)
  const picture = $("picture")
  await init()

  /** @type {{ id: string, name: string, document: Document, view: EditorView, button: HTMLButtonElement }[]} */
  let tabs = []
  let active = 0
  let wanted = null // a tab's id to show once it's open

  const documents = () => tabs.map((tab) => ({ name: tab.name, source: tab.document.source() }))
  const page = shell({
    prefix,
    documents,
    help,
    goTo: (document, line) => goTo(document, line),
    onRender: (problems) => {
      // Each tab marks its own error (deferred: a render can follow a
      // change made inside CodeMirror's own update)
      queueMicrotask(() =>
        tabs.forEach((tab, index) => {
          const error = problems.find((problem) => problem.severity === "error" && problem.document === index)
          tab.view.dispatch({ effects: paint.of(error?.line ?? null) })
          tab.button.classList.toggle("has-error", Boolean(error))
        }),
      )
    },
    wasm: { renderPlanText },
  })
  const store = makeStore(page)
  const presence = store.presence?.(page)
  $("add-document").hidden = !store.create

  // --- The picture, from every document ------------------------------------

  let queued = false
  const draw = () => {
    if (queued) return
    queued = true
    requestAnimationFrame(() => {
      queued = false
      picture.documents = documents()
    })
  }

  // Edits made on the picture arrive as each document's new text, made from
  // the text the picture had. Applied to that tab as the smallest splice, as
  // if typed: they join this person's undo, and anything typed since (by
  // anyone) stays, with the edit moved over it
  picture.addEventListener("dagenda-edit", ({ detail: { edits } }) => {
    for (const { document, source: next, base } of edits) {
      const view = tabs[document]?.view
      if (!view) continue
      const current = view.state.doc.toString()
      let changes = splice(base, next)
      if (current !== base) changes = changes.map(splice(base, current))
      view.dispatch({ changes, userEvent: "input.picture" })
    }
  })

  // Clicking a box shows its line, in whichever document defines it
  picture.addEventListener("dagenda-select", ({ detail: { id } }) => {
    if (id === null) return
    const place = locatePlan(documents(), id)
    if (place) goTo(place.document, place.line)
  })

  // --- Tabs ------------------------------------------------------------------

  const syncHistory = () => {
    const state = tabs[active]?.view.state
    $("undo").disabled = !state || undoDepth(state) === 0
    $("redo").disabled = !state || redoDepth(state) === 0
  }

  const show = (index) => {
    active = Math.max(0, Math.min(index, tabs.length - 1))
    tabs.forEach((tab, at) => {
      // Important, to beat CodeMirror's own `display: flex !important`
      if (at === active) tab.view.dom.style.removeProperty("display")
      else tab.view.dom.style.setProperty("display", "none", "important")
      tab.button.setAttribute("aria-selected", String(at === active))
    })
    presence?.on(tabs[active]?.name ?? "")
    syncHistory()
  }

  const goTo = (document, line) => {
    const tab = tabs[document]
    if (!tab) return
    show(document)
    const doc = tab.view.state.doc
    const { from, to } = doc.line(Math.max(1, Math.min(line, doc.lines)))
    tab.view.dispatch({ selection: { anchor: from, head: to }, effects: EditorView.scrollIntoView(from, { y: "center" }) })
    tab.view.focus()
  }

  /** A tab for the document `id`, called `name`. */
  const open = async (id, name) => {
    const document_ = await store.open(id)
    const tab = { id, name, document: document_ }
    tab.view = new EditorView({
      doc: document_.source(),
      extensions: [
        indentation,
        setup,
        sameLines,
        theme,
        highlighting,
        ownEditsOnly,
        presence?.extensions ?? [],
        document_.extensions,
        EditorView.updateListener.of((update) => {
          if (tab === tabs[active]) syncHistory()
          tab.here?.update(update)
        }),
      ],
      parent: $("code"),
    })
    tab.here = presence?.attach(document_, tab.view)
    tab.stop = document_.subscribe(draw)

    tab.button = document.createElement("button")
    tab.button.type = "button"
    tab.button.setAttribute("role", "tab")
    tab.button.addEventListener("click", () => show(tabs.indexOf(tab)))
    if (store.rename) {
      tab.button.title = "Double-click to rename"
      tab.button.addEventListener("dblclick", () => {
        const next = prompt("This document's name", tab.name)?.trim()
        if (next && next !== tab.name) store.rename(tab.id, next)
      })
    }
    return tab
  }

  const close = (tab) => {
    tab.stop()
    tab.here?.stop()
    tab.view.destroy()
    tab.button.remove()
  }

  // The store's list of documents, as tabs: new ones opened, renamed ones
  // relabelled, removed ones closed. One at a time, since opening one can
  // wait for it to turn up
  const reconcile = async () => {
    const shown = tabs[active]?.id
    const next = []
    for (const { id, name } of store.list()) {
      const tab = tabs.find((tab) => tab.id === id) ?? (await open(id, name))
      tab.name = name
      tab.button.textContent = name
      next.push(tab)
    }
    for (const tab of tabs) if (!next.includes(tab)) close(tab)
    tabs = next
    $("tabs").replaceChildren(...tabs.map((tab) => tab.button))
    const target = wanted ?? shown
    wanted = null
    show(Math.max(0, tabs.findIndex((tab) => tab.id === target)))
    draw()
  }
  let reconciling = Promise.resolve()
  const refresh = () => (reconciling = reconciling.then(reconcile))
  store.watch(refresh)

  $("add-document").addEventListener("click", async () => {
    const name = prompt("The new document's name (a private one, say)", "private")?.trim()
    if (!name) return
    wanted = await store.create(name)
    refresh()
  })

  // --- Editing ---------------------------------------------------------------

  $("undo").addEventListener("click", () => tabs[active] && undo(tabs[active].view))
  $("redo").addEventListener("click", () => tabs[active] && redo(tabs[active].view))

  // The examples replace this tab's document (for everyone, when shared), as
  // one undoable edit
  for (const [id, example] of [
    ["example-basic", BASIC],
    ["example-tour", TOUR],
  ]) {
    $(id).addEventListener("click", () => {
      const view = tabs[active]?.view
      if (!view) return
      view.dispatch({ changes: splice(view.state.doc.toString(), example), userEvent: "input.example" })
      view.scrollDOM.scrollTop = 0
    })
  }

  page.start()
  await refresh()

  return {
    page,
    /** The tab on show. */
    active: () => tabs[active],
    /** Every tab, in the store's order. */
    tabs: () => [...tabs],
  }
}
