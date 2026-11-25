// The preview in headless Chromium: the editor on one document kept in the
// browser, with no sync. Needs the built site (`vite build`).

import assert from "node:assert/strict"
import { after, before, test } from "node:test"
import { fileURLToPath } from "node:url"
import { browser, serve, sleep } from "./harness.mjs"

const dist = fileURLToPath(new URL("../dist/", import.meta.url))
const drawn = `document.getElementById("status")?.textContent.startsWith("✓") && Boolean(window.editor?.active())`
const text = `window.editor.active().view.state.doc.toString()`
const svg = `document.getElementById("picture").svg`

let site
const browsers = []
const open = async (name) => {
  const b = await browser(name)
  browsers.push(b)
  const tab = await b.tab()
  await tab.send("Emulation.setDeviceMetricsOverride", { width: 1600, height: 1200, deviceScaleFactor: 1, mobile: false })
  await tab.open(`${site.url}preview/`)
  await tab.until(drawn, "the preview to load")
  return { b, tab }
}

before(async () => {
  site = await serve(dist)
})

after(async () => {
  await Promise.all(browsers.map((b) => b.close()))
  site?.close()
})

test("one document, no sync, and Collaborate carries its text", async () => {
  const { tab } = await open("preview")
  assert.equal(await tab.evaluate(`window.editor.tabs().map((tab) => tab.name).join(",")`), "plan")
  assert.equal(await tab.evaluate(`document.getElementById("add-document").hidden`), true, "no + to add a document")
  assert.match(await tab.evaluate(svg), /data-id="parser"/, "the small plan, drawn")
  const href = await tab.evaluate(`document.getElementById("collaborate").href`)
  assert.equal(decodeURIComponent(new URL(href).hash.replace(/^#source=/, "")), await tab.evaluate(text))
  assert.equal(new URL(href).pathname, "/collab/")
})

test("the text is kept in the browser, and a typed edit draws", async () => {
  const { b, tab } = await open("kept")
  await tab.evaluate(`window.editor.active().view.dispatch({ changes: { from: 0, to: window.editor.active().view.state.doc.length, insert: "roadmap\\n[ ] kept [Kept]\\n" }, userEvent: "input.type" })`)
  await tab.until(`${svg}.includes('data-id="kept"')`, "the edit drawn")
  await sleep(100)

  const again = await b.tab()
  await again.open(`${site.url}preview/`)
  await again.until(drawn, "the preview to load again")
  assert.equal(await again.evaluate(text), "roadmap\n[ ] kept [Kept]\n")
})

test("clicking a card selects its line in the text", async () => {
  const { tab } = await open("select")
  const point = await tab.evaluate(`(() => {
    const card = document.getElementById("picture").shadowRoot.querySelector('.node[data-id="parser"] .card')
    const { x, y, width, height } = card.getBoundingClientRect()
    return { x: x + width / 2, y: y + height / 2 }
  })()`)
  await tab.send("Input.dispatchMouseEvent", { type: "mousePressed", ...point, button: "left", buttons: 1, clickCount: 1 })
  await tab.send("Input.dispatchMouseEvent", { type: "mouseReleased", ...point, button: "left", buttons: 0, clickCount: 1 })
  const line = await tab.until(`(() => {
    const { state } = window.editor.active().view
    const { from, to } = state.selection.main
    return from !== to && state.sliceDoc(from, to)
  })()`, "the line selected")
  assert.match(line, /\bparser\b/)
})
