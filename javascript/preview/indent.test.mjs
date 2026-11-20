import assert from "node:assert/strict"
import { test } from "node:test"
import { indentLines } from "./indent.js"

/** Apply `indentLines` to `marked`, whose selection is written `«` … `»` (or `|` for a caret). */
const apply = (marked, outdent = false) => {
  const caret = marked.indexOf("|")
  const start = caret >= 0 ? caret : marked.indexOf("«")
  const end = caret >= 0 ? caret : marked.indexOf("»") - 1
  const value = marked.replace(/[«»|]/g, "")
  const { from, to, text, start: s, end: e } = indentLines(value, start, end, outdent)
  const next = value.slice(0, from) + text + value.slice(to)
  return s === e ? `${next.slice(0, s)}|${next.slice(s)}` : `${next.slice(0, s)}«${next.slice(s, e)}»${next.slice(e)}`
}

test("Tab over a selection indents every line it touches, and keeps them selected", () => {
  assert.equal(apply("roadmap\ntrack t\n  «[ ] task a\n  [ ] task b»\nend"), "roadmap\ntrack t\n«    [ ] task a\n    [ ] task b»\nend")
})

test("Shift+Tab takes up to two spaces off each line", () => {
  assert.equal(apply("«a\n b\n    c»", true), "«a\nb\n  c»")
})

test("a selection ending at a line's start doesn't touch that line", () => {
  assert.equal(apply("«a\nb\n»c"), "«  a\n  b»\nc")
})

test("blank lines stay blank, so nothing ends in spaces", () => {
  assert.equal(apply("«a\n\nb»"), "«  a\n\n  b»")
})

test("Shift+Tab with a caret outdents its line, and the caret moves with the text", () => {
  assert.equal(apply("x\n    ab|c\ny", true), "x\n  ab|c\ny")
  assert.equal(apply("x\n | abc", true), "x\n|abc", "never before the line's start")
})

test("a selection within one line moves with its text", () => {
  assert.equal(apply("  task «abc»"), "    task «abc»")
  assert.equal(apply("  «task» abc", true), "«task» abc")
})

test("outdenting an unindented line changes nothing", () => {
  assert.equal(apply("«a\nb»", true), "«a\nb»")
})
