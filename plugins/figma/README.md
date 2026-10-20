# FigTether Figma plugin

The Figma side of FigTether: the main sandbox that reads the document, the UI iframe that owns
the loopback socket to the daemon, and the shared package that mirrors the Rust wire contract.

The plugin never mutates the document (FR24), never opens a socket from the sandbox, and never
stores a credential outside memory (PRD 8.5.4, PRD 8.6.2).

## Layout

```text
plugins/figma/
  manifest.json        development manifest, imported into Figma Desktop
  build.mjs            esbuild build, writes dist/
  tsconfig.json        solution: shared compiler options and project references
  typecheck.mjs        typecheck driver, one run per project
  shared/              the shared wire contract, published as @figtether/shared
    src/               protocol, plugin payloads, capability states, helpers
    test/              unit tests for the helpers and the frame round trip
  main/                the Figma main sandbox (owned by another work stream)
  ui/                  the UI iframe (owned by another work stream)
  dist/                build output, not committed
```

## Commands

Run every command from this directory.

| Command               | What it does                                                               |
| --------------------- | -------------------------------------------------------------------------- |
| `npm install`         | installs the tool chain and writes `package-lock.json`                     |
| `npm run build`       | bundles the plugin into `dist/`                                            |
| `npm run build:watch` | rebuilds on every change, use it while working on the sandbox or the UI    |
| `npm run typecheck`   | typechecks the shared package and every project that is present            |
| `npm run build:types` | emits the declarations of the shared package into `shared/dist`            |
| `npm run lint`        | runs ESLint over the shared package and the tooling files                  |
| `npm run lint:all`    | runs ESLint over the whole plugin, including the other work streams        |
| `npm test`            | runs the unit tests with Vitest                                            |
| `npm run format`      | formats the package with the Prettier configuration of the repository root |

Node.js LTS is the target. The build needs nothing but the tool chain in `package.json`.

## The shared package

`plugins/figma/shared` is the TypeScript mirror of `crates/protocol`. It is imported through the
path mapping of the root `tsconfig.json`:

```ts
import { PROTOCOL_VERSION, type BridgeFrame } from "@figtether/shared";
```

esbuild and Vitest resolve the same path, so the editor, the bundle and the tests always read the
same files. The package uses no browser API and no Node.js API, because the main sandbox has
neither.

What it exports:

| Group             | Exports                                                                                                                                                                                                                                                  |
| ----------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Bridge frames     | `BridgeFrame`, `MessageType`, `BridgeOp`, `MESSAGE_TYPES`, `BRIDGE_OPS`, `isMessageType`, `isBridgeOp`, `PROTOCOL_VERSION`, `MAX_FRAME_BYTES`, `MAX_TRANSFER_BYTES`, `AUTH_WINDOW_MS`, `HEARTBEAT_INTERVAL_MS`, `MISSED_HEARTBEATS_BEFORE_OFFLINE`       |
| Frame payloads    | `AuthRequest`, `AuthAccepted`, `AuthRejected`, `RequestPayload`, `ResultPayload`, `ErrorPayload`, `HeartbeatPayload`, `GrantUpdatePayload`, `RevokePayload`, `ChunkPayload`, `CapabilitiesPayload`                                                       |
| Plugin payloads   | `PluginNode`, `PluginGeometry`, `PluginLayout`, `PluginText`, `PluginTextRun`, `PluginPaint`, `PluginGradientStop`, `PluginEffect`, `PluginComponent`, `PluginVariantProperty`, `PluginVariable`, `PluginDocument`, `PluginExport`, `PluginCapabilities` |
| Capability states | `CapabilityState`, `CapabilityMap`, `capabilityAvailable`, `capabilityUnavailable`, `capabilityUnsupported`, `isCapabilityState`, `isCapabilityAvailable`, `capabilityReason`                                                                            |
| Defaults          | `createPluginNode`, `createPluginGeometry`, `createPluginLayout`, `createPluginText`, `createPluginPaint`, `createPluginEffect`, `createPluginVariable`, `createPluginDocument`                                                                          |
| Frame helpers     | `createFrame`, `serializeFrame`, `parseFrame`, `readPayload`, `createChunk`, `BridgeContractError`                                                                                                                                                       |
| Encoding helpers  | `base64Encode`, `base64Decode`, `base64EncodeText`, `base64DecodeText`, `canonicalStringify`, `parseJsonValue`, `assertJsonValue`, `sha256`, `sha256Hex`, `sha256HexOfText`, `utf8Encode`, `utf8Decode`                                                  |
| JSON types        | `JsonValue`, `JsonObject`, `isJsonObject`, `isJsonArray`                                                                                                                                                                                                 |

Rules that the shared package follows, taken from the Rust definitions:

- a Rust `Option<T>` with `skip_serializing_if = "Option::is_none"` becomes an optional TypeScript
  property, never a property that may hold `undefined`;
- a Rust field with `#[serde(default)]` stays required in TypeScript, because the Rust serializer
  always writes it. The `create*` helpers fill those fields;
- `ChunkPayload.bytes` and `PluginExport.bytes` are base64 strings on the wire, exactly like the
  Rust `base64_bytes` module;
- the capability state is the internally tagged enum of `crates/model/src/capabilities.rs`, so a
  capability reads `{ status: "available" }`, `{ status: "unavailable", reason: "..." }` or
  `{ status: "unsupported", reason: "..." }`;
- `canonicalStringify` sorts object keys, so both sides of the bridge can hash the same payload.

## TypeScript projects

The root `tsconfig.json` is a solution. It carries the compiler options that every project shares
and it references the three projects:

| Project | Configuration          | Notes                                            |
| ------- | ---------------------- | ------------------------------------------------ |
| shared  | `shared/tsconfig.json` | composite, emits declarations into `shared/dist` |
| main    | `main/tsconfig.json`   | `lib: ["ES2020"]`, no DOM                        |
| ui      | `ui/tsconfig.json`     | `lib: ["ES2020", "DOM", "DOM.Iterable"]`         |

A project that is referenced by the solution has to set `composite: true` and to list its own
`include`. The main folder has no browser API, so it needs the Figma globals through the
typings package:

```jsonc
{
  "extends": "../tsconfig.json",
  "compilerOptions": {
    "composite": true,
    "noEmit": false,
    "lib": ["ES2020"],
    "types": ["@figma/plugin-typings"],
  },
  "include": ["src/**/*"],
  "references": [{ "path": "../shared/tsconfig.json" }],
}
```

`@figma/plugin-typings` declares `figma`, `console`, `setTimeout`, `clearTimeout` and `fetch`.
Without it the main sandbox has no `console` at all, because the base library does not declare one.

The root sets `exactOptionalPropertyTypes: true`. A project that cannot live with it overrides the
flag in its own configuration, the UI project does that already.

Two more projects belong to this work stream:

| Project | Configuration               | Notes                                                       |
| ------- | --------------------------- | ----------------------------------------------------------- |
| tooling | `tsconfig.tools.json`       | the Vitest configuration, the only place with Node.js types |
| tests   | `shared/test/tsconfig.json` | the shared package together with its tests                  |

`npm run typecheck` runs `typecheck.mjs`, which typechecks every project configuration that is
present and skips the ones that are not there yet. It never fails because a project of another work
stream is missing.

The shared package keeps its own copy of the compiler options instead of extending the root, so the
contract typechecks and the tests run even when the main and UI projects are not present yet.

## Build output

`npm run build` writes four files into `dist/`:

| File                 | Content                                                        |
| -------------------- | -------------------------------------------------------------- |
| `dist/main.js`       | the bundled main sandbox, one self contained script            |
| `dist/ui.js`         | the bundled UI iframe script                                   |
| `dist/ui.html`       | the UI document, its script tag points at `./ui.js`            |
| `dist/manifest.json` | the development manifest with paths that resolve inside `dist` |

The build fails with an explicit message when a source entry is missing, and it never writes a
partial bundle. `node build.mjs --watch` rebuilds on every change.

## Manifest strategy

`manifest.json` in this folder is the **development** manifest. It is the file that is imported
into Figma Desktop with "Import plugin from manifest", and it points at the build output.

```jsonc
{
  "name": "FigTether Bridge",
  "id": "figtether-dev", // placeholder, replace it with your own plugin id
  "api": "1.0.0",
  "main": "dist/main.js",
  "ui": "dist/ui.html",
  "editorType": ["figma"],
  "documentAccess": "dynamic-page",
  "networkAccess": {
    "allowedDomains": ["http://127.0.0.1"],
    "reason": "The plugin talks to the FigTether daemon on the local loopback interface only.",
  },
}
```

The plugin id `figtether-dev` is a **placeholder**. A plugin id belongs to the account that
publishes the plugin, so every contributor replaces it with their own id before importing the
manifest. The id is not a secret and it carries no permission.

<!--
  Production manifest, decision D11

  The manifest in this folder is a development artifact. A manifest for the
  Figma Community is a separate, reviewed artifact and it is out of scope for
  v0.1 (PRD decision D11, "Community plugin publication: out of v0.1").

  Two things follow from that and both matter:

  1. "devAllowedDomains" on its own is not a production configuration. The
     development allowlist only covers the loopback bridge of a local daemon.
     A published plugin needs a reviewed manifest with a justified network
     access reason for every domain it may reach, and a published plugin may
     not be limited to a loopback daemon that only exists on the author's
     machine.
  2. The build artifact in "dist" carries the development manifest only. It
     must not be shipped as a release bundle, and the release process has to
     produce and review the production manifest separately
     (docs/07-ops/04-release-process.md).

  Until D11 closes, the distribution path is the development manifest with the
  plugin id of the contributor who built it
  (docs/07-ops/01-dev-environment.md, section 4).
-->

The manifest asks for `documentAccess: "dynamic-page"`, never `load-all-pages`, and the network
allowlist holds the loopback address only, never a wildcard domain (PRD 8.6.1, S5).

## Importing the plugin into Figma Desktop

1. `npm install` and `npm run build` in this directory.
2. Open Figma Desktop, then "Plugins > Development > Import plugin from manifest".
3. Choose `plugins/figma/manifest.json`.
4. Replace the placeholder id with your own plugin id before the import if you already have one.

## Boundaries

- The main sandbox never opens a socket and never stores a credential.
- The UI iframe never touches the document and never writes design data to a persistent store.
- The shared package never imports from `main` or `ui`, so both sides can use it.
