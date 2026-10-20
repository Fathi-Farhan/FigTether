# Integration Findings

Status: `Proposed`

This document records what the parallel implementation pass found when independently written
pieces were connected, and how each defect was closed. It exists because the interesting failures
of a project like this one do not appear inside a single module: they appear at the seams.

Every finding below was a real defect in working code, found by running the pieces against each
other, not by reading the design documents.

## 1. The plugin message path was broken in five places

The main sandbox and the UI iframe were written by two agents in parallel, and each invented its
own postMessage contract. The result was a chain that could not carry a single read from the
daemon to the editor and back.

| #   | Defect                                                                                                                                              | Consequence                                                                                   | Fix                                                                                                                               |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| A   | The sandbox emitted `bridge_result` and `bridge_error`; the UI accepted `result` and `error`                                                        | Every answer was rejected by the UI allowlist                                                 | One vocabulary, the names that mirror the Rust `BridgeFrame` message types. The sandbox still tolerates the old alias on receive. |
| B   | The UI envelope type had no `request_id` field                                                                                                      | Answers could not be correlated with their request, so the daemon could never complete a read | The envelope carries the optional request id, it is parsed and kept, and a malformed id is refused with its own reason            |
| C   | The UI never forwarded the sandbox answer to the daemon, although the socket allowlist already permitted `result`, `chunk`, `progress`, and `error` | The daemon waited forever for an answer that was never sent                                   | The controller forwards every answer with the matching frame type and the request id, and keeps the local UI update as well       |
| D   | The controller did not implement the frame handler, so a daemon `request` frame died in the UI                                                      | No read ever reached the editor                                                               | The controller forwards request frames to the sandbox and relays cancel frames                                                    |
| E   | The UI validated the raw postMessage payload instead of unwrapping the `pluginMessage` wrapper Figma adds                                           | Every sandbox answer was discarded before validation                                          | The wrapper is unwrapped before validation; the wrapped and the raw shape are both accepted                                       |

The repair is protected by `plugins/figma/ui/test/message-path.test.ts`, which drives the real
sandbox runtime and the real UI controller together and fails if the unwrap or the request id
propagation is removed.

## 2. The same contract was declared twice, which is what caused finding 1

The channel constant, the postMessage envelope type, the message vocabulary, and the
`pluginMessage` unwrapper each existed in two packages. The shared package now owns all four, and
a guard test fails when the channel literal appears more than once in the tree, when the two
vocabulary lists disagree, or when a package repeats a list.

Lesson for the process, and it is now a rule in the contribution guide: a contract that crosses a
package boundary is declared exactly once, in the package that owns it.

## 3. The benchmarks measured a copy of the core

The benchmark crate was written while the core crate was still a set of stubs, so it carried its
own `MetadataCache`, `BenchScheduler`, `BenchAssetStore`, and `normalize_document`. The bench
targets passed, the report looked right, and the numbers described an implementation that no
user would ever run. The bench crate now exercises the shipped `figtether-core` types, and a test
fails if the benchmark ever stops using them.

This is the failure mode the PRD warns about in section 2.2: a benchmark that measures anything
other than the product is not evidence. The rule that is now enforced by a test: **a benchmark
must import the code it measures**.

## 4. The shipped daemon could not complete a single read

The acceptance tests that were rewired onto the shipped daemon failed, and their failure message
listed the rules the daemon broke. Every one of them was invisible in the unit tests of the
daemon, because each unit exercised one function in isolation.

| Rule | Defect                                                                         | Consequence                                                                                                           |
| ---- | ------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------- |
| 1    | The session was registered but never left `UNPAIRED`                           | The grant was refused with `INVALID_LIMIT` and every read was refused with `SESSION_NOT_FOUND`                        |
| 2    | `plan_of` mapped every read to `JobKind::Read`, whose operation is `ReadNodes` | A `list_nodes` request reached the plugin as `read_nodes`, and the answer was an empty node list                      |
| 3    | `normalize_result` hardcoded `truncated: false` and `next_cursor: None`        | Pagination could never work, and a truncated traversal looked complete                                                |
| 4    | `run_job` never called `Cache::put` nor `AssetStore::put`                      | The cache and the asset store stayed empty, so a revoke had nothing to purge                                          |
| 5    | A granted job was never completed                                              | The single operation slot of the session stayed occupied forever, and every later operation of that session timed out |

Rule 5 was found by the benchmark rewiring, not by the acceptance tests: the shipped scheduler
enforces one active operation per session, and the benchmark had to complete every job by hand to
avoid the timeouts. The daemon now completes every job on every exit path.

The general lesson: the daemon was well tested function by function and untested as a path. The
tests that caught these drive the real daemon with a real client and a real plugin connection.

## 5. A local client could not see the sessions of its own plugin

`figtether revoke --all` reported nothing, and `figtether status` showed no sessions, because
`list_sessions` is scoped by client id while the plugin session carried the client id of the
plugin UI. The tempting fix, a privileged "list every session" call, would have broken the PRD
rule that a client only sees the sessions granted to it.

The model that is implemented instead: a session belongs to the local installation that granted
it. The pairing secret carries its owner, the plugin session is attributed to that owner, and the
local clients resolve the same owner from the credential in the runtime file. A client with a
wrong credential still sees nothing and is still refused.

## 6. The doctor could not report capabilities

The daemon status carried versions, counts, and sizes but no capability report, so
`figtether doctor` downgraded the capability check to a warning. The daemon does know the
capabilities: the plugin reports them and the bridge stores them per session. The status now
carries them, sanitized to names, states, and reasons.

## 7. What the findings say about the process

1. **Contracts first, in one place.** The Rust `model` and `protocol` crates were written and
   frozen before any parallel work started, and the TypeScript side mirrors them exactly. Almost
   every cross-crate defect in this list was a violation of that discipline, not a lack of it.
2. **A green suite is not a working product.** The plugin, the benchmarks, and the acceptance
   harness each passed their own tests while the system could not complete a single read end to
   end. The tests that caught this are integration tests that drive the real components.
3. **Duplicate implementations are worse than missing ones.** The benchmark copy was the most
   dangerous artifact of the whole pass, because it produced plausible numbers for code that
   nobody ships.
4. **Parallel work needs an integration owner.** Every finding above crossed a boundary that no
   single author could see. The integration pass is not a formality; it is where the product
   actually comes together.
5. **A unit tested function is not a working path.** The daemon passed 64 unit tests and still
   could not complete a read. Path tests that drive the real components are what found rule 1 to
   rule 5, and the fix for rule 5 came from the benchmark side.
