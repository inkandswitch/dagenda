// The syntax colouring (highlight.js) as CodeMirror decorations, with the
// line an error points at marked.

import { RangeSetBuilder, StateEffect, StateField } from "@codemirror/state"
import { Decoration, EditorView } from "@codemirror/view"
import { tokens } from "./highlight.js"

/** The line an error in this document points at (1-based), or null. */
export const paint = StateEffect.define()

const ERROR_LINE = Decoration.line({ class: "error-line" })
const marks = new Map()
const mark = (kind) => {
  if (!marks.has(kind)) marks.set(kind, Decoration.mark({ class: kind }))
  return marks.get(kind)
}

const decorate = (doc, errorLine) => {
  const builder = new RangeSetBuilder()
  tokens(doc.toString()).forEach((line, index) => {
    if (index >= doc.lines) return
    let at = doc.line(index + 1).from
    if (index + 1 === errorLine) builder.add(at, at, ERROR_LINE)
    for (const { kind, text } of line) {
      if (kind) builder.add(at, at + text.length, mark(kind))
      at += text.length
    }
  })
  return builder.finish()
}

const field = StateField.define({
  create: (state) => ({ errorLine: null, decorations: decorate(state.doc, null) }),
  update: (value, tr) => {
    const painted = tr.effects.findLast((effect) => effect.is(paint))
    const errorLine = painted ? painted.value : value.errorLine
    return errorLine === value.errorLine && !tr.docChanged
      ? value
      : { errorLine, decorations: decorate(tr.state.doc, errorLine) }
  },
  provide: (f) => EditorView.decorations.from(f, (value) => value.decorations),
})

export const highlighting = [field]
