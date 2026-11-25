// The preview: the collaborative editor's page on one document kept in this
// browser, with no sync. _Collaborate_ hands its text to the collaborative
// editor (../collab/), which makes a shared plan from it.

import { EditorView } from "@codemirror/view"
import "./style.css"
import { mountEditor } from "./editor.js"
import { BASIC } from "./examples.js"

const collaborate = document.getElementById("collaborate")

/** One document, called "plan", saved in this browser as it changes. */
const store = (page) => ({
  list: () => [{ id: "plan", name: "plan" }],
  watch: () => {},
  open: async () => {
    let source = page.load("source", BASIC)
    const listeners = new Set()
    collaborate.hidden = false
    const share = () => (collaborate.href = `../collab/#source=${encodeURIComponent(source)}`)
    share()
    return {
      source: () => source,
      extensions: EditorView.updateListener.of((update) => {
        if (!update.docChanged) return
        source = update.state.doc.toString()
        page.save("source", source)
        share()
        listeners.forEach((onChange) => onChange())
      }),
      subscribe: (onChange) => {
        listeners.add(onChange)
        return () => listeners.delete(onChange)
      },
    }
  },
})

// For poking at in the console, and for the browser tests
window.editor = await mountEditor({ prefix: "dagenda-preview", store })
