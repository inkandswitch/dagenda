// The Wasm bindings, called the way the component and the preview call them.
// Needs pkg/ built first (`nix run .#ci-js` does).

import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { test } from "node:test"
import { check, checkPlan, initSync, locate, locatePlan, renderPlanSvg, renderSvg, renderText } from "./pkg/dagenda_wasm.js"

initSync({ module: readFileSync(new URL("./pkg/dagenda_wasm_bg.wasm", import.meta.url)) })

const PLAN = `roadmap
title Bindings
track core [Core]
  [x] task parser [Parser]
  [ ] task resolver [Resolver]
    needs parser
milestone first [First]
  resolver
`

const plain = (problem) => {
  const copy = {
    document: problem.document,
    line: problem.line,
    column: problem.column,
    severity: problem.severity,
    message: problem.message,
  }
  problem.free()
  return copy
}
const problems = (source) => check(source).map(plain)

const PUBLIC = "roadmap\n[ ] task signatures [Signatures]\n"
const PRIVATE = "roadmap\nextend signatures\n  dri @alice\n  [ ] task wireFormat [Pick the wire format]\n"

test("a good plan has no problems", () => {
  assert.deepEqual(problems(PLAN), [])
})

test("an error comes with its line and column", () => {
  assert.deepEqual(problems("roadmap\n[ ] task a\n  needs b\n"), [
    { document: 0, line: 3, column: 9, severity: "error", message: "nothing in the plan is called `b`" },
  ])
})

test("warnings come back when the plan resolves", () => {
  const [warning] = problems("roadmap\n[~] task a\nmilestone m\n  a\n")
  assert.equal(warning.severity, "warning")
  assert.equal(warning.line, 4)
})

test("renders the tech tree and the text", () => {
  const svg = renderSvg(PLAN)
  assert.match(svg, /^<svg/)
  assert.match(svg, /data-id="resolver"/)
  assert.match(svg, /<g class="edge member" data-from="resolver" data-to="first"\/>/, "hover can follow what the band implies")
  assert.match(renderText(PLAN), /\[ \] resolver · Resolver/)
})

test("rendering a broken plan throws with where it's broken", () => {
  assert.throws(() => renderSvg("roadmap\n[ ] task a\n[ ] task a\n"), {
    message: "3:10: `a` is defined more than once (first at 2:1)",
  })
})

test("locates definitions, even when the plan doesn't resolve", () => {
  assert.equal(locate(PLAN, "resolver"), 5)
  assert.equal(locate(PLAN, "first"), 7)
  assert.equal(locate(PLAN, "nowhere"), undefined)
  assert.equal(locate("roadmap\n[ ] task a\n  needs missing\n", "a"), 2)
})

test("a plan of several documents is one picture", () => {
  assert.deepEqual(checkPlan(["public", "private"], [PUBLIC, PRIVATE]).map(plain), [])
  const svg = renderPlanSvg(["public", "private"], [PUBLIC, PRIVATE])
  assert.match(svg, /data-id="wireFormat"/, "the private task")
  assert.match(svg, /@alice/, "the private DRI, on the public task")
  assert.doesNotMatch(renderSvg(PUBLIC), /@alice/, "and not without the private document")
})

test("problems say which document they're in", () => {
  const [problem] = checkPlan(["public", "private"], [PUBLIC, `${PRIVATE}[ ] task signatures\n`]).map(plain)
  assert.equal(problem.document, 1)
  assert.equal(problem.message, "`signatures` is defined more than once (first at public:2:1)")
  assert.throws(() => renderPlanSvg(["public", "private"], [PUBLIC, "roadmap\n  oops\n"]), {
    message: /^private:2:/,
  })
})

test("locates definitions across documents", () => {
  const place = locatePlan([PUBLIC, PRIVATE], "wireFormat")
  assert.deepEqual({ document: place.document, line: place.line }, { document: 1, line: 4 })
  place.free()
  assert.equal(locatePlan([PUBLIC, PRIVATE], "nowhere"), undefined)
})
