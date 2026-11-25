// The syntax colouring: line for line, by kind, and aware of notes.

import assert from "node:assert/strict"
import { test } from "node:test"
import { tokens } from "./highlight.js"

/** Each line's tokens as `<kind>text</>`, plain text as is: easier to read than arrays. */
const marked = (source) => tokens(source).map((line) => line.map(({ kind, text }) => (kind ? `<${kind}>${text}</>` : text)).join(""))

test("tokens rebuild each line, and there's one array per line", () => {
  const source = "roadmap\n\n  [x] parser [Parser]\n    needs a, b\n  note\n    // markdown\n"
  const lines = tokens(source)
  assert.equal(lines.length, source.split("\n").length)
  assert.deepEqual(
    lines.map((line) => line.map((part) => part.text).join("")),
    source.split("\n"),
  )
})

test("colours the parts of a task line", () => {
  assert.equal(marked("  [x] parser [Parser]")[0], "  <box done>[x]</> <id>parser</> <label>[Parser]</>")
})

test("colours details by kind", () => {
  assert.deepEqual(marked("needs a, b\ndri @alice\npr gh:owner/repo#1\nest 2026-11-19\nlink https://example.com [Spec]"), [
    "<detail>needs</> <id>a</>, <id>b</>",
    "<detail>dri</> <person>@alice</>",
    "<detail>pr</> <ref>gh:owner/repo#1</>",
    "<detail>est</> <value>2026-11-19</>",
    "<detail>link</> <ref>https://example.com</> <label>[Spec]</>",
  ])
})

test("colours a person, their avatar, and a list of DRIs", () => {
  assert.deepEqual(marked("@alice\n  avatar gh:octocat\n  dri @alice, @bob"), [
    "<person>@alice</>",
    "  <detail>avatar</> <ref>gh:octocat</>",
    "  <detail>dri</> <person>@alice</>, <person>@bob</>",
  ])
})

test("a note block is markdown, comments and all, until the indent comes back", () => {
  const lines = marked("  note\n    // not a comment\n\n    more\n  needs a")
  assert.equal(lines[1], "    <markdown>// not a comment</>")
  assert.equal(lines[3], "    <markdown>more</>")
  assert.equal(lines[4], "  <detail>needs</> <id>a</>")
})

test("comment lines, and a milestone's nice-to-haves", () => {
  assert.deepEqual(marked("// hello\n  nice-to-have\n    cycles"), ["<comment>// hello</>", "  <keyword>nice-to-have</>", "    <id>cycles</>"])
})

test("flags an unknown box", () => {
  assert.match(marked("[q] x")[0], /^<box unknown>\[q\]<\/>/)
})
