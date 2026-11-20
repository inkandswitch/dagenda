// Edits made on the picture, with real pointer gestures: a status click, a
// drag from a badge to a card, a card dragged into another era, and a gate
// dragged past another. Each lands in the text, as if typed.

import assert from "node:assert/strict"
import { after, before, test } from "node:test"
import { fileURLToPath } from "node:url"
import { browser, ready, serve, sleep, sync } from "./harness.mjs"

const dist = fileURLToPath(new URL("../dist/", import.meta.url))

let site
const browsers = []
const open = async (name) => {
  const b = await browser(name)
  browsers.push(b)
  const tab = await b.tab()
  await tab.send("Emulation.setDeviceMetricsOverride", { width: 1600, height: 1200, deviceScaleFactor: 1, mobile: false })
  await tab.open(`${site.url}${sync}`)
  await tab.until(ready, "the editor to load")
  return tab
}

before(async () => {
  site = await serve(dist)
})

after(async () => {
  await Promise.all(browsers.map((b) => b.close()))
  site?.close()
})

const text = `window.editor.active().view.state.doc.toString()`

/** Replace the plan's text, as typing would, and wait for the picture. */
const write = async (tab, source, id) => {
  await tab.evaluate(`(() => {
    const { view } = window.editor.active()
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: ${JSON.stringify(source)} }, userEvent: "input.type" })
  })()`)
  await tab.until(`document.getElementById("picture").svg.includes('data-id="${id}"')`, `${id} drawn`)
  await sleep(200)
}

/** The middle of something in the picture, in page pixels. */
const middle = (tab, selector) =>
  tab.evaluate(`(() => {
    const element = document.getElementById("picture").shadowRoot.querySelector(${JSON.stringify(selector)})
    const { x, y, width, height } = element.getBoundingClientRect()
    return { x: x + width / 2, y: y + height / 2 }
  })()`)

const mouse = (tab, type, { x, y }, buttons) =>
  tab.send("Input.dispatchMouseEvent", { type, x, y, button: "left", buttons, clickCount: 1 })

const click = async (tab, point) => {
  await mouse(tab, "mousePressed", point, 1)
  await mouse(tab, "mouseReleased", point, 0)
}

const drag = async (tab, from, to) => {
  await mouse(tab, "mousePressed", from, 1)
  for (let step = 1; step <= 8; step++) {
    const along = { x: from.x + ((to.x - from.x) * step) / 8, y: from.y + ((to.y - from.y) * step) / 8 }
    await mouse(tab, "mouseMoved", along, 1)
  }
  await mouse(tab, "mouseReleased", to, 0)
}

test("clicking a badge sets the status from a menu", async () => {
  const a = await open("status")
  await write(a, "roadmap\n[ ] task a [A]\n[ ] task b [B]\n  needs a\n", "b")
  await click(a, await middle(a, '.node[data-id="a"] .badge'))
  await a.until(`document.getElementById("picture").shadowRoot.querySelectorAll(".menu button").length === 8`, "the menu")
  await a.evaluate(`[...document.getElementById("picture").shadowRoot.querySelectorAll(".menu button")].find((b) => b.textContent.includes("Done")).click()`)
  await a.until(`${text}.includes("[x] task a [A]")`, "the box in the text")
  assert.match(await a.evaluate(text), /\[ \] task b \[B\]/, "and nothing else")
})

test("dragging from a badge to a card adds a need", async () => {
  const a = await open("need")
  await write(a, "roadmap\n[ ] task a [A]\n[ ] task b [B]\n[ ] task c [C]\n  needs b\n", "c")
  await drag(a, await middle(a, '.node[data-id="c"] .badge'), await middle(a, '.node[data-id="a"] .card'))
  await a.until(`${text}.includes("needs b, a")`, "the need in the text")

  // A loop is refused, with the reason on the picture
  await drag(a, await middle(a, '.node[data-id="a"] .badge'), await middle(a, '.node[data-id="c"] .card'))
  const toast = await a.until(`document.getElementById("picture").shadowRoot.querySelector(".toast")?.textContent`, "the reason")
  assert.match(toast, /already waits on/)
})

test("dragging a card into another era re-scopes it", async () => {
  const a = await open("rescope")
  await write(
    a,
    "roadmap\n[ ] task a [A]\n[ ] task b [B]\n\nmilestone first [First]\n  a\n\nmilestone second [Second]\n  b\n",
    "second",
  )
  const card = await middle(a, '.node[data-id="b"] .card')
  const line = await a.evaluate(`(() => {
    const line = document.getElementById("picture").shadowRoot.querySelector('.era[data-milestone="first"] line')
    return line.getBoundingClientRect().y
  })()`)
  await drag(a, card, { x: card.x, y: line + 40 })
  await a.until(`${text}.includes("milestone first [First]\\n  a\\n  b\\n")`, "b in First")
  assert.match(await a.evaluate(text), /milestone second \[Second\]\n$/)
})

test("dragging a gate past another reorders the eras", async () => {
  const a = await open("reorder")
  await write(a, "roadmap\n[ ] task a [A]\n[ ] task b [B]\n\nmilestone first [First]\n  a\n\nmilestone second [Second]\n  b\n", "second")
  const gate = await middle(a, '.node[data-id="first"] .gate')
  const above = await middle(a, '.node[data-id="second"] .gate')
  await drag(a, gate, { x: gate.x, y: above.y - 60 })
  await a.until(`${text}.indexOf("milestone second") < ${text}.indexOf("milestone first")`, "First above Second")
})
