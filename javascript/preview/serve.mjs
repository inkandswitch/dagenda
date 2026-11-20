// Live-reloading dev server for the preview page, as in rain. No dependencies.
//
// It serves javascript/ with caching off, injects a script into HTML that
// reloads on a signal (Server-Sent Events: the signal is one-way, and Node has
// no built-in WebSocket server), and watches two sets of files:
//
//   - Rust sources (dagenda_core, dagenda_wasm) trigger a Wasm rebuild, then
//     a reload. A failed build tells the page, which keeps the last good one.
//   - javascript/ files trigger only a reload. Build output
//     (javascript/dagenda-view/pkg/) is never watched, so a build can't
//     trigger another build.
//
// A "change" only counts if the file's mtime moved, since some platforms
// (Linux inotify) report metadata-only changes too.
//
// `/collab/` goes to the collaborative editor's own dev server (`nix run
// .#editor`, on EDITOR_PORT, default 5173), as the deployed site has it next
// to the preview; the browser carries the `#source=…` hash across.
//
// Usage: node javascript/preview/serve.mjs   (env: PORT, default 8080)

import { spawn } from "node:child_process"
import * as FS from "node:fs"
import * as HTTP from "node:http"
import * as Path from "node:path"

const previewDir = Path.dirname(new URL(import.meta.url).pathname)
const javascriptDir = Path.dirname(previewDir)
const root = Path.dirname(javascriptDir)
const pkgDir = Path.join(javascriptDir, "dagenda-view", "pkg")

const MIME = {
  css: "text/css",
  dagenda: "text/plain; charset=utf-8",
  html: "text/html",
  js: "text/javascript",
  json: "application/json",
  mjs: "text/javascript",
  svg: "image/svg+xml",
  ts: "text/plain",
  wasm: "application/wasm",
}

const RELOAD_SCRIPT = `<script>
  const events = new EventSource("/__events")
  events.addEventListener("reload", () => location.reload())
  events.addEventListener("build-error", (e) => window.dispatchEvent(new CustomEvent("dagenda:build-error", { detail: JSON.parse(e.data) })))
</script>`

const log = (message) => console.log(`\x1b[2m${new Date().toLocaleTimeString()}\x1b[0m ${message}`)

// --- Events ------------------------------------------------------------------

const clients = new Set()

const broadcast = (event, data = "") => {
  for (const response of clients) response.write(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`)
}

// --- Build -------------------------------------------------------------------

let building = false
let pending = false

const build = () => {
  if (building) {
    pending = true
    return
  }

  building = true
  log("building Wasm…")
  const args = ["build", "--dev", "--target", "web", "--out-dir", pkgDir, Path.join(root, "rust", "dagenda_wasm")]
  const child = spawn("wasm-pack", args, { cwd: root })
  let output = ""
  child.stdout.on("data", (chunk) => (output += chunk))
  child.stderr.on("data", (chunk) => (output += chunk))

  child.on("close", (code) => {
    building = false
    if (code === 0) {
      log("\x1b[32mbuilt\x1b[0m, reloading")
      broadcast("reload")
    } else {
      log(`\x1b[31mbuild failed\x1b[0m\n${output}`)
      broadcast("build-error", output.replace(/\x1b\[[0-9;]*m/g, ""))
    }

    if (pending) {
      pending = false
      build()
    }
  })
}

// --- Watch -------------------------------------------------------------------

const debounce = (ms, f) => {
  let timer
  return () => {
    clearTimeout(timer)
    timer = setTimeout(f, ms)
  }
}

const mtimes = new Map()

// Only count a change if the file's content timestamp moved
const reallyChanged = (path) => {
  try {
    const mtime = FS.statSync(path).mtimeMs
    if (mtimes.get(path) === mtime) return false
    mtimes.set(path, mtime)
    return true
  } catch {
    return mtimes.delete(path) // deleted: counts once
  }
}

const watch = (dir, { ignore = () => false, onChange }) => {
  // Record starting mtimes, so the first metadata-only event isn't taken
  // for an edit
  for (const file of FS.readdirSync(dir, { recursive: true })) {
    if (!ignore(file)) reallyChanged(Path.join(dir, file))
  }

  const run = debounce(150, onChange)
  FS.watch(dir, { recursive: true }, (_event, file) => {
    if (!file || ignore(file)) return
    if (reallyChanged(Path.join(dir, file))) run()
  })
}

// Skip editor lock and backup files, such as Emacs's `.#foo.rs` and `#foo.rs#`
const isRustSource = (file) => {
  const base = Path.basename(file)
  return !/^[.#]/.test(base) && (base.endsWith(".rs") || base === "Cargo.toml")
}

for (const crate of ["dagenda_core", "dagenda_wasm"]) {
  watch(Path.join(root, "rust", crate), { ignore: (file) => !isRustSource(file), onChange: build })
}

watch(javascriptDir, {
  ignore: (file) =>
    file === Path.join("preview", "serve.mjs") ||
    file.split(Path.sep).some((part) => part === "pkg" || part === "node_modules" || part.startsWith(".")),
  onChange: () => {
    log("page changed, reloading")
    broadcast("reload")
  },
})

// --- Serve -------------------------------------------------------------------

const NO_CACHE = {
  "Cache-Control": "no-cache, no-store, must-revalidate",
  Expires: "-1",
  Pragma: "no-cache",
}

const server = HTTP.createServer((request, response) => {
  const url = decodeURI((request.url ?? "/").split("?")[0])

  if (url === "/__events") {
    response.writeHead(200, { ...NO_CACHE, "Content-Type": "text/event-stream", Connection: "keep-alive" })
    response.write(": connected\n\n")
    clients.add(response)
    request.on("close", () => clients.delete(response))
    return
  }

  if (url === "/") {
    response.writeHead(302, { ...NO_CACHE, Location: "/preview/" }).end()
    return
  }

  if (url === "/collab/" || url === "/collab") {
    const editor = `http://localhost:${process.env.EDITOR_PORT ?? 5173}/`
    response.writeHead(302, { ...NO_CACHE, Location: editor }).end()
    return
  }

  const file = Path.join(javascriptDir, url.endsWith("/") ? `${url}index.html` : url)
  if (!file.startsWith(javascriptDir + Path.sep)) {
    response.writeHead(403).end()
    return
  }

  const type = MIME[Path.extname(file).slice(1)]
  if (!type) {
    response.writeHead(415).end()
    return
  }

  FS.readFile(file, (error, content) => {
    if (error) {
      response.writeHead(error.code === "ENOENT" ? 404 : 500).end()
      return
    }

    if (type === "text/html") {
      const html = content.toString()
      content = html.includes("</body>") ? html.replace("</body>", `${RELOAD_SCRIPT}\n</body>`) : html + RELOAD_SCRIPT
    }

    response.writeHead(200, { ...NO_CACHE, "Content-Type": type }).end(content)
  })
})

let port = Number(process.env.PORT ?? 8080)

server.on("error", (error) => {
  if (error.code !== "EADDRINUSE") throw error
  log(`port ${port} in use, trying ${port + 1}`)
  server.listen(++port, "127.0.0.1")
})

server.on("listening", () => log(`preview at \x1b[32mhttp://127.0.0.1:${port}/\x1b[0m`))

build()
server.listen(port, "127.0.0.1")
