# Figma plugin

The plugin reads the document through Figma's Plugin API. Its sandbox does not open sockets;
the UI iframe relays messages to the Rust daemon. The shared package owns the wire contract.

This is a development plugin. Automated tests use a fake editor API; Figma Desktop loading,
network policy, and seat-specific capabilities still need a manual validation pass. Start with
the [quickstart](../../docs/quickstart.md) and a synthetic file.

## Build and test

From this directory, with Node.js 22.14 or newer:

```sh
npm ci
npm run build:types
npm run typecheck
npm run lint:all
npm test
npm run build
```

`npm run build:watch` rebuilds during development. `npm run format` uses the repository's
Prettier settings. `npm run lint` covers shared/tooling files only; use `lint:all` before a PR.

| Path | Purpose |
| --- | --- |
| [`main/`](main/README.md) | Allowlisted read handlers, traversal, scope checks, export |
| [`ui/`](ui/README.md) | Pairing, grant controls, status, WebSocket relay |
| `shared/src/` | Frames, postMessage envelope, capabilities, encoding, hashing |
| `build.mjs` | Bundles `main/src/main.ts`, `ui/src/ui.ts`, and the UI document |
| `dist/` | Local build output; ignored by Git |

## Import into Figma

1. Build the plugin above.
2. Create a development plugin in Figma to obtain your own plugin ID.
3. From the repository root, generate a local manifest:

   ```sh
   figtether manifest --plugin-id YOUR_PLUGIN_ID --out plugins/figma/manifest.local.json
   ```

4. Import that local manifest in Figma's development plugin menu. Start `figtether daemon` and
   pair using `figtether pair`, then grant a small selected frame.

Do not overwrite the checked-in example just to insert your account's ID. A plugin ID is not
a secret, but local IDs don't belong in the shared manifest. The CLI manifest uses
`documentAccess: dynamic-page`, Figma Design only, and explicit development loopback addresses.
The checked-in example is not a Community/production manifest.

The UI currently uses `127.0.0.1:38947`. Regenerating a manifest with a different port does not
change that UI constant. Keep the default for now.

## Build output and limits

The build writes JavaScript, CSS, source maps, `ui.html`, and a copied development manifest to
`dist/`. Source-map files can contain local source details; review release artifacts separately
from the source repository. The generated manifest in `dist/` rewrites paths relative to that
directory. The local manifest above instead references `dist/` from its parent directory.

Editor packaging is not yet validated. In particular, script/style loading inside Figma's UI
iframe needs a real-editor test; a successful esbuild run alone does not prove the plugin loads.
Snapshot UI controls, large transfers, and asset-resource delivery do not yet make complete
daemon workflows. Read the [current limitations](../../docs/10-implementation/01-implementation-status.md).

## Contract changes

```ts
import { PROTOCOL_VERSION, type BridgeFrame } from "@figtether/shared";
```

TypeScript, esbuild, and Vitest resolve that import to `shared/src`. Update the Rust protocol
and this shared definition together. Do not redeclare message names or the postMessage envelope
in the sandbox/UI; drift tests guard that boundary. See [CONTRIBUTING.md](../../CONTRIBUTING.md).
