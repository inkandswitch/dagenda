// The preview's syntax colouring: line for line, escaped, and aware of notes.

import assert from "node:assert/strict"
import { test } from "node:test"
import { highlight } from "./highlight.js"

const lines = (html) => html.replace(/\n$/, "").split("\n")

test("keeps one line of HTML per line of source", () => {
  const source = "roadmap\n\ntrack core [Core]\n  [x] task parser\n"
  assert.equal(lines(highlight(source)).length, source.split("\n").length)
})

test("colours the parts of a task line", () => {
  const [, line] = lines(highlight("roadmap\n  [x] task parser [Parser]"))
  assert.match(line, /^ {2}<span class="box done">\[x\]<\/span> /)
  assert.match(line, /<span class="keyword">task<\/span> <span class="id">parser<\/span> <span class="label">\[Parser\]<\/span>$/)
})

test("colours details by kind", () => {
  const html = highlight("needs a, b\ndri @alice\npr gh:owner/repo#1\nest 2w")
  assert.match(html, /<span class="id">a<\/span>, <span class="id">b<\/span>/)
  assert.match(html, /<span class="person">@alice<\/span>/)
  assert.match(html, /<span class="ref">gh:owner\/repo#1<\/span>/)
  assert.match(html, /<span class="value">2w<\/span>/)
})

test("escapes what HTML would swallow", () => {
  const html = highlight("[ ] task a [Fish & chips <3>]")
  assert.match(html, /Fish &amp; chips &lt;3&gt;/)
  assert.doesNotMatch(html, /<3/)
})

test("a note block is markdown, comments and all, until the indent comes back", () => {
  const html = lines(highlight("  note\n    // not a comment\n\n    more\n  needs a"))
  assert.match(html[1], /<span class="markdown">\/\/ not a comment<\/span>/)
  assert.match(html[3], /<span class="markdown">more<\/span>/)
  assert.match(html[4], /<span class="detail">needs<\/span>/)
})

test("comment lines and cut lines", () => {
  const [comment, cut] = lines(highlight("// hello\n---"))
  assert.equal(comment, '<span class="comment">// hello</span>')
  assert.equal(cut, '<span class="cut">---</span>')
})

test("marks the error line", () => {
  const html = lines(highlight("roadmap\ntask x", 2))
  assert.match(html[1], /^<span class="error-line">.*<\/span>$/)
  assert.doesNotMatch(html[0], /error-line/)
})

test("flags an unknown box", () => {
  assert.match(highlight("[q] task x"), /<span class="box unknown">\[q\]<\/span>/)
})
