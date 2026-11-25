# The preview as a static site (for GitHub Pages): a release Wasm build, the
# component, and the preview page, with `/` sending you to the preview, plus
# the collaborative editor at `collab/`. Writes `_site/` (or the directory
# given)
root="${WORKSPACE_ROOT:-$PWD}"
out="$(realpath -m "${1:-_site}")"
case "$out" in
  / | "$root" | "$HOME") echo "refusing to replace $out" >&2; exit 1 ;;
esac
set -x
rm -rf "$out"
mkdir -p "$out/preview" "$out/dagenda-view"
wasm-pack build --release --target web --no-pack \
  --out-dir "$out/dagenda-view/pkg" "$root/rust/dagenda_wasm"
rm -f "$out/dagenda-view/pkg/.gitignore"
cp "$root/javascript/dagenda-view/dagenda-view.js" "$root/javascript/dagenda-view/editing.js" "$out/dagenda-view/"
for file in index.html examples.js highlight.js indent.js main.js shell.js style.css; do
  cp "$root/javascript/preview/$file" "$out/preview/"
done
# The editor bundles the component, so it takes the same release build. A
# relative base serves it from any path (/dagenda/collab/)
rm -rf "$root/javascript/dagenda-view/pkg"
cp -r "$out/dagenda-view/pkg" "$root/javascript/dagenda-view/"
(cd "$root" && pnpm install --frozen-lockfile)
(cd "$root/javascript/editor" && node node_modules/vite/bin/vite.js build --base ./ --outDir "$out/collab" --emptyOutDir)
cat > "$out/index.html" <<'HTML'
<!doctype html>
<meta charset="utf-8">
<title>dagenda</title>
<meta http-equiv="refresh" content="0; url=preview/">
<a href="preview/">Open the dagenda preview</a>
HTML
touch "$out/.nojekyll"
