// Both pages, the preview (preview/) and the collaborative editor (collab/),
// with / sending you to the preview, as the deployed site has them. They
// share their panes and header (src/*.html, through `<!-- include … -->`).
//
// In dev, Rust sources (dagenda_core, dagenda_wasm) rebuild the component's
// Wasm, then reload the page. A failed build tells the page, which keeps the
// last good one. A "change" only counts if the file's mtime moved, since
// some platforms (Linux inotify) report metadata-only changes too.

import { spawn } from "node:child_process"
import * as FS from "node:fs"
import * as Path from "node:path"
import { fileURLToPath } from "node:url"
import { defineConfig } from "vite"

const here = Path.dirname(fileURLToPath(import.meta.url))
const root = Path.resolve(here, "../..")
const pkg = Path.join(root, "javascript", "dagenda-view", "pkg")

/** `<!-- include src/panes.html -->` becomes that file, from this directory. */
const include = () => ({
  name: "dagenda:include",
  transformIndexHtml: {
    order: "pre",
    handler: (html) => html.replace(/<!-- include (\S+) -->/g, (_, file) => FS.readFileSync(Path.join(here, file), "utf8").trimEnd()),
  },
})

/** Rebuild the Wasm when the Rust changes, then reload (dev only). */
const wasm = () => ({
  name: "dagenda:wasm",
  apply: "serve",
  configureServer: (server) => {
    const log = (message) => server.config.logger.info(message, { timestamp: true })
    let building = false
    let pending = false
    const build = () => {
      if (building) return void (pending = true)
      building = true
      log("building Wasm…")
      const child = spawn("wasm-pack", ["build", "--dev", "--target", "web", "--out-dir", pkg, Path.join(root, "rust", "dagenda_wasm")], { cwd: root })
      let output = ""
      child.stdout.on("data", (chunk) => (output += chunk))
      child.stderr.on("data", (chunk) => (output += chunk))
      child.on("close", (code) => {
        building = false
        if (code === 0) {
          log("built, reloading")
          server.ws.send({ type: "full-reload" })
        } else {
          server.config.logger.error(`Wasm build failed\n${output}`, { timestamp: true })
          server.ws.send({ type: "custom", event: "dagenda:build-error", data: output.replace(/\x1b\[[0-9;]*m/g, "") })
        }
        if (pending) {
          pending = false
          build()
        }
      })
    }

    const mtimes = new Map()
    const changed = (path) => {
      try {
        const mtime = FS.statSync(path).mtimeMs
        if (mtimes.get(path) === mtime) return false
        mtimes.set(path, mtime)
        return true
      } catch {
        return mtimes.delete(path) // deleted: counts once
      }
    }
    // Skip editor lock and backup files, such as Emacs's `.#foo.rs` and `#foo.rs#`
    const isSource = (file) => {
      const base = Path.basename(file)
      return !/^[.#]/.test(base) && (base.endsWith(".rs") || base === "Cargo.toml")
    }
    let timer
    for (const crate of ["dagenda_core", "dagenda_wasm"]) {
      const dir = Path.join(root, "rust", crate)
      for (const file of FS.readdirSync(dir, { recursive: true })) if (isSource(file)) changed(Path.join(dir, file))
      const watcher = FS.watch(dir, { recursive: true }, (_event, file) => {
        if (!file || !isSource(file) || !changed(Path.join(dir, file))) return
        clearTimeout(timer)
        timer = setTimeout(build, 150)
      })
      server.httpServer?.on("close", () => watcher.close())
    }
  },
})

export default defineConfig({
  plugins: [include(), wasm()],
  // Top-level await (the signer, finding the workspace) needs a modern target
  build: {
    target: "es2022",
    rollupOptions: {
      input: {
        collab: Path.join(here, "collab", "index.html"),
        index: Path.join(here, "index.html"),
        preview: Path.join(here, "preview", "index.html"),
      },
    },
  },
  // In dev, pre-bundling copies the Wasm packages into each dependent's
  // bundle, so their full and `slim` entries stop sharing one initialized
  // instance (a WebAssembly.Exception at startup). Left out, every import of
  // them resolves to the same module. (Their dependents stay pre-bundled:
  // they bring CommonJS dependencies.)
  optimizeDeps: {
    exclude: ["@automerge/automerge", "@automerge/subduction"],
  },
  // The Wasm rebuild reloads the page itself, once it's all written
  server: { watch: { ignored: [`${pkg}/**`] } },
})
