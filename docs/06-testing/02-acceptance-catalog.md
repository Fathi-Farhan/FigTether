# FigTether Acceptance Test Catalog (AT01..AT20)

Status: `Proposed`

Purpose: one section per PRD Appendix A row, preserving the PRD scenario text and gate mapping
verbatim in meaning. Each section adds preconditions, numbered steps, evidence requirements, pass
criteria, and requirement links; no wording weakens the PRD and no ID is renumbered or remapped.
Execution organization (pyramid, environments, fixtures, evidence storage) is
`docs/06-testing/01-test-strategy.md`; benchmark procedures for `N01..N10` are
`docs/06-testing/04-benchmark-plan.md`, referenced but never duplicated here.

## How to read

- `<CLI>`, `<client>`, `<session>` are placeholders expanded in the evidence `commands` field. CLI subcommands (`doctor`, `daemon`, `pair`, `config print`, `status`, `revoke`, `mcp --stdio`) come from PRD 5.1 and PRD 8.4; the daemon default port is `38947`, loopback only.
- Fixtures `FX-SMALL` (100 nodes), `FX-MEDIUM` (1000), `FX-LARGE` (10000 nodes in scope), `FX-CROSSPAGE` (big cross page document for lazy loading), `FX-MIXED` (attribute checklist), and `FX-ADVERSARIAL` are defined in `docs/06-testing/01-test-strategy.md` section 4; every run records `fixture_hash`.
- Numeric limits (batch 50 IDs, depth 32, response 64 KiB, queue 64/16, export 16 MP) are quoted from `docs/04-data-contracts/06-resource-limits.md`; PRD 8.5.5 values prevail on conflict.
- A scenario listed at two gates (for example `G0/G2`) is evidenced at the earlier gate and re-evidenced on the release candidate; both records are kept (strategy rule 5.2.7).
- Evidence template (PRD Appendix C) mandatory for every block: `commit`, `artifact_sha256`, `schema/protocol_version`, `OS/CPU/RAM`, `Figma_version`, `plugin_manifest_hash`, `account_plan/seat_context`, `fixture_hash`, `test_id`, `commands`, `expected/actual`, `duration/latency/RSS`, `network_path_evidence`, `known_limitations`, `reviewer`, `tested_at`. Each section names the fields carrying scenario specific content on top of this baseline.

## AT01 - Core path uses no official MCP or REST endpoints

**Purpose.** Agent calls plugin backed tools without any official MCP connection; the path is proven not to use official MCP or REST endpoints. **Preconditions.** Artifact built from an exact commit; `FX-MIXED` paired; deny egress guard instrumentable around `crates/daemon` and `crates/mcp`.

**Steps.**

1. Build from the pinned commit; record `commit`, `artifact_sha256`, `schema/protocol_version` (FR19; G0).
2. Start `<CLI> daemon` with an OS deny rule blocking non-loopback destinations for core processes; pair on `FX-MIXED` root `1:2` (FR19; G0).
3. From `<client>` call `get_bridge_status`, `list_nodes`, `get_nodes`, `inspect_design`, `export_assets` on the granted root; all return plugin path data (FR19; G0).
4. Capture `network_path_evidence`: per process connection log plus source review of `crates/*` at the same commit proving no Figma REST client and no official MCP adapter (FR19; G0, repeated at G2 from the release artifact).
5. Record that no PAT, token, or quota metric appears anywhere in the tool chain (FR19; G0/G2).

**Evidence required.** Baseline plus `network_path_evidence` (process attributed; Figma editor connections listed separately per PRD Appendix A note), `commands` (full tool call list), `expected/actual` (zero prohibited endpoints), `known_limitations` (probe gaps).

**Pass criteria.** All tools succeed through the plugin path; zero core process connections to `api.figma.com`, Figma REST paths, or any official MCP endpoint; `G01` trace holds at the exact commit. **Requirements.** FR19; P01/P14. **Gate.** G0/G2.

## AT02 - No PAT dependency and loopback only core egress

**Purpose.** `FIGMA_TOKEN` not set; the core still reads nodes; core egress is blocked except loopback. **Preconditions.** Same artifact as AT01; `FX-SMALL` loaded; credential variables absent; deny rule allowing loopback only.

**Steps.**

1. Unset every Figma credential variable, restart the daemon, and confirm startup succeeds with no token prompt and no REST fallback attempt (FR19; G0).
2. Execute a fresh read on `FX-SMALL` nodes (`consistency=fresh`); verify payloads match the fixture revision and fresh reads reach the plugin with observation timestamps (FR06; G0).
3. Verify the egress deny log shows zero non-loopback attempts from core processes during the run (FR19; G0).
4. Retry with a bogus `FIGMA_TOKEN` present; verify the core still ignores credentials and no authorized Figma REST call is attempted (FR19; supports P02).

**Evidence required.** Baseline plus environment capture proving token absence, `network_path_evidence` (deny log plus process capture), `fixture_hash`, `duration/latency/RSS` of the fresh read.

**Pass criteria.** Node reads succeed with zero credentials; no non-loopback egress from the core; no silent REST fallback exists in the path. **Requirements.** FR06; FR19; P02. **Gate.** G0.

## AT03 - New user install, pair, grant, and read on two OSes

**Purpose.** A new user installs binary plus plugin, pairs, grants, and reads on two operating systems without a Rust or Node toolchain. **Preconditions.** Published artifact and plugin bundle with checksums; Windows x64 and macOS (arm64 or x64) with Figma Desktop; `FX-SMALL` opened; first time users.

**Steps.**

1. Download the release binary, plugin bundle, and checksum; verify `artifact_sha256` before running anything (FR01; G2).
2. Run `<CLI> doctor`; confirm version, runtime directory, port, and manifest checks pass with no toolchain installed (FR01; G2).
3. Start the daemon, generate the development manifest with the user plugin ID, open the plugin in Figma Desktop, and enter the pairing secret from `<CLI> pair`; verify session name and fingerprint (FR02; G2).
4. Select root `1:2` on `FX-SMALL` and grant; call `list_nodes` and `get_nodes` from `<client>` and complete one fresh read plus one snapshot read (FR03, FR04, FR17; G2).
5. Repeat all steps from scratch on the second OS and record per step durations and any step needing more than the PRD 5.1 guided flow (FR01, FR02, FR17; G2).

**Evidence required.** Baseline plus `account_plan/seat_context` per OS, per step timings inside `duration/latency/RSS`, `Figma_version` per OS, `plugin_manifest_hash`, `known_limitations` (assisted steps).

**Pass criteria.** Both OS sequences complete without a toolchain install; every read carries the FR12 envelope; failures are recorded per step, not silently skipped. **Requirements.** FR01; FR02; FR17; P09. **Gate.** G2.

## AT04 - Fresh read tracks edits while snapshots stay stable

**Purpose.** Change text and layout; the fresh read reflects it; the old snapshot stays unchanged; cached results carry their age. **Preconditions.** Paired session on `FX-MIXED` root `1:2`; a captured snapshot exists; editor edit rights; artifact from an exact commit.

**Steps.**

1. Perform a fresh read of a text node and a layout node (`consistency=fresh`); record `observed_at`, `content_hash`, `freshness`, and `coverage` (FR06, FR12; G1).
2. Capture a snapshot via plugin UI and read the same nodes through `snapshot_id`; record values and `consistency=best_effort` (FR05; G1).
3. In the editor change the label text and a padding value; repeat the identical fresh read; verify new values and a new `observed_at` with `freshness=fresh_read` (FR06; G1).
4. Read the old snapshot again; verify values unchanged and `snapshot_id` plus `content_hash` stable, labeled snapshot never claiming live (FR05; G1).
5. Request `consistency=cached` with `max_age_ms`; verify `cache_age_ms` is present and that overage forces a fresh read or a structured failure (FR12; G1).
6. Edit once during a traversal; verify `DOCUMENT_CHANGED` or a warning with at most one retry (PRD 8.3; FR12, FR14; G1).

**Evidence required.** Baseline plus `expected/actual` per step with timestamp deltas and hashes, `Figma_version`, sanitized logs, `duration/latency/RSS` of fresh reads.

**Pass criteria.** Fresh reads never serve stale data; snapshots are immutable with stable hashes; cache labeling is explicit; edit during read follows PRD 8.3. **Requirements.** FR05; FR06; FR12; P03. **Gate.** G1.

## AT05 - Two sessions over identical node IDs stay isolated

**Purpose.** Two sessions using identical node IDs exchange no data, cursors, or assets. **Preconditions.** Two sessions S1 and S2 (same file via two tabs, or two files with coincidentally identical node IDs); one client; distinct grants.

**Steps.**

1. Pair S1 and S2; grant distinct roots in each; verify `list_sessions` shows only sessions granted to this client, each with its own `session_id` and capabilities (FR04; G1).
2. Read the same node ID `1:2` from both sessions; verify each response carries the matching `session_id`, `grant_epoch`, and session bound `observation_id` (FR04; G1).
3. Use a cursor produced in S1 against S2; verify `CURSOR_EXPIRED` or a structured rejection; repeat in the reverse direction (FR04; G1).
4. Request an asset whose `asset_id` was minted in S1 while attached as S2; verify `PERMISSION_DENIED` or `OUT_OF_SCOPE` with no asset bytes served (FR03; G1).
5. Revoke the grant of S1; verify S1 requests now fail with `REVOKED` while S2 keeps serving its own scope with no cache cross hit (FR03; G1).

**Evidence required.** Baseline plus `test_id` annotated per session, `expected/actual` per step, `commands` including both session flows, sanitized logs showing separated cache keys.

**Pass criteria.** No data, cursor, or asset crosses the session boundary; every read stays inside its own session grant; document labels never act as routing keys. **Requirements.** FR03; FR04; P06. **Gate.** G1.

## AT06 - Negative authentication, transport, scope, and revoke mid transfer

**Purpose.** Wrong token, expired token, replay, wrong Host or Origin, out of scope node, revoke during transfer: each yields a structured result and leaks nothing. **Preconditions.** Frame crafting harness; `FX-MIXED` session; revoke observer; ability to corrupt Host or Origin headers.

**Steps.**

1. Pair with a wrong secret; verify the connection closes inside the 3 s auth window and no design data, session metadata, or capability info precedes auth (FR02; G1).
2. Let a valid secret age past the 5 min TTL and retry; verify `AUTH_EXPIRED` and that re-pairing is required (FR02; G1).
3. Pair successfully, then replay the consumed secret in a new connection; verify one time use rejection (FR02; G1).
4. Open a socket with wrong `Host` or `Origin` values; verify the daemon rejects with a logged reason and serves nothing (FR02; G1).
5. Request a node outside the granted root; verify a structured `OUT_OF_SCOPE` or `PERMISSION_DENIED` result containing no sibling or ancestor data (FR03; G1).
6. Start an asset transfer and invoke revoke mid transfer; verify bytes stop within the N07 logical window, no further chunks are emitted, and already delivered bytes are not counted as new leakage (FR15; G1 desktop capture; G2 re-run).

**Evidence required.** Baseline plus per case `expected/actual`, `network_path_evidence` for transport refusals, `reviewer` signature from the security reviewer role, `known_limitations` for residual risks named in `docs/05-security/01-threat-model.md`.

**Pass criteria.** Every negative case returns a structured error with `code`, `message`, `retryable`, `action`, and `request_id`; no crash; no data leak; no silent fallback. **Requirements.** FR02; FR03; FR15; P10. **Gate.** G1/G2.

**Adversarial counterpart.** The deterministic form of these cases lives in
`crates/e2e/tests/adversarial.rs`, and the same threat classes are attacked by an autonomous agent
in the PentAGI assessment described in `docs/05-security/04-pentagi-validation.md`. Step 4 has one
case that the deterministic suite adds beyond this catalog: a request that carries an `Origin`
header, including `Origin: null`, must not reach the control endpoints that mint a pairing secret
or stop the daemon.

## AT07 - Pagination, truncation, and no leakage on oversized scopes

**Purpose.** A fixture exceeding bytes and depth limits yields correct partial results with a resumable cursor; sibling or ancestor information outside scope never leaks. **Preconditions.** `FX-LARGE` (10000 nodes in scope) and `FX-CROSSPAGE` loaded; limits pinned in the artifact per `docs/04-data-contracts/06-resource-limits.md`.

**Steps.**

1. Call `list_nodes` on the `FX-LARGE` root; verify `truncated` plus `next_cursor` appear when limits are reached and each response stays within the 64 KiB encoded budget (FR07; G1).
2. Walk every page to completion; verify each cursor resumes exactly where the last page stopped and the aggregate node set matches the fixture revision (FR07, FR12; G1).
3. Request `depth` greater than 32 or a batch over 50 node IDs; verify `INVALID_LIMIT` and no oversized execution (FR07, FR13; G1).
4. Drive a traversal beyond the 10000 node per operation cap; verify `partial` with a cursor when continuation is possible, otherwise `SCOPE_TOO_LARGE` (FR07; G1).
5. On `FX-CROSSPAGE`, page across pages while a node referenced by a later page is deleted; verify remaining pages stay valid and the deletion surfaces as a per item error, not silent loss (FR07, FR12; G1).
6. Inspect every response for sibling or ancestor data outside the grant; none may be present (FR07; G1).

**Evidence required.** Baseline plus page by page `expected/actual` node counts, `duration/latency/RSS` for the walk, `fixture_hash` for `FX-LARGE` and `FX-CROSSPAGE`, sanitized logs.

**Pass criteria.** Truncation is never hidden; cursors resume exactly; scope leakage is zero; every limit produces a structured error. **Requirements.** FR07; FR12; P04. **Gate.** G1.

## AT08 - Queue overload, resource limits, mixed soak, cancellation

**Purpose.** Queue overload, slow requests, memory limits, cancellation, and a mixed soak all stay inside the resource contract. **Preconditions.** Two clients; soak driver; metrics enabled (FR18); `FX-MEDIUM` sessions; one real editor session included; RSS sampled at 1 s.

**Steps.**

1. Drive requests past the queue caps (64 total, 16 per client); verify excess requests receive `BUSY` with `retry_after_ms` and nothing is silently dropped (FR13; G2).
2. Issue a request whose deadline expires while queued; verify `TIMEOUT`, no late execution, and no late delivery (FR14; G2).
3. Raise memory pressure until cache eviction triggers; verify LRU eviction and that a single asset above 16 MiB is rejected with a structured error (FR13, FR14; G2).
4. Run the 60 min mixed soak with 2 clients, 4 mock sessions, and at least one real editor session; verify no crash, no OOM, unexpected error rate below 0.1 percent, and no editor freeze above 1 s from our traversal (FR13, FR14; PRD 2.1 N04/N05/N06; G2).
5. Cancel a long running read mid flight; verify cancellation propagates to the scheduler and the late result is discarded (FR14; G2).
6. Verify one active editor operation per session and one global export slot throughout soak, with revoke, cancel, and heartbeat prioritized (FR13; G2).

**Evidence required.** Baseline plus `duration/latency/RSS` (1 s RSS series), `expected/actual` per cap, soak report under `docs/07-ops/release-evidence/`, benchmark records per `docs/06-testing/04-benchmark-plan.md`.

**Pass criteria.** Backpressure honors stated limits; soak is clean per N04 and N05; cancellation lands within the logical window; memory stays bounded. **Requirements.** FR13; FR14; P05. **Gate.** G2.

## AT09 - Connection disturbance: plugin reopen, sleep wake, daemon restart, stale epoch

**Purpose.** Plugin close and reopen, sleep and wake, daemon restart, and delayed frames from an old connection epoch produce no wrong data. **Preconditions.** Paired READY session; instrumentation able to suppress or delay plugin frames; OS sleep and network controls; `FX-SMALL` loaded.

**Steps.**

1. Close the plugin while READY; verify offline detection within the N08 window (15 s) and that fresh reads then fail with `PLUGIN_OFFLINE` instead of serving cache as fresh (FR16; G1).
2. Reopen the plugin and re-pair; verify a new session identity runs while old session reads fail with `SESSION_NOT_FOUND` (FR16; G1).
3. Sleep the machine with an active session, wake it; verify offline detection within 15 s and reconnect readiness within 10 s once transport recovers (FR16; N08; G1 desktop).
4. Restart the daemon while a client holds an old credential; verify the new `daemon_instance_id` invalidates it, stale runtime files are detected and not trusted, and pairing is required again (FR16; G2).
5. Release a suppressed plugin result carrying the old `connection_epoch`; verify the frame is discarded and never merged into the new epoch (FR16; G2).
6. Kill the transport mid traversal and reconnect; verify no blind replay of the request set, only bounded resumption or an explicit restart decision (FR16; G2).

**Evidence required.** Baseline plus per OS timing tables inside `duration/latency/RSS`, `Figma_version`, `expected/actual` per disturbance, `network_path_evidence` for reconnects.

**Pass criteria.** Offline and reconnect stay inside the N08 windows; old epoch data never surfaces as valid; recovery requires explicit state transitions, not silent substitution. **Requirements.** FR16; P11. **Gate.** G1/G2.

## AT10 - Normalization fidelity against curated fixtures

**Purpose.** Normalized JSON matches curated fixture expectations; mixed text, gradients, and unresolved aliases survive; nothing is silently lost or invented. **Preconditions.** `FX-MIXED` carrying the full attribute checklist (nested layout, mixed text, variants, aliases/modes, gradients, masks, image fills, SVG, missing nodes, hidden/locked, non Latin text, unknown types); curated expectations present.

**Steps.**

1. Call `inspect_design`, `get_nodes`, and `get_design_tokens` on the `FX-MIXED` root; verify every attribute produces its normalized field or an explicit unavailable or warning entry (FR08, FR09; G1).
2. Diff each response against `fixtures/expected/FX-MIXED.json`; any mismatch is a defect or a recorded `known_limitations` entry, never an accepted variance (FR08, FR10; G1).
3. Verify mixed styled text returns as runs, gradients keep stop order, unresolved aliases are reported with a reason, and unknown node types survive as `unsupported` (FR08, FR10; G1).
4. Verify availability semantics: null only where availability says `available`; unknown values carry an unavailable reason instead of null (FR09; G1).
5. Repeat steps 1 to 4 in Figma Desktop for the artifact under test; record `Figma_version`; a desktop versus CI mismatch blocks the gate (FR08; G2).

**Evidence required.** Baseline plus a diff summary table per attribute, `fixture_hash`, `expected/actual` per attribute row, sanitized logs, `duration/latency/RSS` of the reads.

**Pass criteria.** No field the API exposes is absent or silently replaced; unavailable features surface as structured reasons; the curated comparison is exact. **Requirements.** FR08; FR09; FR10; P08/P12. **Gate.** G1/G2.

## AT11 - Asset export fidelity and limits

**Purpose.** PNG and SVG match the fixture; MIME and hash are correct; oversize assets and active SVG content are handled safely. **Preconditions.** `FX-MIXED` contains PNG eligible nodes, SVG eligible nodes, an oversize image target, and an SVG with active content; desktop available; asset store bound to the grant.

**Steps.**

1. Call `export_assets` for PNG and SVG targets at fixture scales; verify per item `asset_id`, MIME, dimensions, byte count, and content hash match `fixtures/expected/FX-MIXED.json` and the `fixtures/README.md` descriptor (FR11; G1).
2. Fetch the resource behind each `asset_id` via the grant bound resource URI; verify bytes match and the URI is rejected from any other session or grant (FR11, FR03; G1).
3. Request an export past the 16 megapixel cap or scale outside 0.25 to 2; verify `INVALID_LIMIT` and no export attempt (FR11; PRD 8.5.5; G1).
4. Request the active content SVG from `FX-ADVERSARIAL`; verify it is sanitized or rejected, never executed as HTML, and any filesystem export stays inside the user chosen directory (FR11, PRD 8.6.1; G1).
5. Revoke the grant, then attempt the asset URI again; verify it fails instead of serving bytes (FR11, FR15; G2).

**Evidence required.** Baseline plus per asset `expected/actual` (hash, bytes, MIME), `duration/latency/RSS` of exports, `fixture_hash`, sanitized logs, `known_limitations` for renderer differences declared by Figma.

**Pass criteria.** Assets reproduce the fixture reference for fixture targets; limits are enforced; active SVG is neutralized; grant bound access survives revoke checks. **Requirements.** FR11; P07. **Gate.** G1/G2.

## AT12 - Capability probing across eligible and restricted contexts

**Purpose.** An eligible Starter Design context works end to end; a seat or permission context that rejects the plugin is reported honestly. **Preconditions.** Login contexts representing (a) Starter Design eligible and (b) a seat or organization policy rejecting plugin run; no private data; isolation from other tests.

**Steps.**

1. Under the eligible context pair and read; verify capability discovery reports plugin usability through `get_bridge_status` and `list_sessions` (FR09; G0).
2. Under the restricted context attempt pairing and reads; verify a structured denial or `CAPABILITY_UNAVAILABLE` with reason, with no crash and no inferred eligibility (FR09; G0).
3. Probe features one by one (variables, library references, styles, watch); verify each unavailable API yields a structured reason, and null is never treated as absent (FR09, FR10; G0).
4. Verify session identity comes from session identifiers, never from account names, `fileKey`, or seat class inference (FR09, FR04; G0).
5. Re-run the matrix at release candidate stage on Windows and macOS including a restricted seat context; publish the support matrix for release docs (FR09; G2).

**Evidence required.** Baseline plus `account_plan/seat_context` per run, the capability table inside `expected/actual`, `reviewer` signature from the plugin owner role, `known_limitations` for seat based feature gaps.

**Pass criteria.** Capability and availability statements match runtime reality per context with no inference from names or account tiers; denied contexts return structured results. **Requirements.** FR09; P13. **Gate.** G0/G2.

## AT13 - Fuzz and property testing of parsers and path resolution

**Purpose.** Recursion, numeric extremes, invalid IDs, unknown enums, malformed chunks, and zip bombs (P1) are survived without crash or unbounded allocation. **Preconditions.** Fuzz corpus checked in under `fixtures/fuzz/`; property suite compiled; allocator limits instrumented on the artifact.

**Steps.**

1. Run the fuzz corpus against the bridge frame parser and DTO parser in `crates/protocol`; verify no crash and no unbounded memory (FR07, FR13; G1).
2. Add corpus entries for each PRD scenario class: recursion, numeric extremes (integer boundaries, negative sizes, NaN dimensions), invalid node IDs (wrong separators, oversized strings), unknown enum variants, malformed chunk sequences and checksums (FR07, FR13; G1).
3. Run property tests over path resolution and scope membership: deep paths, overlapping grants, epoch changes mid walk, alias graph cycles, `CURSOR_EXPIRED` after epoch bump (FR03, FR07; G1).
4. Run a chunk assembly fuzz: sequence gaps, oversize frames, mid transfer deadline; verify bounded reassembly and clean discard of incomplete transfers (FR13; G1).
5. At G3, run the offline bundle parser fuzz including decompression bomb payloads; verify rejection with `PAYLOAD_TOO_LARGE` and no extraction spike (FR07, FR13, FR21; G3).

**Evidence required.** Baseline plus corpus revision ID, sanitizer or crash reports (or their absence), `expected/actual` per payload class, `known_limitations` for discovered but unfixed parser gaps.

**Pass criteria.** Zero crashes on the corpus; extreme inputs resolve to structured errors; allocations stay bounded per the limits contract. **Requirements.** FR07; FR13; P04/P10. **Gate.** G1/G3.

## AT14 - MCP stdio lifecycle on two clients

**Purpose.** MCP initialize, tools, resources, and cancellation work; stdout carries protocol only on at least two clients. **Preconditions.** `<CLI> mcp --stdio` ready; two independent MCP clients (for example one generic harness and one desktop harness); paired session on `FX-SMALL`.

**Steps.**

1. Run `initialize` from client A and client B against the pinned SDK protocol version; record negotiated capabilities per client (FR17; PRD 8.4; G2).
2. Enumerate tools and resources; verify the tool set matches the frozen PRD 8.5.1 surface and every read tool carries `readOnlyHint=true` and `destructiveHint=false` (FR17; G2).
3. Execute `list_nodes` and `get_nodes` through both clients; verify envelopes carry `schema_version`, source, observation, coverage, and warnings (FR07, FR12; G2).
4. Start a long export and send a client cancellation; verify the cancellation reaches the scheduler and no late result is delivered (FR17, FR14; G2).
5. Inspect stdout of both clients; verify zero non-protocol bytes and structured diagnostics on stderr only (FR17, FR18; G2).

**Evidence required.** Baseline plus client identity and version inside `commands`, per client `expected/actual` for the handshake and tool surface, sanitized stderr excerpts, `known_limitations` for client capability gaps.

**Pass criteria.** Full lifecycle passes on both clients; tool names match the frozen contract; stdout is protocol clean; cancellation works end to end. **Requirements.** FR17; P09. **Gate.** G2.

## AT15 - Revoke cleanup and no replay after reconnect

**Purpose.** Revoke clears cache and assets and blocks retries; nothing replays after reconnect. **Preconditions.** READY session with warm cache and at least one exported asset; reconnect and job instrumentation available.

**Steps.**

1. Read nodes and export an asset on the granted root to populate cache, asset store, and cursors (FR13, FR15; G1).
2. Invoke revoke from the plugin UI and separately from `<CLI> revoke` while the plugin is closed; verify grant, cache entries, asset store entries, and cursors for that grant are invalidated (FR15; G1/G2).
3. Replay a previously successful read and the asset resource fetch; verify both return `REVOKED` or a structured denial with no payload (FR15; G1).
4. Reconnect after revoke; verify no pending or queued job replays, no stale fragments survive, and requests require newly granted state (FR14, FR16; G2).
5. Pause then attempt reads with `PAUSED` surfaced, resume, then revoke; verify pause gates new reads only, while revoke removes existing results (FR15; G1).

**Evidence required.** Baseline plus `expected/actual` per step, sanitized diffs of cache and asset store state, revoke timing inside `duration/latency/RSS`, `network_path_evidence` for reconnect behavior.

**Pass criteria.** Nothing previously readable survives as an authoritative artifact after revoke; no replay occurs; pause and revoke remain distinct states. **Requirements.** FR14; FR15; FR16; P10/P11. **Gate.** G1/G2.

## AT16 - Claim language in public materials

**Purpose.** README and public pages make no unlimited claim and no all features free claim; measured limits are stated. **Preconditions.** Candidate public artifacts (README, website copy, release notes) under review; benchmark records for `N01..N10` exist per `docs/06-testing/04-benchmark-plan.md` where numbers are cited.

**Steps.**

1. Grep and manually read README, website, release notes, and claim bearing docs for wording that promises unlimited capacity, all features free, or full Figma capability replacement; none may be present (G01, G02 anchors; P14; G2).
2. Verify every cited number points to a record containing `commit`, environment, and `Figma_version`; verify no claim uses a metric without its record (P14; G2).
3. Verify the support matrix and capability boundaries state their gate level; features not in v0.1 are not hidden behind vague wording (FR09; G2).
4. Verify Starter eligibility, seat limits, and paths that do not consume official quota are described exactly as PRD 2.1 and 8.11 define them, with the non-MCP path claim only (FR19; G2).

**Evidence required.** Baseline (OS fields may be empty when `known_limitations` marks this as a documentation review), reviewed file paths and diff marks inside `expected/actual`, `reviewer` signature from the product maintainer role, `tested_at`.

**Pass criteria.** No unlimited or overpromising language survives the review; every number has a record; limitations and boundaries are explicit. **Requirements.** G01; G02; P14. **Gate.** G2.

## AT17 - Scope changes during active reads

**Purpose.** Scope moved, deleted, or renamed during a read: membership checks stay fresh, stale results are rejected when invalid, and selection never widens scope. **Preconditions.** Paired session on `FX-MEDIUM` with a grant on root `1:2`; editorial control over hierarchy and selection.

**Steps.**

1. Start a paginated `list_nodes` traversal over the granted subtree; verify the first pages succeed (FR07; G1).
2. Move a node out of the granted subtree mid traversal; verify it disappears from subsequent pages, `coverage` reflects the removal, and the node is no longer readable once out (FR03, FR06; G1).
3. Rename the granted root mid traversal; verify reads continue because a rename is not a scope change and `grant_epoch` stays stable (FR03, FR24; G1).
4. Delete the granted root mid traversal; verify subsequent requests return a structured `NODE_NOT_FOUND` or `OUT_OF_SCOPE` and scope bound resources stop resolving (FR03, FR06; G1).
5. Change editor selection during reads; verify selection alone never broadens scope or exposes new nodes (FR03, FR24; G1).
6. Race a fresh membership check against payload delivery so that the grant epoch changes mid read; verify the response is either rejected or re-evidenced with a fresh membership pass, never served uncritically (FR03, FR06; G1).

**Evidence required.** Baseline plus per step `expected/actual` including scope and epoch state, membership check timing inside `duration/latency/RSS`, `fixture_hash`, sanitized logs.

**Pass criteria.** Membership checks stay fresh; out of scope or stale content is never served; selection never widens grant; the document is never mutated. **Requirements.** FR03; FR06; FR24. **Gate.** G1.

## AT18 - No document mutation from read or export flows

**Purpose.** Before and after fixture state is unchanged after reads and exports; selection and page never change as a side effect. **Preconditions.** Clean copies of `FX-SMALL` and `FX-MIXED`; snapshot capture available; instrumentation to read document state, selection, and current page.

**Steps.**

1. Record document state (node ordering, text values, selection, current page) and the fixture content hash before any call (FR24; G0).
2. Execute a full pass: `list_nodes`, `get_nodes` over the whole grant, `inspect_design`, `get_design_tokens`, `export_assets` for fixture targets, plus one snapshot capture and read (FR24; G0).
3. Compare before and after states; verify document hash, node order, selection, and page are identical and no edit request exists anywhere in the trace (FR24; G0).
4. In Figma Desktop, observe the editor session across the pass; record any editor side effect and attribute it to the editor with evidence if it occurs (FR24; G2).
5. Review the tool schema at the same commit to confirm no write operation is exposed; source inspection supports, while the runtime state comparison proves (FR24; G2).

**Evidence required.** Baseline plus before and after state tables inside `expected/actual`, `Figma_version`, `fixture_hash`, sanitized trace logs, `known_limitations` for editor driven changes.

**Pass criteria.** Document state, selection, and page are unchanged across the full read and export pass; the used tool surface contains no write operation. **Requirements.** FR24. **Gate.** G0/G2.

## AT19 - Watch event gaps, dependencies, overflow, and resync

**Purpose.** Watch signal gaps, component or style dependency changes, journal overflow, and resync all produce honest states (watch is P1 and invalidation only). **Preconditions.** P1 feature flag for `watch_scope` and `get_changes` (FR20, PRD 8.5.1); session on `FX-MIXED`; ability to suppress or reorder events; journal limits from PRD 8.3.

**Steps.**

1. Enable `watch_scope` on a small subtree; apply edits and deletes; verify `get_changes` returns changed IDs and deleted IDs as tombstones rather than full payloads (FR20; G3).
2. Suppress one event batch to force a gap; verify the next `get_changes` reports `RESYNC_REQUIRED` and the journal never claims a clean sync (FR20; G3).
3. Change a component or style dependency outside the granted subtree; verify the coverage statement directs the reader to a fresh read instead of a silent merge (FR20; G3).
4. Exceed journal limits (10000 events or 5 minutes) or the 10 batch per second cap; verify overflow is reported clearly and resync is offered with no payload loss claims (FR20; PRD 8.3; G3).
5. Disable watch at runtime; verify live fresh reads remain fully available and the client never mistakes watch absence for the absence of change (FR20, FR06; G3).

**Evidence required.** Baseline plus journal snapshots, `expected/actual` per disturbance class, sanitized logs, `known_limitations` for multi edit races and partial event coverage.

**Pass criteria.** No silent loss of change or over-claim of synchronization; resync is always offered honestly; fresh reads stay available when watch is absent. **Requirements.** FR20. **Gate.** G3.

## AT20 - Offline bundle integrity and source honesty

**Purpose.** Bundle hash, schema, and size validation works; offline source is clearly labeled and never implicitly routed to live paths (bundle is P1). **Preconditions.** Offline bundle export (FR21) available on the artifact; a bundle with manifest, hash, and schema files; ability to tamper bytes.

**Steps.**

1. Export an offline bundle for a snapshot with manifest, hash, schema version, and size limits; verify hashes and content match the release asset (FR21; G3).
2. Import the bundle offline; verify the source `kind` reads `offline_bundle` in the envelope and no live session or plugin path is open (FR21; G3).
3. Tamper one byte inside the bundle; verify import refuses with a structured integrity error and no plugin data is returned (FR21; G3).
4. Pass an exported bundle into queries that normally expect a live session; verify a structured error and no implicit live routing (FR21; G3).
5. Attempt bundle import with an oversize or bomb style payload; verify bounded decompression and a structured rejection (FR21, PRD 8.5.5; G3).

**Evidence required.** Baseline plus bundle manifest hash inside `expected/actual`, per step `expected/actual`, sanitized logs, `known_limitations` for unsupported bundle features.

**Pass criteria.** Bundle integrity is verifiable by hash and schema; offline reads stay offline; no implicit live routing drift; oversize handling is bounded. **Requirements.** FR21. **Gate.** G3.

## Closing notes

The template baseline in How to read is mandatory for every scenario; `docs/06-testing/01-test-strategy.md` section 5 fixes sanitized log rules, evidence storage, and rerun rules. `docs/06-testing/03-traceability-matrix.md` maps each FR and each PRD problem `P01..P14` to the scenarios covering them; `docs/06-testing/04-benchmark-plan.md` owns the `N01..N10` measurement procedures. No scenario is renumbered, weakened against its PRD Appendix A row, or moved to a gate the PRD does not assign.
