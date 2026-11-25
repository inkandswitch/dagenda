// The collaborative editor's page: a workspace (a plan's documents) synced
// over Subduction, named in the URL, with who's here, the connection, and
// whether everything has reached the sync server. Open the same URL in
// another browser to edit together.

// Importing the full packages initializes their Wasm; the repo and the
// Subduction backend import the `slim` builds, which share it
import "@automerge/automerge"
import { Repo, isValidAutomergeUrl } from "@automerge/automerge-repo"
import { SubductionBackend } from "@automerge/automerge-repo-subduction"
import { DummySubductionStorageAdapter } from "@automerge/automerge-repo-subduction/helpers/DummySubductionStorageAdapter.js"
import { WebSocketSubductionNetworkAdapter } from "@automerge/automerge-repo-subduction-network-websocket"
import { IndexedDBSubductionStorageAdapter } from "@automerge/automerge-repo-subduction-storage-indexeddb"
import { createMemorySigner, createWebCryptoSigner } from "@automerge/subduction"
import { automergeSyncPlugin } from "@automerge/automerge-codemirror"
import "./style.css"
import "./collab.css"
import { mountEditor } from "./editor.js"
import { BASIC } from "./examples.js"
import { carets, identity, remoteCarets, roster } from "./presence.js"

const SYNC_SERVER = new URLSearchParams(location.search).get("sync") ?? "wss://subduction.sync.inkandswitch.com"

const $ = (id) => document.getElementById(id)
const connection = $("connection")

// One tab per browser owns local storage: the IndexedDB adapter allows one
// live backend per database, across tabs too. That tab keeps documents for
// offline use, under the browser's persistent identity (the WebCrypto key
// is kept in IndexedDB). Other tabs sync in memory, each as its own peer, so
// the server never sees one identity connected twice.
const ownsStorage = await new Promise((resolve) => {
  if (!navigator.locks) return resolve(true)
  navigator.locks.request("dagenda-editor:storage", { ifAvailable: true }, (lock) => {
    resolve(lock !== null)
    // Held until the tab closes
    return lock ? new Promise(() => {}) : undefined
  })
})

// Sync: a Subduction backend with storage and the sync server
const server = new WebSocketSubductionNetworkAdapter(SYNC_SERVER)
server.subscribe((state) => {
  const name = state.status ?? String(state)
  connection.className = name
  connection.textContent = state.error ? `${name}: ${state.error}` : name
})
const signer = ownsStorage ? await createWebCryptoSigner() : createMemorySigner()
const backend = new SubductionBackend({
  storage: ownsStorage ? new IndexedDBSubductionStorageAdapter() : new DummySubductionStorageAdapter(),
  network: [server],
  signer,
})
const repo = new Repo({ backend })

// A document nobody has sent yet (just made elsewhere, or the server is
// unreachable) is unavailable for now, not forever: keep looking, and say so
const find = async (url) => {
  for (let wait = 500; ; wait = Math.min(wait * 2, 10_000)) {
    try {
      return await repo.find(url)
    } catch (e) {
      $("sync").className = "offline"
      $("sync").textContent = "… looking for this plan"
      $("sync").title = `${e.message ?? e}; trying again`
      await new Promise((resolve) => setTimeout(resolve, wait))
    }
  }
}

// The workspace named in the URL's hash, or a new one put there, with one
// document: the text given as `#source=…` (the preview's Collaborate
// button), or a small plan
const hash = location.hash.slice(1)
const given = new URLSearchParams(hash).get("source")
const create = async (source) => {
  const plan = await repo.create({ source })
  return repo.create({ documents: [{ name: "plan", url: plan.url }] })
}
const workspace = isValidAutomergeUrl(hash) ? await find(hash) : await create(given ?? BASIC)
// In place of the bare URL (or the given text), not as a step back to it,
// so reloading opens this workspace rather than making another
if (!isValidAutomergeUrl(hash)) history.replaceState(null, "", `#${workspace.url}`)
// Another workspace's link pasted into this tab changes only the hash: load it
addEventListener("hashchange", () => location.reload())
const share = $("share")
share.href = share.textContent = location.href
connection.title = ownsStorage
  ? "This tab keeps a copy for offline use"
  : "Another tab keeps the offline copy; this one syncs live only"

// Whether the sync server has everything this tab has, for the workspace and
// every document in it: their latest heads (sent as it takes changes in)
// match this copy's
const syncState = $("sync")
const watched = new Set()
const serverHeads = new Map()
const upToDate = (handle) => {
  const theirs = serverHeads.get(handle)
  const heads = handle.heads()
  return theirs?.length === heads.length && heads.every((head) => theirs.includes(head))
}
const showSync = () => {
  const synced = [...watched].every(upToDate)
  const [state, text, title] = synced
    ? ["synced", "✓ synced", "Every change here has reached the sync server"]
    : ["connected", "connecting"].includes(connection.className)
      ? ["syncing", "↻ syncing…", "Changes are on their way to or from the sync server"]
      : ownsStorage
        ? ["offline", "● not synced", "Changes are kept in this browser, and sync when the server is back"]
        : ["offline", "● not synced", "Changes are kept in this tab only (another tab keeps the offline copy) until the server is back"]
  syncState.className = state
  syncState.textContent = text
  syncState.title = title
}
const watch = (handle) => {
  if (watched.has(handle)) return
  watched.add(handle)
  handle.on("remote-heads", ({ heads }) => {
    serverHeads.set(handle, heads)
    showSync()
  })
  handle.on("change", showSync)
}
watch(workspace)
server.subscribe(showSync)

/** The collaborative editor's own help, after the shared help. */
const HELP = `
  <h3>Working together</h3>
  <ul>
    <li>Share the link in the header: anyone with it edits the same plan, live.</li>
    <li><b>+</b> adds a document to the plan (a private one, say). Double-click a tab to rename it.</li>
    <li>Click your own name in the header to change it.</li>
    <li><b>↶</b> and <b>↷</b> undo and redo your own edits, never anyone else's.</li>
  </ul>
`

/**
 * The workspace as the editor's store: its list of documents, each an
 * Automerge document with a `source` string. Until Keyhive, a private
 * document is one whose URL isn't linked from anything public.
 */
const store = (page) => ({
  list: () => (workspace.doc()?.documents ?? []).map(({ name, url }) => ({ id: url, name })),
  watch: (onChange) => workspace.on("change", onChange),
  open: async (url) => {
    const handle = await find(url)
    watch(handle)
    showSync()
    return {
      handle,
      source: () => handle.doc()?.source ?? "",
      extensions: automergeSyncPlugin({ handle, path: ["source"] }),
      subscribe: (onChange) => {
        handle.on("change", onChange)
        return () => handle.off("change", onChange)
      },
    }
  },
  rename: (url, name) =>
    workspace.change((doc) => {
      const entry = doc.documents.find((entry) => entry.url === url)
      if (entry) entry.name = name
    }),
  create: async (name) => {
    const handle = await repo.create({ source: "roadmap\n" })
    workspace.change((doc) => {
      doc.documents.push({ name, url: handle.url })
    })
    return handle.url
  },
  presence: () => {
    const user = identity(page)
    const attached = new Set()
    const here = roster({
      handle: workspace,
      user,
      list: $("people"),
      save: page.save,
      onRename: () => attached.forEach((tab) => tab.announce()),
    })
    return {
      extensions: remoteCarets,
      attach: ({ handle }, view) => {
        const tab = carets({ handle, view, user })
        attached.add(tab)
        return { update: tab.update, stop: () => (attached.delete(tab), tab.stop()) }
      },
      on: (name) => here.on(name),
    }
  },
})

const editor = await mountEditor({ prefix: "dagenda-editor", store, help: HELP })

// For poking at in the console, and for the browser tests
window.workspace = workspace
window.editor = editor
window.ownsStorage = ownsStorage
window.peerId = signer.peerId()
window.repo = repo
