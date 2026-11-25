// The Tour is the golden test's tour, so it always parses and
// shows every form.

import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { test } from "node:test"
import { TOUR } from "./examples.js"

test("the Tour button loads the golden tour", () => {
  const golden = new URL("../../../rust/dagenda_core/tests/golden/tour.dagenda", import.meta.url)
  assert.equal(TOUR, readFileSync(golden, "utf8"))
})
