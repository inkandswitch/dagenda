# Collaboration (javascript/editor). On its own, the collaborative editor's
# dev server: it installs the JS dependencies if needed, builds the
# component's Wasm, and serves the editor with Vite at http://localhost:5173
# (PORT to change); the preview's Collaborate button reaches it through
# `nix run .#preview` (EDITOR_PORT there). With a command, a shared plan from
# the command line, as files in a directory (cli/collab.mjs): `pull`, `push`,
# `undo`, `status`, `diff`, or `new`, directories relative to where you run it
root="${WORKSPACE_ROOT:-$PWD}"
if [ ! -f "$root/javascript/editor/index.html" ]; then
  echo "run this from the dagenda repo root" >&2
  exit 1
fi
[ -d "$root/javascript/editor/node_modules" ] || (cd "$root" && pnpm install --frozen-lockfile >&2)
wasm() {
  (cd "$root" && wasm-pack --log-level warn build --dev --target web --out-dir ../../javascript/dagenda-view/pkg rust/dagenda_wasm >&2)
}
case "${1:-}" in
  "")
    wasm
    cd "$root/javascript/editor"
    # Through node, not the .bin shim: its #!/bin/sh is not everywhere
    exec node node_modules/vite/bin/vite.js --port "${PORT:-5173}" --strictPort
    ;;
  # These check the plan with this repo's parser first
  push | new) wasm ;;
esac
exec node "$root/javascript/editor/cli/collab.mjs" "$@"
