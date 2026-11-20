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
  [x] parser [Parser]
  [ ] resolver [Resolver]
    needs parser
</script></dagenda-view>

<dagenda-view src="./plan.dagenda" theme="dark"></dagenda-view>
```

The notation's indentation is relative, so inline source can be indented to match the page.

| Attribute | Effect                                                      |
|-----------|-------------------------------------------------------------|
| `src`     | Load the source from a URL                                  |
| `theme`   | `light` or `dark`, overriding the system colour scheme      |
| `zoom`    | Scale factor, such as `1.5`; a change keeps the point under the pointer still (or, with the pointer elsewhere, the middle of the view) |
| `editable` | Let the picture edit the plan (below)                      |

| Property   | Meaning                                                                 |
|------------|-------------------------------------------------------------------------|
| `source`   | The plan's text, for a plan of one document; setting it redraws         |
| `documents` | A plan of several documents, `[{ name, source }]`; setting it redraws from all of them |
| `problems` | The plan's problems: `{ document, line, column, severity, message }`    |
| `selected` | The selected node's id, or `null`                                       |
| `person`   | The person picked, as in `"@alice"`, or `null`; setting it picks them (`null` shows everything) |
| `svg`      | The drawn SVG's markup, for export                                      |
| `select(id)` | Select a node (`null` clears)                                         |

| Event            | When                                | Detail              |
|------------------|-------------------------------------|---------------------|
| `dagenda-render` | After every change of source        | `{ problems, ok }`  |
| `dagenda-select` | A node was selected, or cleared     | `{ id }`            |
| `dagenda-person` | A person was picked, or cleared     | `{ handle }`        |
| `dagenda-edit`   | An edit was made on the picture     | `{ kind, edits }`: each changed document's index, new `source`, and the `base` text it was made from |

## Editing the picture

With `editable`:

| Gesture | Edit |
|---|---|
| Click a card's badge | A menu of the eight status boxes; picking one rewrites the task's box |
| Drag from a badge onto a card | "This needs that": a `needs` in the task's block |
| Drag a card into a milestone's band | Re-scope it: off the milestone whose band it was in, onto that one's must-haves (the top layer: none) |
| Drag a milestone (its gate, or its dashed line) | Move its boundary: up past cards brings them in (onto its must-haves); down past cards hands them to the era above (or to no milestone). Past another milestone's line, reorder the two: its `milestone` block moves |

The picture never changes the text itself. It asks the Wasm for the edit and fires `dagenda-edit` with each changed document's new text; the page applies the difference as the smallest splice, so it joins that person's undo and merges with typing (the preview and the editor both do). An edit that makes no sense (a loop, a milestone below one it builds on, two milestones in different documents) shows its reason on the picture, where the gesture ended, instead. Cards a milestone drag would move light up while it's held. There's no keyboard path for the drags yet; the text is.

## Looking around

Hover a node to light up everything it needs and everything waiting on it, all the way up and down. Click to select; click again or press Escape to clear. A card's pull requests, issues, and links open in a new tab, without selecting the card. Click someone's sticker, or their `@name` on a card, to light up everything they're assigned to, the way hovering lights up a chain: their cards (outlined, and at full strength even if locked), the lines between them, and the banners of tracks they run, with everything else faded and their stickers ringed. It stays, whatever's hovered, until you click them again, click the background, or press Escape. Text that doesn't parse keeps the last good picture up, so the tree doesn't flicker while someone types.
