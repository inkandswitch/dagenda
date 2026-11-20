// Work on a shared plan from the command line, alongside people in the
// editor. A workspace is an Automerge document listing a plan's documents,
// `{ documents: [{ name, url }] }`, each its own Automerge document with a
// `source` string: what the editor's tabs show. This keeps them as files in
// a directory, so any tool (or agent) can read and edit them (`nix run
// .#collab` with no command starts the editor itself):
//
//   nix run .#collab -- pull [dir] [automerge:…]  each document's source into <name>.dagenda
//   nix run .#collab -- push [dir]                local edits back, merged with everyone else's
//   nix run .#collab -- undo [dir]                take back the last push, keeping what others typed since
//   nix run .#collab -- status [dir]              each document, whether it has edits to push, and the last push
//   nix run .#collab -- diff [dir]                what's changed since the pull, and who changed it, and your edits
//   nix run .#collab -- new [dir]                 a new workspace from the .dagenda files there
//
// `dir` (the current directory if left out) holds one workspace: its URL in
// `workspace.url`, a `<name>.dagenda` per document, and `.base/`: the text and
// heads each was pulled at, and the pushes `undo` can take back. Give `pull`
// a workspace's URL the first time.
//
// Nothing here loses work:
//
// - `pull` won't write over a file with edits that aren't pushed (`--discard`
//   to drop them).
// - `push` makes each edit against the heads it was pulled at (`changeAt`), so
//   Automerge merges it with what others typed since, as two tabs would. It
//   waits for the sync server to have everything, then pulls the merged
//   result. A `<name>.dagenda` with no document yet becomes a new document.
// - `push` (and `new`) check the plan first, with this repo's parser and with
//   the deployed editor's, and refuse on errors: people would see them. Pass
//   `--ahead-of-site` to push syntax the deployed editor doesn't know yet, and
//   `--no-check` to skip both.
// - `undo` makes the inverse of the last push at the heads that push made, so
//   it takes back only that push's edits, and merges with anything typed since.
//
// Pushes are named, so people can tell whose edits are whose: `--as=NAME`, or
// DAGENDA_AGENT, or "agent".
//
// Syncs through the public Subduction server unless DAGENDA_SYNC names another
// (a `ws://` or `wss://` URL). The deployed editor is DAGENDA_SITE, by default
// https://inkandswitch.github.io/dagenda/.

import * as A from "@automerge/automerge"
import { Repo, decodeHeads } from "@automerge/automerge-repo"
import { SubductionBackend } from "@automerge/automerge-repo-subduction"
import { DummySubductionStorageAdapter } from "@automerge/automerge-repo-subduction/helpers/DummySubductionStorageAdapter.js"
import { WebSocketSubductionNetworkAdapter } from "@automerge/automerge-repo-subduction-network-websocket"
import { createMemorySigner } from "@automerge/subduction"
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs"
import { join, resolve } from "node:path"

const args = process.argv.slice(2)
const flags = new Set(args.filter((arg) => arg.startsWith("--")).map((arg) => arg.split("=")[0]))
const option = (name) => args.find((arg) => arg.startsWith(`--${name}=`))?.slice(name.length + 3)
const [command, dir, given] = args.filter((arg) => !arg.startsWith("--"))

const HERE = resolve(dir ?? ".")
const WORKSPACE_FILE = join(HERE, "workspace.url")
const BASE = join(HERE, ".base")
const PUSHES = join(BASE, "pushes.json")
const SERVER = process.env.DAGENDA_SYNC ?? "wss://subduction.sync.inkandswitch.com"
const SITE = process.env.DAGENDA_SITE ?? "https://inkandswitch.github.io/dagenda/"
const EDITOR = new URL("collab/", SITE).href
const NAME = option("as") ?? process.env.DAGENDA_AGENT ?? "agent"

const log = (...lines) => console.error(...lines)
const repo = new Repo({
  backend: new SubductionBackend({
    storage: new DummySubductionStorageAdapter(),
    network: [new WebSocketSubductionNetworkAdapter(SERVER)],
    signer: createMemorySigner(),
  }),
})

const file = (name) => join(HERE, `${name}.dagenda`)
const base = (name) => join(BASE, `${name}.json`)
const read = (path) => (existsSync(path) ? readFileSync(path, "utf8") : null)
const pulled = (name) => (existsSync(base(name)) ? JSON.parse(readFileSync(base(name), "utf8")) : null)
const local = () =>
  readdirSync(HERE)
    .filter((f) => f.endsWith(".dagenda"))
    .map((f) => f.slice(0, -".dagenda".length))
const pushes = () => (existsSync(PUSHES) ? JSON.parse(readFileSync(PUSHES, "utf8")) : [])

/** The workspace's URL: `given`, saved for next time, or the one saved here. */
const workspaceUrl = () => {
  const saved = read(WORKSPACE_FILE)?.trim()
  if (given) {
    if (!given.startsWith("automerge:")) throw new Error(`expected an automerge: URL, not ${given}`)
    if (saved && saved !== given) throw new Error(`${HERE} already holds ${saved}; use another directory for ${given}`)
    mkdirSync(HERE, { recursive: true })
    writeFileSync(WORKSPACE_FILE, `${given}\n`)
    return given
  }
  if (!saved) throw new Error(`no workspace.url in ${HERE}: pull it with its automerge: URL, or make one with \`new\``)
  return saved
}

const find = async (url) => {
  for (let wait = 1000, tries = 0; ; tries++, wait = Math.min(wait * 2, 8000)) {
    try {
      return await repo.find(url)
    } catch (e) {
      if (tries >= 6) throw new Error(`can't find ${url}: ${e.message ?? e}`)
      await new Promise((resolve) => setTimeout(resolve, wait))
    }
  }
}

/** Resolve once the sync server reports the same heads as this copy. */
const synced = (handle, timeout = 30_000) =>
  new Promise((resolve, reject) => {
    const mine = () => handle.heads()
    const same = (theirs) => theirs.length === mine().length && mine().every((head) => theirs.includes(head))
    const timer = setTimeout(() => reject(new Error(`the server hasn't confirmed ${handle.url} after ${timeout / 1000}s`)), timeout)
    handle.on("remote-heads", ({ heads }) => {
      if (same(heads)) {
        clearTimeout(timer)
        resolve()
      }
    })
  })

// --- Checking ---------------------------------------------------------------

/**
 * A plan checker from wasm-bindgen's web build of `dagenda_wasm`: its JS glue's
 * source, and its Wasm. `label` keeps two builds apart: the same glue imported
 * twice would be one module, sharing one Wasm.
 */
const checker = async (label, glue, wasm) => {
  const source = `${glue}\n// ${label}\n`
  const module = await import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`)
  await module.default({ module_or_path: wasm })
  return (documents) => module.checkPlan(documents)
}

/** This repo's parser: the build in javascript/dagenda-view/pkg (`nix run .#collab` makes it). */
const repoChecker = () => {
  const pkg = new URL("../../dagenda-view/pkg/", import.meta.url)
  const glue = new URL("dagenda_wasm.js", pkg)
  if (!existsSync(glue)) throw new Error(`no Wasm build at ${pkg.pathname}: run through \`nix run .#collab\`, or pass --no-check`)
  return checker("repo", readFileSync(glue, "utf8"), readFileSync(new URL("dagenda_wasm_bg.wasm", pkg)))
}

/** The deployed editor's parser, from the site. */
const siteChecker = async () => {
  const pkg = new URL("dagenda-view/pkg/", SITE)
  const get = async (name) => {
    const response = await fetch(new URL(name, pkg), { signal: AbortSignal.timeout(20_000) })
    if (!response.ok) throw new Error(`${response.status} for ${response.url}`)
    return response
  }
  const [glue, wasm] = await Promise.all([get("dagenda_wasm.js").then((r) => r.text()), get("dagenda_wasm_bg.wasm").then((r) => r.arrayBuffer())])
  return checker("site", glue, new Uint8Array(wasm))
}

const errorsIn = (check, documents) => check(documents).filter((problem) => problem.severity === "error")
const where = (documents) => (problem) =>
  `${documents[problem.document]?.name ?? problem.document}.dagenda:${problem.line}:${problem.column}: ${problem.severity}: ${problem.message}`

/**
 * Refuse to share a plan with errors: ones this repo's parser finds, or ones
 * only the deployed editor finds (the syntax is newer than the site), unless
 * told to.
 */
const check = async (documents) => {
  if (flags.has("--no-check")) return
  const ours = errorsIn(await repoChecker(), documents)
  if (ours.length > 0) {
    throw new Error(`the plan has errors, so nothing was pushed:\n${ours.map(where(documents)).join("\n")}`)
  }
  let theirs
  try {
    theirs = errorsIn(await siteChecker(), documents)
  } catch (e) {
    const cause = e.cause?.code ?? e.cause?.message
    log(`couldn't load the deployed editor's parser (${e.message ?? e}${cause ? `: ${cause}` : ""}); checked with this repo's only`)
    return
  }
  if (theirs.length === 0) return
  const problems = theirs.map(where(documents)).join("\n")
  if (flags.has("--ahead-of-site")) {
    log(`the deployed editor will show errors until the site is redeployed:\n${problems}`)
    return
  }
  throw new Error(
    `the deployed editor (${SITE}) can't read this yet, so people would see errors; nothing was pushed. Redeploy the site first, or push anyway with --ahead-of-site:\n${problems}`,
  )
}

// --- Commands ---------------------------------------------------------------

/**
 * Write each document's source, and the heads it was read at. Refuses to
 * write over edits that aren't pushed, unless `force` (after a push, or with
 * `--discard`).
 */
const pull = async (force = flags.has("--discard")) => {
  const workspace = await find(workspaceUrl())
  const documents = []
  for (const { name, url } of workspace.doc().documents ?? []) {
    const handle = await find(url)
    documents.push({ name, url, handle, source: handle.doc().source ?? "" })
  }
  if (!force) {
    const edited = documents.filter(({ name, source }) => {
      const mine = read(file(name))
      return mine !== null && mine !== (pulled(name)?.source ?? source)
    })
    if (edited.length > 0) {
      const names = edited.map(({ name }) => `${name}.dagenda`).join(", ")
      throw new Error(`${names} in ${HERE} has edits that aren't pushed: push them (they merge with what's new), or pull with --discard to drop them`)
    }
  }
  mkdirSync(BASE, { recursive: true })
  for (const { name, url, handle, source } of documents) {
    writeFileSync(file(name), source)
    writeFileSync(base(name), JSON.stringify({ url, heads: handle.heads(), source }, null, 2))
    log(`pulled ${name} (${source.split("\n").length} lines)`)
  }
  log(`open it in the editor: ${EDITOR}#${workspace.url}`)
}

const push = async () => {
  const workspace = await find(workspaceUrl())
  const entries = workspace.doc().documents ?? []
  const listed = new Set(entries.map((entry) => entry.name))
  const edits = []
  const plan = []
  for (const { name, url } of entries) {
    const mine = read(file(name))
    const was = pulled(name)
    if (mine !== null && was === null) throw new Error(`${name}.dagenda has never been pulled: pull first`)
    if (mine === null) {
      plan.push({ name, source: (await find(url)).doc().source ?? "" })
      continue
    }
    plan.push({ name, source: mine })
    if (mine !== was.source) edits.push({ name, url, was, mine })
  }
  const added = local().filter((name) => !listed.has(name))
  for (const name of added) plan.push({ name, source: read(file(name)) })
  if (edits.length === 0 && added.length === 0) {
    log("nothing to push")
    return
  }
  await check(plan)

  const message = `dagenda collab push by ${NAME}`
  const record = { at: new Date().toISOString(), by: NAME, documents: [] }
  const waits = []
  for (const { name, url, was, mine } of edits) {
    const handle = await find(url)
    // Against the heads it was pulled at, so others' edits since then merge
    const heads = await handle.changeAt(was.heads, (doc) => A.updateText(doc, ["source"], mine), { message })
    waits.push(synced(handle))
    record.documents.push({ name, url, before: was.source, after: mine, heads })
    log(`pushed ${name}`)
  }
  for (const name of added) {
    const handle = await repo.create({ source: read(file(name)) })
    waits.push(synced(handle))
    await workspace.change((doc) => {
      doc.documents.push({ name, url: handle.url })
    }, { message })
    record.documents.push({ name, url: handle.url, added: true })
    log(`added ${name} as ${handle.url}`)
  }
  // The server never confirms heads it already has, so only wait on the
  // workspace when it changed
  if (added.length > 0) waits.push(synced(workspace))
  await Promise.all(waits)
  mkdirSync(BASE, { recursive: true })
  writeFileSync(PUSHES, JSON.stringify([...pushes(), record], null, 2))
  log("the sync server has everything")
  await pull(true)
}

/**
 * Take back the last push: the inverse of each edit, made at the heads that
 * push made, so it merges with anything typed since. A document the push
 * added comes off the workspace's list.
 */
const undo = async () => {
  const done = pushes()
  const last = done.at(-1)
  if (!last) throw new Error(`no push from ${HERE} to undo`)
  // Undo starts from what's on the server, so local edits would be lost
  const workspace = await find(workspaceUrl())
  for (const { name } of workspace.doc().documents ?? []) {
    const mine = read(file(name))
    if (mine !== null && pulled(name) && mine !== pulled(name).source) {
      throw new Error(`${name}.dagenda has edits that aren't pushed: push them, or pull with --discard, before undoing`)
    }
  }
  const message = `dagenda collab undo by ${NAME}`
  const waits = []
  for (const { name, url, before, heads, added } of last.documents) {
    if (added) {
      await workspace.change((doc) => {
        const index = doc.documents.findIndex((entry) => entry.url === url)
        if (index >= 0) doc.documents.splice(index, 1)
      }, { message })
      waits.push(synced(workspace))
      log(`took ${name} off the workspace (the document itself stays on the server)`)
      continue
    }
    const handle = await find(url)
    await handle.changeAt(heads, (doc) => A.updateText(doc, ["source"], before), { message })
    waits.push(synced(handle))
    log(`took back ${last.by}'s push to ${name}, from ${last.at}`)
  }
  await Promise.all(waits)
  writeFileSync(PUSHES, JSON.stringify(done.slice(0, -1), null, 2))
  await pull(true)
}

/**
 * `before` to `after`, line by line: each change with two lines around it,
 * under `@@ line N @@` (N in `after`), lines taken out marked `-`, put in `+`.
 */
const lineDiff = (before, after, context = 2) => {
  const a = before.split("\n")
  const b = after.split("\n")
  // Only what's between the common start and end needs the table
  let start = 0
  while (start < a.length && start < b.length && a[start] === b[start]) start++
  let end = 0
  while (end < a.length - start && end < b.length - start && a[a.length - 1 - end] === b[b.length - 1 - end]) end++
  const x = a.slice(start, a.length - end)
  const y = b.slice(start, b.length - end)
  const longest = Array.from({ length: x.length + 1 }, () => new Uint32Array(y.length + 1))
  for (let i = x.length - 1; i >= 0; i--) {
    for (let j = y.length - 1; j >= 0; j--) {
      longest[i][j] = x[i] === y[j] ? longest[i + 1][j + 1] + 1 : Math.max(longest[i + 1][j], longest[i][j + 1])
    }
  }
  const ops = a.slice(0, start).map((line, k) => [" ", line, k])
  let [i, j] = [0, 0]
  while (i < x.length || j < y.length) {
    if (i < x.length && j < y.length && x[i] === y[j]) ops.push([" ", x[i++], start + j++])
    else if (j >= y.length || (i < x.length && longest[i + 1][j] >= longest[i][j + 1])) ops.push(["-", x[i++], start + j])
    else ops.push(["+", y[j], start + j++])
  }
  ops.push(...b.slice(b.length - end).map((line, k) => [" ", line, b.length - end + k]))

  const near = ops.map((_, k) => ops.slice(Math.max(0, k - context), k + context + 1).some(([kind]) => kind !== " "))
  const out = []
  ops.forEach(([kind, line, at], k) => {
    if (!near[k]) return
    if (k === 0 || !near[k - 1]) out.push(`@@ line ${at + 1} @@`)
    out.push(`${kind} ${line}`)
  })
  return out.join("\n")
}

/** Who made `changes`: each named push (or undo), then the editor's. */
const authors = (changes) => {
  const named = new Map()
  const sessions = new Set()
  let typed = 0
  for (const change of changes) {
    const by = /^dagenda collab (?:push|undo) by (.+)$/.exec(change.message ?? "")?.[1]
    if (by) named.set(by, (named.get(by) ?? 0) + 1)
    else {
      sessions.add(change.actor)
      typed += 1
    }
  }
  const plural = (n, word) => `${n} ${word}${n === 1 ? "" : "s"}`
  const parts = [...named].map(([who, n]) => `${who} (${plural(n, "push")})`.replace("pushs", "pushes"))
  if (typed > 0) parts.push(`the editor (${plural(typed, "change")}${sessions.size > 1 ? `, from ${sessions.size} sessions` : ""})`)
  const latest = Math.max(...changes.map((change) => change.time ?? 0))
  const when = latest > 0 ? `, latest at ${new Date(latest * 1000).toISOString().slice(0, 16).replace("T", " ")} UTC` : ""
  return `${parts.join(", ")}${when}`
}

/**
 * What's changed since the pull: on the server, and who changed it; and here,
 * the edits not pushed yet. Read it before pushing over what people did, or
 * to answer for it.
 */
const diff = async () => {
  const workspace = await find(workspaceUrl())
  const entries = workspace.doc().documents ?? []
  let changed = false
  for (const { name, url } of entries) {
    const was = pulled(name)
    if (!was) {
      console.log(`${name}: not pulled here yet`)
      continue
    }
    const handle = await find(url)
    const server = handle.doc().source ?? ""
    if (server !== was.source) {
      changed = true
      const changes = A.getChangesMetaSince(handle.doc(), decodeHeads(was.heads))
      console.log(`${name}: changed on the server since your pull, by ${authors(changes)}`)
      console.log(lineDiff(was.source, server))
    }
    const mine = read(file(name))
    if (mine !== null && mine !== was.source) {
      changed = true
      console.log(`${name}: your edits, not pushed`)
      console.log(lineDiff(was.source, mine))
    }
  }
  const listed = new Set(entries.map((entry) => entry.name))
  for (const name of local().filter((name) => !listed.has(name))) {
    changed = true
    console.log(`${name}: new here; push adds it`)
  }
  if (!changed) console.log("nothing has changed since your pull")
}

/** Each document: its name, URL, lines on the server, and whether the file here has edits to push; then the last push. */
const status = async () => {
  const workspace = await find(workspaceUrl())
  const entries = workspace.doc().documents ?? []
  for (const { name, url } of entries) {
    const source = (await find(url)).doc().source ?? ""
    const was = pulled(name)?.source ?? null
    const mine = read(file(name))
    const state = mine === null ? "not pulled" : mine !== was ? "edited: push" : source === was ? "up to date" : "behind: pull"
    console.log(`${name}\t${url}\t${source.split("\n").length} lines\t${state}`)
  }
  const listed = new Set(entries.map((entry) => entry.name))
  for (const name of local().filter((name) => !listed.has(name))) console.log(`${name}\t-\tnew here\tpush adds it`)
  const last = pushes().at(-1)
  if (last) console.log(`last push: by ${last.by} at ${last.at}, to ${last.documents.map((d) => d.name).join(", ")} (undo takes it back)`)
}

/** A new workspace with a document for each `.dagenda` file here, `plan` first if there is one. */
const create = async () => {
  if (existsSync(WORKSPACE_FILE)) throw new Error(`${HERE} already has a workspace: ${read(WORKSPACE_FILE).trim()}`)
  const names = local().sort((a, b) => (a === "plan" ? -1 : b === "plan" ? 1 : a.localeCompare(b)))
  if (names.length === 0) throw new Error(`no .dagenda files in ${HERE}`)
  await check(names.map((name) => ({ name, source: read(file(name)) })))
  const documents = []
  const waits = []
  for (const name of names) {
    const handle = await repo.create({ source: read(file(name)) })
    waits.push(synced(handle))
    documents.push({ name, url: handle.url })
  }
  const workspace = await repo.create({ documents })
  waits.push(synced(workspace))
  await Promise.all(waits)
  writeFileSync(WORKSPACE_FILE, `${workspace.url}\n`)
  log(`created ${workspace.url} with ${names.join(", ")}; the sync server has everything`)
}

try {
  if (command === "pull") await pull()
  else if (command === "push") await push()
  else if (command === "undo") await undo()
  else if (command === "status") await status()
  else if (command === "diff") await diff()
  else if (command === "new") {
    await create()
    await pull(true)
  } else {
    throw new Error("usage: collab pull|push|undo|status|diff|new [dir] [automerge:URL for a first pull] [--as=NAME] [--discard] [--no-check] [--ahead-of-site]")
  }
  process.exit(0)
} catch (e) {
  log(`collab: ${e.message ?? e}`)
  process.exit(1)
}
