// Tab and Shift+Tab over a selection, for a plain textarea: indentation is
// structure in the notation, so it's always two spaces, never a tab.

const UNIT = "  "

/**
 * Indent every line a selection touches by two spaces, or with `outdent`
 * take up to two spaces off each. A selection that ends at the very start
 * of a line doesn't touch that line. Blank lines aren't indented, so no
 * line ends in spaces.
 *
 * Returns the replacement, `{ from, to, text }` (the lines touched, whole),
 * and the selection afterwards, `{ start, end }`: a caret moves with its
 * line's text; a selection keeps covering the lines it touched.
 *
 * @param {string} value  the whole text
 * @param {number} start  the selection's start
 * @param {number} end  the selection's end
 * @param {boolean} outdent  take indentation off, rather than add it
 */
export const indentLines = (value, start, end, outdent = false) => {
  const from = value.lastIndexOf("\n", start - 1) + 1
  const last = end > start && value[end - 1] === "\n" ? end - 1 : end
  const lineEnd = value.indexOf("\n", last)
  const to = lineEnd === -1 ? value.length : lineEnd

  const lines = value.slice(from, to).split("\n")
  const shifts = lines.map((line) =>
    outdent ? -Math.min(UNIT.length, /^ */.exec(line)[0].length) : line.length === 0 ? 0 : UNIT.length,
  )
  const text = lines.map((line, index) => (shifts[index] < 0 ? line.slice(-shifts[index]) : shifts[index] > 0 ? UNIT + line : line)).join("\n")

  if (start === end) {
    // A caret moves with its line's text, but never before the line's start
    const caret = Math.max(from, start + shifts[0])
    return { from, to, text, start: caret, end: caret }
  }
  if (lines.length === 1) {
    return { from, to, text, start: Math.max(from, start + shifts[0]), end: Math.max(from, end + shifts[0]) }
  }
  return { from, to, text, start: from, end: from + text.length }
}
