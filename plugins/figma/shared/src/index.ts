/**
 * Shared wire contract for the FigTether Figma plugin.
 *
 * This package is the TypeScript mirror of the Rust contract in
 * `crates/protocol`. The main sandbox and the UI iframe both import it so the
 * two sides of the bridge cannot drift apart silently.
 *
 * It also carries the contract of the postMessage channel between the two
 * sides of the plugin: the channel, the envelope, the vocabulary of the
 * message kinds and the unwrapping of the messages of the editor.
 *
 * Nothing in this package uses a browser API or a Node.js API: the Figma main
 * sandbox has neither.
 */
export * from "./json";
export * from "./capabilities";
export * from "./protocol";
export * from "./plugin";
export * from "./defaults";
export * from "./base64";
export * from "./canonical";
export * from "./sha256";
export * from "./utf8";
export * from "./frame";
export * from "./bridge";
