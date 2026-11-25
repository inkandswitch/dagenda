# Both pages as a static site (for GitHub Pages): the preview at `preview/`,
# with `/` sending you there, and the collaborative editor at `collab/`, on a
# release Wasm build. Writes `_site/` (or the directory given)
root="${WORKSPACE_ROOT:-$PWD}"
out="$(realpath -m "${1:-_site}")"
case "$out" in
  / | "$root" | "$HOME") echo "refusing to replace $out" >&2; exit 1 ;;
esac
set -x
rm -rf "$out"
(cd "$root" && wasm-pack build --release --target web --no-pack --out-dir ../../javascript/dagenda-view/pkg rust/dagenda_wasm)
(cd "$root" && pnpm install --frozen-lockfile)
# A relative base serves it from any path (/dagenda/)
(cd "$root/javascript/editor" && node node_modules/vite/bin/vite.js build --base ./ --outDir "$out" --emptyOutDir)
touch "$out/.nojekyll"
