// The collaborative editor on a workspace: an Automerge document listing a
// plan's documents, `{ documents: [{ name, url }] }`, each its own Automerge
// document with a `source` string. A tab per document, its text in
// CodeMirror; one picture drawn from all of them, through the preview's page
// shell (../../preview/shell.js). Until Keyhive, a private document is one
// whose URL isn't linked from anything public.

import { automergeSyncPlugin } from "@automerge/automerge-codemirror"
import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete"
import { defaultKeymap, history, historyKeymap, redo, redoDepth, undo, undoDepth } from "@codemirror/commands"
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
import { BASIC, TOUR } from "../../preview/examples.js"
import { shell } from "../../preview/shell.js"
import { highlighting, paint } from "./highlighting.js"
import { carets, identity, remoteCarets, roster } from "./presence.js"

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

// Indentation is structure: Enter keeps the line's indent, and Tab indents
// with two spaces, as in the preview
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
      view.dispatch(view.state.replaceSelection("  "), { userEvent: "input" })
      return true
    },
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

const sourceOf = (handle) => handle.doc()?.source ?? ""

/**
 * Mount the editor on `workspace`, in the page's ids (index.html).
 *
 * @param {object} options
 * @param {import("@automerge/automerge-repo").Repo} options.repo
 * @param {import("@automerge/automerge-repo").DocHandle} options.workspace
 * @param {(url: string) => Promise<import("@automerge/automerge-repo").DocHandle>} options.find
 *   a document by URL, waiting for it to turn up
 * @param {(handles: import("@automerge/automerge-repo").DocHandle[]) => void} [options.onHandles]
 *   the plan documents' handles, whenever the list changes
 * @returns {Promise<{ active: () => { name: string, view: EditorView, handle: object } | undefined, tabs: () => object[] }>}
 */
export const mountWorkspace = async ({ repo, workspace, find, onHandles = () => {} }) => {
  const $ = (id) => document.getElementById(id)
  const picture = $("picture")
  await init()

  /** @type {{ name: string, url: string, handle: object, view: EditorView, here: object }[]} */
  let tabs = []
  let active = 0
  let wanted = null // a tab's URL to show once it's open

  const documents = () => tabs.map((tab) => ({ name: tab.name, source: sourceOf(tab.handle) }))
  const page = shell({
    prefix: "dagenda-editor",
    documents,
    goTo: (document, line) => goTo(document, line),
    onRender: (problems) => {
      // Each tab marks its own error (deferred: a render can follow a
      // change made inside CodeMirror's own update)
      queueMicrotask(() =>
        tabs.forEach((tab, index) => {
          const error = problems.find((problem) => problem.severity === "error" && problem.document === index)
          tab.view.dispatch({ effects: paint.of(error?.line ?? null) })
          tab.button?.classList.toggle("has-error", Boolean(error))
        }),
      )
    },
    wasm: { renderPlanText },
  })

  const user = identity(page)
  const here = roster({
    handle: workspace,
    user,
    list: $("people"),
    save: page.save,
    onRename: () => tabs.forEach((tab) => tab.here.announce()),
  })

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
    const place = locatePlan(
      tabs.map((tab) => ({ source: sourceOf(tab.handle) })),
      id,
    )
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
    here.on(tabs[active]?.name ?? "")
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

  /** A tab for the document at `url`, called `name`. */
  const open = async (name, url) => {
    const handle = await find(url)
    const tab = { name, url, handle }
    tab.view = new EditorView({
      doc: sourceOf(handle),
      extensions: [
        indentation,
        setup,
        sameLines,
        theme,
        highlighting,
        ownEditsOnly,
        remoteCarets,
        automergeSyncPlugin({ handle, path: ["source"] }),
        EditorView.updateListener.of((update) => {
          if (tab === tabs[active]) syncHistory()
          tab.here?.update(update)
        }),
      ],
      parent: $("code"),
    })
    tab.here = carets({ handle, view: tab.view, user })
    handle.on("change", draw)

    tab.button = document.createElement("button")
    tab.button.type = "button"
    tab.button.setAttribute("role", "tab")
    tab.button.title = "Double-click to rename"
    tab.button.addEventListener("click", () => show(tabs.indexOf(tab)))
    tab.button.addEventListener("dblclick", () => {
      const next = prompt("This document's name", tab.name)?.trim()
      if (!next || next === tab.name) return
      workspace.change((doc) => {
        const entry = doc.documents.find((entry) => entry.url === tab.url)
        if (entry) entry.name = next
      })
    })
    return tab
  }

  const close = (tab) => {
    tab.handle.off("change", draw)
    tab.here.stop()
    tab.view.destroy()
    tab.button.remove()
  }

  // The workspace's list of documents, as tabs: new ones opened, renamed
  // ones relabelled, removed ones closed. One at a time, since opening one
  // can wait for it to turn up
  const reconcile = async () => {
    const listed = workspace.doc()?.documents ?? []
    const shown = tabs[active]?.url
    const next = []
    for (const { name, url } of listed) {
      const tab = tabs.find((tab) => tab.url === url) ?? (await open(name, url))
      tab.name = name
      tab.button.textContent = name
      next.push(tab)
    }
    for (const tab of tabs) if (!next.includes(tab)) close(tab)
    tabs = next
    $("tabs").replaceChildren(...tabs.map((tab) => tab.button))
    const target = wanted ?? shown
    wanted = null
    show(Math.max(0, tabs.findIndex((tab) => tab.url === target)))
    onHandles(tabs.map((tab) => tab.handle))
    draw()
  }
  let reconciling = Promise.resolve()
  const refresh = () => (reconciling = reconciling.then(reconcile))
  workspace.on("change", refresh)

  $("add-document").addEventListener("click", async () => {
    const name = prompt("The new document's name (a private one, say)", "private")?.trim()
    if (!name) return
    const handle = await repo.create({ source: "roadmap\n" })
    wanted = handle.url
    workspace.change((doc) => {
      doc.documents.push({ name, url: handle.url })
    })
  })

  // --- Editing ---------------------------------------------------------------

  $("undo").addEventListener("click", () => tabs[active] && undo(tabs[active].view))
  $("redo").addEventListener("click", () => tabs[active] && redo(tabs[active].view))

  // The examples replace this tab's document for everyone, as one undoable
  // edit
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
    /** The tab on show. */
    active: () => tabs[active],
    /** Every tab, in the workspace's order. */
    tabs: () => [...tabs],
  }
}
