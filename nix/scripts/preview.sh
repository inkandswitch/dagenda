# Both pages, hot-reloading (javascript/editor, through Vite): the preview at
# /preview/ and the collaborative editor at /collab/. It installs the JS
# dependencies if needed and builds the component's Wasm, which it rebuilds
# when the Rust changes (see javascript/editor/vite.config.js)
root="${WORKSPACE_ROOT:-$PWD}"
if [ ! -f "$root/javascript/editor/vite.config.js" ]; then
  echo "run this from the dagenda repo root" >&2
  exit 1
fi
[ -d "$root/javascript/editor/node_modules" ] || (cd "$root" && pnpm install --frozen-lockfile >&2)
(cd "$root" && wasm-pack --log-level warn build --dev --target web --out-dir ../../javascript/dagenda-view/pkg rust/dagenda_wasm >&2)
port="${PORT:-8080}"
echo "the preview: http://localhost:$port/preview/ · the collaborative editor: http://localhost:$port/collab/" >&2
cd "$root/javascript/editor"
# Through node, not the .bin shim: its #!/bin/sh is not everywhere
exec node node_modules/vite/bin/vite.js --port "$port" --strictPort
