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

test("dragging a milestone's line reorders it, as its gate does", async () => {
  const a = await open("line")
  await write(a, "roadmap\n[ ] task a [A]\n[ ] task b [B]\n\nmilestone first [First]\n  a\n\nmilestone second [Second]\n  b\n", "second")
  const line = await a.evaluate(`(() => {
    const { x, y } = document.getElementById("picture").shadowRoot.querySelector('.era[data-milestone="first"] line.hit').getBoundingClientRect()
    return { x: x + 40, y: y + 7 }
  })()`)
  const above = await middle(a, '.node[data-id="second"] .gate')
  await drag(a, line, { x: line.x, y: above.y - 60 })
  await a.until(`${text}.indexOf("milestone second") < ${text}.indexOf("milestone first")`, "First above Second")
})

test("a refused drag says why, where it was dropped", async () => {
  const a = await open("refused")
  await write(a, "roadmap\n[ ] task a [A]\n\nmilestone first [First]\n  a\n\nmilestone second [Second]\n  first\n", "second")
  const gate = await middle(a, '.node[data-id="first"] .gate')
  const above = await middle(a, '.node[data-id="second"] .gate')
  const drop = { x: gate.x, y: above.y - 60 }
  await drag(a, gate, drop)
  const toast = await a.until(
    `(() => {
      const toast = document.getElementById("picture").shadowRoot.querySelector(".toast")
      if (!toast) return null
      const { x, y, right } = toast.getBoundingClientRect()
      return { text: toast.textContent, x, y, right, width: document.documentElement.clientWidth }
    })()`,
    "the reason",
  )
  assert.match(toast.text, /`second` builds on `first`/)
  assert.ok(Math.abs(toast.y - drop.y) < 60, `near the drop: ${JSON.stringify({ toast, drop })}`)
  assert.ok(toast.x >= 0 && toast.right <= toast.width, `inside the window: ${JSON.stringify(toast)}`)
  assert.match(await a.evaluate(text), /milestone first \[First\]\n {2}a\n\nmilestone second/, "and the text is as it was")
})

/** A point on milestone `id`'s line, near its left end. */
const onLine = (tab, id) =>
  tab.evaluate(`(() => {
    const { x, y } = document.getElementById("picture").shadowRoot.querySelector('.era[data-milestone="${id}"] line.hit').getBoundingClientRect()
    return { x: x + 40, y: y + 7 }
  })()`)

test("dragging a milestone's line up past a card brings it in", async () => {
  const a = await open("bring")
  await write(a, "roadmap\n[ ] task a [A]\n[ ] task c [C]\n\nmilestone first [First]\n  a\n", "first")
  const line = await onLine(a, "first")
  const card = await middle(a, '.node[data-id="c"] .card')
  await drag(a, line, { x: line.x, y: card.y - 30 })
  await a.until(`${text}.includes("milestone first [First]\\n  a\\n  c\\n")`, "c in First")
})

test("dragging a milestone's line down past a card hands it to the era above", async () => {
  const a = await open("hand")
  await write(
    a,
    "roadmap\n[ ] task a [A]\n[ ] task b [B]\n  needs a\n[ ] task c [C]\n\nmilestone first [First]\n  a\n  b\n\nmilestone second [Second]\n  c\n",
    "second",
  )
  const line = await onLine(a, "first")
  const b = await middle(a, '.node[data-id="b"] .card')
  const below = await middle(a, '.node[data-id="a"] .card')
  await drag(a, line, { x: line.x, y: (b.y + below.y) / 2 })
  await a.until(`${text}.includes("milestone second [Second]\\n  c\\n  b\\n")`, "b in Second")
  assert.match(await a.evaluate(text), /milestone first \[First\]\n {2}a\n\n/, "and off First")
})

test("a milestone's grab area is never drawn", async () => {
  const a = await open("thin")
  await write(a, "roadmap\n[x] task a [A]\n\nmilestone first [First]\n  a\n", "first")
  await a.evaluate(`document.getElementById("picture").setAttribute("theme", "dark")`)
  const hit = await a.evaluate(`(() => {
    const hit = document.getElementById("picture").shadowRoot.querySelector('.era[data-milestone="first"] line.hit')
    return getComputedStyle(hit).strokeOpacity
  })()`)
  assert.equal(hit, "0", "a reached era, in the dark")
})

/** A plan big enough to scroll when zoomed in: four tracks of chained tasks. */
const BIG = [
  "roadmap",
  ...["a", "b", "c", "d"].flatMap((track) => [
    `track ${track}`,
    ...[0, 1, 2, 3, 4, 5].flatMap((n) => [`  [ ] task ${track}${n}`, ...(n > 0 ? [`    needs ${track}${n - 1}`] : [])]),
  ]),
].join("\n")

test("zooming with the wheel keeps the card under the pointer still", async () => {
  const a = await open("zoom")
  await write(a, BIG, "d5")
  // Big enough to scroll, which is what lets a point stay put: a picture
  // smaller than its pane just grows from its corner
  for (let step = 0; step < 3; step++) await a.evaluate(`document.getElementById("zoom-in").click()`)
  await sleep(100)
  const card = '.node[data-id="c3"] .card'
  const point = await middle(a, card)
  await a.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: point.x, y: point.y })
  for (let step = 0; step < 6; step++) {
    // Ctrl + wheel, as a pinch on a trackpad sends
    await a.send("Input.dispatchMouseEvent", { type: "mouseWheel", x: point.x, y: point.y, deltaX: 0, deltaY: -120, modifiers: 2 })
  }
  await a.until(`Number(document.getElementById("picture").getAttribute("zoom")) > 1.5`, "zoomed in")
  await sleep(100)
  const after = await middle(a, card)
  assert.ok(Math.abs(after.x - point.x) < 3 && Math.abs(after.y - point.y) < 3, `c3 stayed under the pointer: ${JSON.stringify({ point, after })}`)
})

test("zooming with the buttons keeps the middle of the view still", async () => {
  const a = await open("zoom-buttons")
  await write(a, BIG, "d5")
  // The middle of the pane that scrolls the picture, as fractions of the
  // picture, before and after
  const centre = `(() => {
    const pane = document.getElementById("views").getBoundingClientRect()
    const svg = document.getElementById("picture").shadowRoot.querySelector("svg").getBoundingClientRect()
    const [x, y] = [pane.x + pane.width / 2, pane.y + Math.min(pane.height, innerHeight - pane.y) / 2]
    return { across: (x - svg.x) / svg.width, down: (y - svg.y) / svg.height }
  })()`
  await a.evaluate(`document.getElementById("zoom-in").click()`)
  await a.evaluate(`document.getElementById("zoom-in").click()`)
  await sleep(100)
  const before = await a.evaluate(centre)
  await a.evaluate(`document.getElementById("zoom-in").click()`)
  await sleep(100)
  const after = await a.evaluate(centre)
  assert.ok(
    Math.abs(after.across - before.across) < 0.01 && Math.abs(after.down - before.down) < 0.01,
    `the same point stays in the middle: ${JSON.stringify({ before, after })}`,
  )
})
