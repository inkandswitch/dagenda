# Hot-reloading preview: rebuilds the Wasm when Rust sources change and
# reloads the page (see javascript/preview/serve.mjs)
root="${WORKSPACE_ROOT:-$PWD}"
if [ ! -f "$root/javascript/preview/serve.mjs" ]; then
  echo "run this from the dagenda repo root" >&2
  exit 1
fi

exec node "$root/javascript/preview/serve.mjs"
