// Syntax colouring for the dagenda notation: HTML for a <pre> laid under the
// (transparent) textarea, line for line.

const BLOCKS = new Set(["extend", "milestone", "roadmap", "task", "title", "track"])
const DETAILS = new Set(["dri", "est", "issue", "needs", "note", "pr", "target"])

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
const span = (kind, text) => (text ? `<span class="${kind}">${escape(text)}</span>` : "")

/**
 * Colour `source`, marking `errorLine` (1-based) if given. The result has
 * exactly as many lines as `source`, plus a final newline so a <pre> is as
 * tall as the textarea.
 */
export const highlight = (source, errorLine = null) => {
  // Inside a `note` block, everything indented further than the `note` is
  // markdown, comments and all
  let noteIndent = null
  const lines = source.split("\n").map((line, index) => {
    const text = line.trimStart()
    const indent = line.slice(0, line.length - text.length)
    let html
    if (noteIndent !== null && (text === "" || indent.length > noteIndent)) {
      html = escape(indent) + span("markdown", text)
    } else {
      noteIndent = /^note\s*$/.test(text) ? indent.length : null
      html = escape(indent) + colour(text)
    }
    return index + 1 === errorLine ? `<span class="error-line">${html}</span>` : html
  })
  return `${lines.join("\n")}\n`
}

/** One line's text, after its indent. */
const colour = (text) => {
  if (text.startsWith("//")) return span("comment", text)
  if (text === "---") return span("cut", text)

  let html = ""
  let rest = text
  const box = /^\[(.)\](\s*)/u.exec(rest)
  if (box) {
    html += span(`box ${STATUS[box[1]] ?? "unknown"}`, `[${box[1]}]`) + box[2]
    rest = rest.slice(box[0].length)
  }

  const [, word = "", gap = "", after = ""] = /^(\S*)(\s*)(.*)$/su.exec(rest) ?? []
  if (BLOCKS.has(word)) {
    html += span("keyword", word) + gap
    if (word === "title") return html + span("label", after)
    const [, id = "", space = "", label = ""] = /^([^\s[]*)(\s*)(.*)$/su.exec(after) ?? []
    return html + span("id", id) + space + span("label", label)
  }

  if (DETAILS.has(word)) {
    html += span("detail", word) + gap
    switch (word) {
      case "needs":
        return html + after.split(/(,\s*)/).map((part, i) => (i % 2 ? escape(part) : span("id", part))).join("")
      case "note":
        return html + span("markdown", after)
      case "dri":
        return html + span("person", after)
      case "pr":
      case "issue":
        return html + span("ref", after)
      default:
        return html + span("value", after)
    }
  }

  // A lone id: a milestone member
  return html + span("id", word) + escape(gap + after)
}
