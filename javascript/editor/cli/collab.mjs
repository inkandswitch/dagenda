// Work on a shared plan from the command line, alongside people in the
// editor. A workspace is an Automerge document listing a plan's documents,
// `{ documents: [{ name, url }] }`, each its own Automerge document with a
// `source` string: what the editor's tabs show. This keeps them as files in
// a directory, so any tool (or agent) can read and edit them:
//
//   nix run .#collab -- pull [dir] [automerge:…]  each document's source into <name>.dagenda
//   nix run .#collab -- push [dir]                local edits back, merged with everyone else's
//   nix run .#collab -- status [dir]              each document, and whether it has edits to push
//   nix run .#collab -- new [dir]                 a new workspace from the .dagenda files there
//
// `dir` (the current directory if left out) holds one workspace: its URL in
// `workspace.url`, a `<name>.dagenda` per document, and `.base/`, the text and
// heads each was pulled at. Give `pull` a workspace's URL the first time.
//
// A push never overwrites what others typed since the pull: each edit is
// made against the heads it was pulled at (`changeAt`), so Automerge merges
// it with their concurrent changes, as two tabs would. Then it waits for the
// sync server to have everything, and pulls again, so the files show the
// merged result. A `<name>.dagenda` with no document yet becomes a new
// document in the workspace.
//
// Syncs through the public Subduction server unless DAGENDA_SYNC names
// another (a `ws://` or `wss://` URL).

import * as A from "@automerge/automerge"
import { Repo } from "@automerge/automerge-repo"
import { SubductionBackend } from "@automerge/automerge-repo-subduction"
import { DummySubductionStorageAdapter } from "@automerge/automerge-repo-subduction/helpers/DummySubductionStorageAdapter.js"
import { WebSocketSubductionNetworkAdapter } from "@automerge/automerge-repo-subduction-network-websocket"
import { createMemorySigner } from "@automerge/subduction"
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs"
import { join, resolve } from "node:path"

const [command, dir, given] = process.argv.slice(2)
const HERE = resolve(dir ?? ".")
const WORKSPACE_FILE = join(HERE, "workspace.url")
const BASE = join(HERE, ".base")
const SERVER = process.env.DAGENDA_SYNC ?? "wss://subduction.sync.inkandswitch.com"
const EDITOR = "https://inkandswitch.github.io/dagenda/collab/"

const log = (...args) => console.error(...args)
const repo = new Repo({
  backend: new SubductionBackend({
    storage: new DummySubductionStorageAdapter(),
    network: [new WebSocketSubductionNetworkAdapter(SERVER)],
    signer: createMemorySigner(),
  }),
})

const file = (name) => join(HERE, `${name}.dagenda`)
const base = (name) => join(BASE, `${name}.json`)
const local = () =>
  readdirSync(HERE)
    .filter((f) => f.endsWith(".dagenda"))
    .map((f) => f.slice(0, -".dagenda".length))

/** The workspace's URL: `given`, saved for next time, or the one saved here. */
const workspaceUrl = () => {
  const saved = existsSync(WORKSPACE_FILE) ? readFileSync(WORKSPACE_FILE, "utf8").trim() : null
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

/** Write each document's source, and the heads it was read at. */
const pull = async () => {
  const workspace = await find(workspaceUrl())
  mkdirSync(BASE, { recursive: true })
  for (const { name, url } of workspace.doc().documents ?? []) {
    const handle = await find(url)
    const source = handle.doc().source ?? ""
    writeFileSync(file(name), source)
    writeFileSync(base(name), JSON.stringify({ url, heads: handle.heads(), source }, null, 2))
    log(`pulled ${name} (${source.split("\n").length} lines)`)
  }
  log(`open it in the editor: ${EDITOR}#${workspace.url}`)
}

const push = async () => {
  const workspace = await find(workspaceUrl())
  const entries = workspace.doc().documents ?? []
  const waits = []
  for (const { name } of entries) {
    if (!existsSync(file(name)) || !existsSync(base(name))) continue
    const mine = readFileSync(file(name), "utf8")
    const { url, heads, source } = JSON.parse(readFileSync(base(name), "utf8"))
    if (mine === source) continue
    const handle = await find(url)
    // Against the heads it was pulled at, so others' edits since then merge
    await handle.changeAt(heads, (doc) => A.updateText(doc, ["source"], mine), { message: "dagenda collab push" })
    waits.push(synced(handle))
    log(`pushed ${name}`)
  }
  // A local file with no document yet: a new document in the workspace
  const listed = new Set(entries.map((entry) => entry.name))
  let added = false
  for (const name of local().filter((name) => !listed.has(name))) {
    const handle = await repo.create({ source: readFileSync(file(name), "utf8") })
    waits.push(synced(handle))
    await workspace.change((doc) => {
      doc.documents.push({ name, url: handle.url })
    })
    added = true
    log(`added ${name} as ${handle.url}`)
  }
  // The server never confirms heads it already has, so only wait on the
  // workspace when it changed
  if (added) waits.push(synced(workspace))
  await Promise.all(waits)
  log(waits.length > 0 ? "the sync server has everything" : "nothing to push")
  await pull()
}

/** Each document: its name, URL, lines on the server, and whether the file here has edits to push. */
const status = async () => {
  const workspace = await find(workspaceUrl())
  const entries = workspace.doc().documents ?? []
  for (const { name, url } of entries) {
    const source = (await find(url)).doc().source ?? ""
    const pulled = existsSync(base(name)) ? JSON.parse(readFileSync(base(name), "utf8")).source : null
    const mine = existsSync(file(name)) ? readFileSync(file(name), "utf8") : null
    const state =
      mine === null ? "not pulled" : mine === pulled ? (source === pulled ? "up to date" : "behind: pull") : "edited: push"
    console.log(`${name}\t${url}\t${source.split("\n").length} lines\t${state}`)
  }
  const listed = new Set(entries.map((entry) => entry.name))
  for (const name of local().filter((name) => !listed.has(name))) console.log(`${name}\t-\tnew here\tpush adds it`)
}

/** A new workspace with a document for each `.dagenda` file here, `plan` first if there is one. */
const create = async () => {
  if (existsSync(WORKSPACE_FILE)) throw new Error(`${HERE} already has a workspace: ${readFileSync(WORKSPACE_FILE, "utf8").trim()}`)
  const names = local().sort((a, b) => (a === "plan" ? -1 : b === "plan" ? 1 : a.localeCompare(b)))
  if (names.length === 0) throw new Error(`no .dagenda files in ${HERE}`)
  const documents = []
  const waits = []
  for (const name of names) {
    const handle = await repo.create({ source: readFileSync(file(name), "utf8") })
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
  else if (command === "status") await status()
  else if (command === "new") {
    await create()
    await pull()
  } else throw new Error("usage: collab pull|push|status|new [dir] [automerge:URL for a first pull]")
  process.exit(0)
} catch (e) {
  log(`collab: ${e.message ?? e}`)
  process.exit(1)
}
