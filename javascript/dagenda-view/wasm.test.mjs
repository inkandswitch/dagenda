// The Wasm bindings, called the way the component and the preview call them.
// Needs pkg/ built first (`nix run .#ci-js` does).

import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { test } from "node:test"
import {
  addNeed,
  check,
  checkPlan,
  initSync,
  locate,
  locatePlan,
  renderPlanSvg,
  renderSvg,
  renderText,
  reorder,
  rescope,
  setStatus,
} from "./pkg/dagenda_wasm.js"

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

const problems = (source) => check(source)

const PUBLIC = "roadmap\n[ ] task signatures [Signatures]\n"
const PRIVATE = "roadmap\nextend signatures\n  dri @alice\n  [ ] task wireFormat [Pick the wire format]\n"
const BOTH = [
  { name: "public", source: PUBLIC },
  { name: "private", source: PRIVATE },
]
const one = (source) => [{ source }]

/** What `f` throws, or a failure if it doesn't. */
const thrown = (f) => {
  try {
    f()
  } catch (error) {
    return error
  }
  assert.fail("didn't throw")
}

test("a good plan has no problems", () => {
  assert.deepEqual(problems(PLAN), [])
})

test("an error comes with its line and column", () => {
  assert.deepEqual(problems("roadmap\n[ ] task a\n  needs b\n"), [
    { document: 0, line: 3, column: 9, severity: "error", message: "nothing in the plan is called `b`" },
  ])
})

test("every error comes back at once", () => {
  assert.deepEqual(
    problems("roadmap\n[ ] task a\n  needs x\n[ ] task b\n  needs y\n").map((p) => `${p.line}:${p.column} ${p.message}`),
    ["3:9 nothing in the plan is called `x`", "5:9 nothing in the plan is called `y`"],
  )
})

test("results are plain data: they serialize, spread, and need no freeing", () => {
  const [problem] = problems("roadmapp")
  assert.equal(
    JSON.stringify(problem),
    '{"document":0,"line":1,"column":1,"severity":"error","message":"expected `roadmap` on the first line, not \\"roadmapp\\""}',
  )
  assert.deepEqual(Object.keys({ ...problem }), ["document", "line", "column", "severity", "message"])
  assert.equal(problem.free, undefined)
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

test("rendering a broken plan throws every problem, and says where the first is", () => {
  const error = thrown(() => renderSvg("roadmap\n[ ] task a\n[ ] task a\n  needs ghost\n"))
  assert.ok(error instanceof Error)
  assert.equal(error.message, "3:10: `a` is defined more than once (first at 2:1) (and 1 more problem)")
  assert.equal(error.kind, "problems")
  assert.deepEqual(
    error.problems.map((problem) => problem.message),
    ["`a` is defined more than once (first at 2:1)", "nothing in the plan is called `ghost`"],
  )
})

test("locates definitions, even when the plan doesn't resolve", () => {
  assert.equal(locate(PLAN, "resolver"), 5)
  assert.equal(locate(PLAN, "first"), 7)
  assert.equal(locate(PLAN, "nowhere"), undefined)
  assert.equal(locate("roadmap\n[ ] task a\n  needs missing\n", "a"), 2)
})

test("a plan of several documents is one picture", () => {
  assert.deepEqual(checkPlan(BOTH), [])
  const svg = renderPlanSvg(BOTH)
  assert.match(svg, /data-id="wireFormat"/, "the private task")
  assert.match(svg, /@alice/, "the private DRI, on the public task")
  assert.doesNotMatch(renderSvg(PUBLIC), /@alice/, "and not without the private document")
})

test("problems say which document they're in", () => {
  const [problem] = checkPlan([BOTH[0], { name: "private", source: `${PRIVATE}[ ] task signatures\n` }])
  assert.equal(problem.document, 1)
  assert.equal(problem.message, "`signatures` is defined more than once (first at public:2:1)")
  assert.match(thrown(() => renderPlanSvg([BOTH[0], { name: "private", source: "roadmap\n  oops\n" }])).message, /^private:2:/)
})

test("documents must be `[{ name?, source }]`", () => {
  for (const bad of [["roadmap"], [{ name: "x" }], "roadmap", null, [{ source: 3 }]]) {
    const error = thrown(() => checkPlan(bad))
    assert.equal(error.kind, "documents", JSON.stringify(bad))
    assert.match(error.message, /^expected documents as `\[\{ name, source \}\]`/)
  }
  assert.deepEqual(checkPlan([{ source: PLAN }]), [], "a name is optional")
  assert.deepEqual(checkPlan([{ source: PLAN, extra: true }]), [], "other fields are ignored, so `view.documents` passes as is")
})

test("locates definitions across documents", () => {
  assert.deepEqual(locatePlan(BOTH, "wireFormat"), { document: 1, line: 4 })
  assert.equal(locatePlan(BOTH, "nowhere"), undefined)
})

test("a status click rewrites the box, in the document that defines the task", () => {
  assert.deepEqual(setStatus(BOTH, "signatures", "x"), [{ document: 0, source: "roadmap\n[x] task signatures [Signatures]\n" }])
  const error = thrown(() => setStatus(one(PUBLIC), "signatures", "q"))
  assert.equal(error.message, "`[q]` isn't a status box")
  assert.deepEqual({ ...error }, { kind: "status", mark: "q" })
})

test("connecting two cards writes a need, and refuses a loop, saying which", () => {
  const [{ source }] = addNeed(one("roadmap\n[ ] task a\n[ ] task b\n"), "b", "a")
  assert.equal(source, "roadmap\n[ ] task a\n[ ] task b\n  needs a\n")
  const loop = thrown(() => addNeed(one(source), "a", "b"))
  assert.equal(loop.message, "`b` already waits on `a`, so `a` can't need it")
  assert.deepEqual({ ...loop }, { kind: "loop", dependent: "a", prerequisite: "b" })
  assert.deepEqual({ ...thrown(() => addNeed(one(source), "b", "a")) }, { kind: "alreadyNeeds", dependent: "b", prerequisite: "a" })
  assert.deepEqual({ ...thrown(() => addNeed(one(source), "b", "not an id")) }, { kind: "id", id: "not an id" })
})

test("re-scoping and reordering milestones", () => {
  const plan = "roadmap\n[ ] task a\n\nmilestone one\n  a\n\nmilestone two\n"
  const [{ source }] = rescope(one(plan), "a", "one", "two")
  assert.equal(source, "roadmap\n[ ] task a\n\nmilestone one\n\nmilestone two\n  a\n")
  assert.deepEqual(rescope(one(plan), "a", undefined, "one"), [], "already there")
  assert.deepEqual(rescope(one(plan), "a", null, "one"), [], "null is no milestone too")
  const [{ source: reordered }] = reorder(one(plan), "one", "two", true)
  assert.equal(reordered, "roadmap\n[ ] task a\n\nmilestone two\n\nmilestone one\n  a\n")
  assert.deepEqual({ ...thrown(() => reorder(one(plan), "a", "two", true)) }, { kind: "notAMilestone", id: "a" })
})
