# Integration regression notes

The first integration pass found failures that component-level suites did not cover. This page
keeps the lessons and the regression-test locations, rather than a transcript of the build.

## The bridge port could not be bound on this Windows machine

Windows reserves TCP port ranges for the hypervisor (Hyper-V, WSL, Docker Desktop) and reserves
them again at every boot. On this machine the range 38929-39028 was reserved, so the default
bridge port 38947 could not be bound even though nothing listened on it. The bind failed with
`WSAEACCES` (os error 10013), not with "address in use", and the daemon surfaced the raw operating
system error:

```text
error: the bridge port 38947 on 127.0.0.1 could not be bound: An attempt was made to access a
socket in a way forbidden by its access permissions. (os error 10013)
```

Three things were wrong with that answer. It named an operating system error instead of the
cause, because a port inside a reserved range is unusable by design rather than by accident. It
offered no next step, although the user has to move the port, the plugin manifest, and the plugin
build together, because the plugin reaches the daemon over the address its manifest allows. And it
was not stable: the reserved ranges are re-reserved at boot, so the default port can stop working
on a machine where it worked yesterday, with no change to the project.

The repair separates the diagnosis from the fix. The daemon names the reserved range and suggests a
free port, `figtether doctor` reports the same diagnosis for the port the daemon is expected to
use, and `figtether manifest --port auto` picks a port outside every reserved range and writes it
into the manifest. The port is still never moved silently: the manifest and the daemon have to
agree, and the user is told the exact commands to run.

The lesson is the one the write path taught again: a refusal is only useful when it names the
cause and the next action. A raw operating system error is a defect in the product, not in the
machine.

## Plugin relay

The sandbox and iframe initially disagreed on result/error names, dropped the request ID, and
handled Figma's `pluginMessage` wrapper differently. The controller also updated its own UI
without forwarding replies to the daemon.

The shared contract now defines the channel, envelope, and message vocabulary once.
`plugins/figma/ui/test/message-path.test.ts` drives the actual runtime and controller together;
`shared/test/bridge.test.ts` guards duplicated channel/vocabulary definitions. This proves the
relay under simulated editor input, not loading or networking inside Figma Desktop.

The later [desktop report](../../benchmarks/published/desktop-2026-10-20.md) separately records
real-editor loading, routing, pairing, scoped reads, approved native creation, and export metadata.
The UI build inlines scripts, styles, and its icon; source-relative assets are not served by
Figma's HTML-string iframe. These desktop observations do not expand the simulated test's scope.

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
