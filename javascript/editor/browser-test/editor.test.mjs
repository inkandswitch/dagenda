// The editor in headless Chromium: a workspace of documents drawn as one
// plan, problems and cards that lead to the right tab, two people editing
// together, and whether everything has reached the sync server. Needs the
// built site (`vite build`) and a sync server: the public one, unless
// DAGENDA_SYNC names another.

import assert from "node:assert/strict"
import { after, before, test } from "node:test"
import { fileURLToPath } from "node:url"
import { browser, ready, serve, sync } from "./harness.mjs"

const dist = fileURLToPath(new URL("../dist/", import.meta.url))
// Nothing listens here, so the page can't reach a server
const NOWHERE = "?sync=ws://127.0.0.1:9"

let site
const browsers = []
const open = async (name, hash = "") => {
  const b = await browser(name)
  browsers.push(b)
  const tab = await b.tab()
  await tab.open(`${site.url}${sync}${hash}`)
  return tab
}

before(async () => {
  site = await serve(dist)
})

after(async () => {
  await Promise.all(browsers.map((b) => b.close()))
  site?.close()
})

const add = (tab, name, source) =>
  tab.evaluate(`(async () => {
    const handle = await window.repo.create({ source: ${JSON.stringify(source)} })
    window.workspace.change((doc) => { doc.documents.push({ name: ${JSON.stringify(name)}, url: handle.url }) })
    return handle.url
  })()`)

/** Replace document `index`'s text, as typing would. */
const write = (tab, index, source) =>
  tab.evaluate(`(() => {
    const { view } = window.editor.tabs()[${index}]
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: ${JSON.stringify(source)} }, userEvent: "input.type" })
  })()`)

const tabNames = `window.editor.tabs().map((tab) => tab.name).join(",")`
const svg = `document.getElementById("picture").svg`
const status = `document.getElementById("status").textContent`

test("a new workspace has one document, drawn", async () => {
  const a = await open("new")
  await a.until(ready, "the editor to load")
  assert.equal(await a.evaluate(tabNames), "plan")
  assert.match(await a.evaluate(svg), /data-id="parser"/)
  assert.match(await a.evaluate("location.hash"), /^#automerge:/)
})

test("the picture draws from every document, and problems say which", async () => {
  const a = await open("documents")
  await a.until(ready, "the editor to load")
  await add(a, "private", "roadmap\nextend parser\n  dri @alice\n[ ] task secret [Secret]\n  needs parser\n")
  await a.until(`${tabNames} === "plan,private"`, "the second tab")
  await a.until(`${svg}.includes("@alice") && ${svg}.includes('data-id="secret"')`, "the private document in the picture")
  // Only the tab on show has its text on screen
  const shown = `[...document.querySelectorAll("#code .cm-editor")].filter((editor) => editor.getClientRects().length).length`
  assert.equal(await a.evaluate(shown), 1)

  await write(a, 1, "roadmap\n  oops\n")
  const error = await a.until(`${status}.startsWith("✗") && ${status}`, "the error")
  assert.match(error, /^✗ private:2:/)
  await a.until(`window.editor.tabs()[1].button.classList.contains("has-error")`, "its tab marked")

  // The picture keeps its last good state meanwhile
  assert.match(await a.evaluate(svg), /data-id="secret"/)
})

test("selecting a card shows its line, in the document that defines it", async () => {
  const a = await open("select")
  await a.until(ready, "the editor to load")
  await add(a, "private", "roadmap\n[ ] task secret [Secret]\n")
  await a.until(`${svg}.includes('data-id="secret"')`, "the private task drawn")
  await a.evaluate(`document.getElementById("picture").select("secret")`)
  await a.until(`window.editor.active().name === "private"`, "its tab shown")
  const line = await a.evaluate(`(() => {
    const { state } = window.editor.active().view
    return state.doc.lineAt(state.selection.main.head).text
  })()`)
  assert.match(line, /task secret/)
})

test("two people edit together, and see each other", async () => {
  const a = await open("A")
  await a.until(ready, "A to load")
  const url = await a.evaluate("window.workspace.url")
  const b = await open("B", `#${url}`)
  await b.until(ready, "B to load A's workspace")

  await a.evaluate(`window.editor.active().view.dispatch({ changes: { from: 0, insert: "// from A\\n" }, userEvent: "input.type" })`)
  await b.until(`window.editor.active().view.state.doc.toString().startsWith("// from A")`, "A's edit to reach B")

  await add(b, "notes", "roadmap\n[ ] task jot [A note]\n")
  await a.until(`${tabNames} === "plan,notes"`, "B's new document to reach A")
  await a.until(`${svg}.includes('data-id="jot"')`, "and its task in A's picture")

  await a.until(`document.getElementById("people").children.length === 2`, "A to see B")
  await b.until(`document.getElementById("people").children.length === 2`, "B to see A")
})

test("an edit syncs; with no server, the page says it's kept in this browser", async () => {
  const a = await open("synced")
  await a.until(ready, "the editor to load")
  await a.until(`document.getElementById("sync").className === "synced"`, "synced", 20_000)

  const b = await browser("offline")
  browsers.push(b)
  const c = await b.tab()
  await c.open(`${site.url}${NOWHERE}`)
  await c.until("!!window.editor?.active()", "the editor to load offline", 20_000)
  await c.evaluate(`window.editor.active().view.dispatch({ changes: { from: 0, insert: "// offline\\n" }, userEvent: "input.type" })`)
  const title = await c.until(
    `document.getElementById("sync").className === "offline" && document.getElementById("sync").title`,
    "not synced",
    20_000,
  )
  assert.match(title, /kept in this browser/)
})
