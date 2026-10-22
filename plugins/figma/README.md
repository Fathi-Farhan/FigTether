# Figma plugin

The plugin reads, creates, and edits native document nodes through Figma's Plugin API. Its sandbox does not open sockets;
the UI iframe relays messages to the Rust daemon. The shared package owns the wire contract.

This is a development plugin. Automated tests use a fake editor API. A limited Windows/Figma
Desktop 126.10.10 evaluation verified loading, pairing, scoped reads, approved writes, and
export metadata. Other platforms and seat-specific capabilities remain unverified. Start with
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

The build uses `manifest.local.json` when present, otherwise the checked-in `manifest.json`.
`node build.mjs --manifest PATH` selects a different manifest explicitly. Both the bundle's
port and copied manifest id come from that file. A conflicting `FIGTETHER_PORT` is rejected.
`npm run build:watch` rebuilds during development; restart it when the manifest changes.
`npm run format` uses the repository's
Prettier settings. `npm run lint` covers shared/tooling files only; use `lint:all` before a PR.

| Path                      | Purpose                                                             |
| ------------------------- | ------------------------------------------------------------------- |
| [`main/`](main/README.md) | Scoped read handlers, native write execution, traversal, and export |
| [`ui/`](ui/README.md)     | Pairing, grant controls, status, WebSocket relay                    |
| `shared/src/`             | Frames, postMessage envelope, capabilities, encoding, hashing       |
| `build.mjs`               | Bundles `main/src/main.ts`, `ui/src/ui.ts`, and the UI document     |
| `dist/`                   | Local build output; ignored by Git                                  |

## Import into Figma

1. Install dependencies with `npm ci` above.
2. Create a development plugin in Figma to obtain your own plugin ID.
3. From the repository root, prepare the manifest, build, and start the daemon:

   ```sh
   figtether start --plugin-id YOUR_PLUGIN_ID
   ```

4. Import the printed `dist/manifest.json` in Figma's development plugin menu. Paste the secret
   from that same terminal to pair, then grant a small selected frame.

For manual preparation, generate the manifest first and run `npm run build` afterward.
Start `figtether daemon` and mint a secret with `figtether pair` in another terminal.

Do not overwrite the checked-in example just to insert your account's ID. A plugin ID is not
a secret, but local IDs don't belong in the shared manifest. The CLI manifest uses
`documentAccess: dynamic-page`, Figma Design only, and explicit development loopback addresses.
The checked-in example is not a Community/production manifest.

The UI connects to `ws://localhost:PORT/bridge`; the daemon binds `127.0.0.1:PORT`.
The manifest selects `PORT` at build time. After changing it, rebuild and reimport the
manifest into Figma. `figtether start --port PORT` generates and rebuilds both sides.

## Build output and limits

The build writes JavaScript, CSS, source maps, `ui.html`, and a copied development manifest to
`dist/`. Source-map files can contain local source details; review release artifacts separately
from the source repository. The generated manifest in `dist/` rewrites paths relative to that
directory. Figma requires the imported filename to be exactly `manifest.json`; the local
`manifest.local.json` is the build input and cannot be imported directly.

The build embeds JavaScript, CSS, and the small local icon into `ui.html`, because the Figma
iframe does not serve sibling asset URLs. This packaging was loaded in the recorded desktop
test; a successful build alone does not establish support on other editors or seats.
Snapshot UI controls, large transfers, and asset-resource delivery do not yet make complete
daemon workflows. Read the [current limitations](../../docs/10-implementation/01-implementation-status.md).

## Contract changes

```ts
import { PROTOCOL_VERSION, type BridgeFrame } from "@figtether/shared";
```

TypeScript, esbuild, and Vitest resolve that import to `shared/src`. Update the Rust protocol
and this shared definition together. Do not redeclare message names or the postMessage envelope
in the sandbox/UI; drift tests guard that boundary. See [CONTRIBUTING.md](../../CONTRIBUTING.md).
