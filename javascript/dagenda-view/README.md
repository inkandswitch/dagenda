# `@inkandswitch/dagenda-view`

`<dagenda-view>`, a web component that draws a roadmap's tech tree from the dagenda notation. All the logic (parsing, resolving, layout, and drawing) is the Rust core compiled to Wasm; `dagenda-view.js` only wires the SVG's `data-*` hooks to the page.

> [!NOTE]
> Not published yet (`"private": true`). The [preview](../preview/) (`nix run .#preview`) imports it from here, so it always runs this exact component.

## Build

```sh
npm run build  # wasm-pack → pkg/ (needs wasm-pack and the pinned wasm-bindgen-cli; both in the Nix dev shell)
```

## Use

```html
<script type="module" src="/path/to/dagenda-view.js"></script>

<dagenda-view><script type="text/dagenda">
  roadmap
  [x] task parser [Parser]
  [ ] task resolver [Resolver]
    needs parser
</script></dagenda-view>

<dagenda-view src="./plan.dagenda" theme="dark"></dagenda-view>
```

The notation's indentation is relative, so inline source can be indented to match the page.

| Attribute | Effect                                                      |
|-----------|-------------------------------------------------------------|
| `src`     | Load the source from a URL                                  |
| `theme`   | `light` or `dark`, overriding the system colour scheme      |
| `zoom`    | Scale factor, such as `1.5`                                 |

| Property   | Meaning                                                                 |
|------------|-------------------------------------------------------------------------|
| `source`   | The plan's text; setting it redraws                                     |
| `problems` | The current source's problems: `{ line, column, severity, message }`    |
| `selected` | The selected node's id, or `null`                                       |
| `svg`      | The drawn SVG's markup, for export                                      |
| `select(id)` | Select a node (`null` clears)                                         |

| Event            | When                                | Detail              |
|------------------|-------------------------------------|---------------------|
| `dagenda-render` | After every change of source        | `{ problems, ok }`  |
| `dagenda-select` | A node was selected, or cleared     | `{ id }`            |

Hover a node to light up everything it needs and everything waiting on it, all the way up and down. Click to select; click again or press Escape to clear. A card's pull requests, issues, and links open in a new tab, without selecting the card. Text that doesn't parse keeps the last good picture up, so the tree doesn't flicker while someone types.
