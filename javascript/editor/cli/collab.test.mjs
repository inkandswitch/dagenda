// The collab command against the sync server (DAGENDA_SYNC, or the public
// one): two directories stand in for two people. The deployed editor's
// parser comes from a stand-in site here, so these don't depend on what's
// deployed: `ok/` reads everything, and `strict/` is a site older than the
// plan's syntax. This repo's parser is the real Wasm build.

import assert from "node:assert/strict"
import { spawn } from "node:child_process"
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { createServer } from "node:http"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { after, before, test } from "node:test"

const SCRIPT = new URL("collab.mjs", import.meta.url).pathname
const PLAN = "roadmap\n[ ] a [A]\n[ ] b [B]\n  needs a\n"

const glue = (problems) =>
  `export default async function init() {}\nexport function checkPlan() { return ${JSON.stringify(problems)} }\n`
const SITES = {
  ok: glue([]),
  strict: glue([{ document: 0, line: 2, column: 1, severity: "error", message: "unknown line starting `@ann`" }]),
}

let site
let server
const root = mkdtempSync(join(tmpdir(), "dagenda-collab-"))
const dir = (name) => join(root, name)
const text = (name) => readFileSync(join(dir(name), "plan.dagenda"), "utf8")
const edit = (name, change) => writeFileSync(join(dir(name), "plan.dagenda"), change(text(name)))

/** Run the command; resolves to its exit code and output, failure or not. */
const collab = (args, { as = "test", siteName = "ok" } = {}) =>
  new Promise((resolve) => {
    const child = spawn(process.execPath, [SCRIPT, ...args, `--as=${as}`], {
      env: { ...process.env, DAGENDA_SITE: `${site}${siteName}/` },
    })
    let output = ""
    child.stdout.on("data", (chunk) => (output += chunk))
    child.stderr.on("data", (chunk) => (output += chunk))
    child.on("close", (code) => resolve({ code, output }))
  })

const ok = async (args, options) => {
  const { code, output } = await collab(args, options)
  assert.equal(code, 0, output)
  return output
}

before(async () => {
  server = createServer((request, response) => {
    const [, name, ...rest] = request.url.split("/")
    const path = rest.join("/")
    if (path === "dagenda-view/pkg/dagenda_wasm.js" && SITES[name]) {
      response.writeHead(200, { "content-type": "text/javascript" }).end(SITES[name])
    } else if (path === "dagenda-view/pkg/dagenda_wasm_bg.wasm") {
      response.writeHead(200, { "content-type": "application/wasm" }).end(Buffer.alloc(8))
    } else {
      response.writeHead(404).end()
    }
  })
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve))
  site = `http://127.0.0.1:${server.address().port}/`

  writeFileSync(join(root, "plan.dagenda"), PLAN)
  await ok(["new", root])
  const url = readFileSync(join(root, "workspace.url"), "utf8").trim()
  for (const person of ["ann", "bo"]) await ok(["pull", dir(person), url])
})

after(() => {
  server?.close()
  rmSync(root, { recursive: true, force: true })
})

test("a new workspace pulls back the same text, anywhere", () => {
  assert.equal(text("ann"), PLAN)
  assert.equal(text("bo"), PLAN)
})

test("pull won't write over edits that aren't pushed", async () => {
  edit("ann", (plan) => plan.replace("[ ] a [A]", "[-] a [A]"))
  const { code, output } = await collab(["pull", dir("ann")])
  assert.equal(code, 1)
  assert.match(output, /edits that aren't pushed/)
  assert.match(text("ann"), /\[-\] a \[A\]/, "the edit is still there")
  await ok(["pull", dir("ann"), "--discard"])
  assert.equal(text("ann"), PLAN, "until it's discarded")
})

test("push refuses a plan with errors, and changes nothing", async () => {
  edit("ann", (plan) => `${plan}[ ] c\n  needs nothing\n`)
  const { code, output } = await collab(["push", dir("ann")])
  assert.equal(code, 1)
  assert.match(output, /plan\.dagenda:\d+:\d+: error: nothing in the plan is called `nothing`/)
  await ok(["pull", dir("bo")])
  assert.equal(text("bo"), PLAN)
  await ok(["pull", dir("ann"), "--discard"])
})

test("push refuses what the deployed editor can't read yet, unless told to", async () => {
  edit("ann", (plan) => plan.replace("[ ] a [A]", "[!] a [A]"))
  const refused = await collab(["push", dir("ann")], { siteName: "strict" })
  assert.equal(refused.code, 1, refused.output)
  assert.match(refused.output, /deployed editor .* can't read this yet/)
  assert.match(refused.output, /plan\.dagenda:2:1: error: unknown line/)
  const pushed = await collab(["push", dir("ann"), "--ahead-of-site"], { siteName: "strict" })
  assert.equal(pushed.code, 0, pushed.output)
  assert.match(pushed.output, /will show errors until the site is redeployed/)
  await ok(["pull", dir("bo")])
  assert.match(text("bo"), /\[!\] a \[A\]/)
})

test("two people's pushes merge, and undo takes back only the last one's", async () => {
  // Both start from the same text; ann renames a, bo renames b
  await ok(["pull", dir("ann")])
  await ok(["pull", dir("bo")])
  edit("ann", (plan) => plan.replace("[A]", "[Apple]"))
  edit("bo", (plan) => plan.replace("[B]", "[Banana]"))
  await ok(["push", dir("ann")], { as: "ann" })
  await ok(["push", dir("bo")], { as: "bo" })
  assert.match(text("bo"), /\[Apple\][\s\S]*\[Banana\]/, "bo's push merged with ann's")

  // ann takes hers back; bo's, made since, stays
  assert.match(await ok(["status", dir("ann")]), /last push: by ann/)
  await ok(["undo", dir("ann")], { as: "ann" })
  assert.match(text("ann"), /\[A\][\s\S]*\[Banana\]/)
  await ok(["pull", dir("bo")])
  assert.match(text("bo"), /\[A\][\s\S]*\[Banana\]/)
})
