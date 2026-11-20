// Syntax colouring for the dagenda notation, as tokens: the preview turns
// them into HTML for a <pre> laid under its (transparent) textarea, and the
// collaborative editor into CodeMirror decorations. Line for line.

const BLOCKS = new Set(["extend", "milestone", "nice-to-have", "roadmap", "task", "title", "track"])
const DETAILS = new Set(["dri", "est", "issue", "link", "needs", "note", "pr", "target"])

/** The status class for each box's mark, as the SVG uses. */
export const STATUS = {
  " ": "not-started",
  ">": "deferred",
  x: "done",
  "-": "in-progress",
  "?": "idea",
  "!": "needs-attention",
  "*": "research",
  "~": "dropped",
}

const escape = (text) => text.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c])
const token = (kind, text) => ({ kind, text })

/**
 * Each line of `source` as tokens, `{ kind, text }`: `kind` is a class list
 * (`"keyword"`, `"box done"`), or `""` for plain text. One array per line, so
 * there are exactly as many as `source` has lines, and each line's texts
 * join back into it.
 */
export const tokens = (source) => {
  // Inside a `note` block, everything indented further than the `note` is
  // markdown, comments and all
  let noteIndent = null
  return source.split("\n").map((line) => {
    const text = line.trimStart()
    const indent = token("", line.slice(0, line.length - text.length))
    if (noteIndent !== null && (text === "" || indent.text.length > noteIndent)) {
      return [indent, token("markdown", text)].filter((part) => part.text)
    }
    noteIndent = /^note\s*$/.test(text) ? indent.text.length : null
    return [indent, ...colour(text)].filter((part) => part.text)
  })
}

/**
 * Colour `source` as HTML, marking `errorLine` (1-based) if given. The result
 * has exactly as many lines as `source`, plus a final newline so a <pre> is
 * as tall as the textarea.
 */
export const highlight = (source, errorLine = null) => {
  const lines = tokens(source).map((line, index) => {
    const html = line.map(({ kind, text }) => (kind ? `<span class="${kind}">${escape(text)}</span>` : escape(text))).join("")
    return index + 1 === errorLine ? `<span class="error-line">${html}</span>` : html
  })
  return `${lines.join("\n")}\n`
}

/** One line's tokens, after its indent. */
const colour = (text) => {
  if (text.startsWith("//")) return [token("comment", text)]

  const parts = []
  let rest = text
  const box = /^\[(.)\](\s*)/u.exec(rest)
  if (box) {
    parts.push(token(`box ${STATUS[box[1]] ?? "unknown"}`, `[${box[1]}]`), token("", box[2]))
    rest = rest.slice(box[0].length)
  }

  const [, word = "", gap = "", after = ""] = /^(\S*)(\s*)(.*)$/su.exec(rest) ?? []
  if (BLOCKS.has(word)) {
    parts.push(token("keyword", word), token("", gap))
    if (word === "title") return [...parts, token("label", after)]
    const [, id = "", space = "", label = ""] = /^([^\s[]*)(\s*)(.*)$/su.exec(after) ?? []
    return [...parts, token("id", id), token("", space), token("label", label)]
  }

  if (DETAILS.has(word)) {
    parts.push(token("detail", word), token("", gap))
    switch (word) {
      case "needs":
        return [...parts, ...after.split(/(,\s*)/).map((part, i) => token(i % 2 ? "" : "id", part))]
      case "note":
        return [...parts, token("markdown", after)]
      case "dri":
        return [...parts, token("person", after)]
      case "pr":
      case "issue":
        return [...parts, token("ref", after)]
      case "link": {
        const [, url = "", space = "", label = ""] = /^(\S*)(\s*)(.*)$/su.exec(after) ?? []
        return [...parts, token("ref", url), token("", space), token("label", label)]
      }
      default:
        return [...parts, token("value", after)]
    }
  }

  // A lone id: a milestone member
  return [...parts, token("id", word), token("", gap + after)]
}
