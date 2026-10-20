# Integration regression notes

The first integration pass found failures that component-level suites did not cover. This page
keeps the lessons and the regression-test locations, rather than a transcript of the build.

## Plugin relay

The sandbox and iframe initially disagreed on result/error names, dropped the request ID, and
handled Figma's `pluginMessage` wrapper differently. The controller also updated its own UI
without forwarding replies to the daemon.

The shared contract now defines the channel, envelope, and message vocabulary once.
`plugins/figma/ui/test/message-path.test.ts` drives the actual runtime and controller together;
`shared/test/bridge.test.ts` guards duplicated channel/vocabulary definitions. This proves the
relay under simulated editor input, not loading or networking inside Figma Desktop.

## Benchmark implementation

Early workloads carried local cache, scheduler, normalizer, and asset-store copies. Their timing
did not describe the shipped core. The copies were removed; the workloads import the core types.
`crates/bench/tests/shipped_core.rs` checks those identities and reads a cache entry written
through the real core.

Always benchmark the implementation being shipped. Keep editor work marked unmeasured when no
editor is involved; see the [benchmark guide](../../benchmarks/README.md).

## Daemon read path

Shipped-service tests found five missing links:

| Failure                                          | Fix                                                                |
| ------------------------------------------------ | ------------------------------------------------------------------ |
| Sessions stayed unpaired after auth              | Advance the session lifecycle after auth and a grant.              |
| Different read tools collapsed to `read_nodes`   | Carry the intended operation in the daemon job ledger.             |
| Coverage discarded truncation/cursor information | Retain page coverage and bind opaque cursors to the grant/context. |
| Reads/exports did not populate stores            | Write cache and asset entries before serving their references.     |
| Unfinished jobs held scheduler lanes             | Complete jobs on every terminal path, with a drop backstop.        |

`crates/daemon/tests/read_path.rs` and the shipped-service scenarios in
`crates/e2e/tests/acceptance.rs` cover those paths. Other acceptance scenarios use a reference
daemon to simulate faults. They must not be counted as proof of shipped-service features.

## Local installation ownership

The CLI and plugin initially used different client-ID strings as discovery keys, so the CLI
could not list/revoke the plugin's sessions. Ownership now resolves from the installation
credential. Client labels do not authorize a request. Regression tests live in
`crates/daemon/tests/session_ownership.rs`; this model does not isolate hostile processes running
with the same user's privileges.

## Adversarial checks

`crates/e2e/tests/adversarial.rs` initially exposed browser-accessible native control endpoints,
missing daemon-side scope admission, and unchecked announced transfer sizes. The native header
guard, scope admission, and transfer-size rejection now have regression coverage.

These are specific fixes, not a conclusion that the service is secure. Fresh scope changes,
Windows credential permissions, full transfers, and editor behavior still need review. The
[implementation status](01-implementation-status.md) and [security policy](../../SECURITY.md)
record the remaining limits.
