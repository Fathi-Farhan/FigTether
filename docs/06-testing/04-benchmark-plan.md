# Benchmark Plan (N01..N10)

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

Status: `Proposed`

Purpose. This plan fixes how every nonfunctional target N01..N10 is measured, narrated and
published, so that no number silently travels from PRD target to marketing result. Targets come
from PRD section 2.1 and 2.2 and from docs/00-overview/01-prd-analysis.md section 6; acceptance
scenarios they feed live in docs/06-testing/02-acceptance-catalog.md; binding traceability lives in
docs/06-testing/03-traceability-matrix.md. Until a report following section 3 exists, every threshold
in this document remains a target and never a measured result. Evidence is filed in the release
process slots defined by docs/07-ops/04-release-process.md.

## 1. Ground rules

### 1.1 Pre-run freeze checklist

Each row is frozen before any run starts. Changing a frozen item mid-run invalidates the whole run;
the rerun starts from scratch and the invalidated run is never deleted.

| Frozen item         | Rule                                                                                                                                                              | Recorded as                                        |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| Fixture set         | fixtures pinned to a commit, content identical across the run, non-private and clearly licensed (PRD 2.1); no redraw during a run                                 | fixture manifest plus one fixture_hash per fixture |
| Figma version       | exact Figma Desktop version recorded; plugin runs in normal runtime without Developer VM [S11]                                                                    | figma_version field in the report                  |
| Account and seat    | plan/seat eligibility recorded without personal identity; a quota or eligibility failure is reported as an access limitation, never an accuracy failure (PRD 2.2) | account_plan_seat_context field                    |
| Device profile      | 4 logical cores, 8 GB RAM; idle desktop, fixed power plan, no thermal trades during the run                                                                       | os, cpu, ram fields                                |
| Build               | daemon built in release mode; tested artifact taken from the distribution artifact, not the source tree                                                           | artifact_sha256                                    |
| Configuration       | released defaults; limits per docs/04-data-contracts/06-resource-limits.md; every override recorded; cache TTL configuration for cached profiles recorded         | config digest inside commands                      |
| Success definitions | pass thresholds, denominators and failure taxonomy fixed before the run; post-hoc threshold edits invalidate the run                                              | expected vs actual fields                          |

### 1.2 Run discipline

| Rule                              | Detail                                                                                                                                                                                                                    |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Profile every run                 | every run produces p50/p95/p99 plus CPU percent, RSS sampling at 1 s (N04 cadence), and a profiling artifact (trace or flamegraph export) attached to the summary.                                                        |
| Exact commit published            | core and plugin commits plus artifact hashes are printed in the report; a number without a commit is not publishable.                                                                                                     |
| Workload class separation         | cached, fresh, snapshot and export workloads are measured and reported as separate classes and are never mixed into one statistical base; see 2.0.                                                                        |
| Cache hits                        | a cache hit never counts toward fresh-read success or a fresh denominator; hits appear only in the cached class and the hit-rate line.                                                                                    |
| Task order                        | order is randomized or balanced across fixtures, sizes and operations per run; operator skill and manual interventions are logged.                                                                                        |
| Negative and adversarial controls | every report includes controls: out-of-scope node, malformed ID, oversized payload, stale cursor, revoked grant, offline injection, malformed frame. Controls are reported but never folded into positive denominators.   |
| Review                            | an independent reviewer re-derives sampled results against the fixture and against changes deliberately introduced in a mutated copy; product status text (for example READY) is not accepted as evidence of correctness. |
| Task accounting                   | blocked, timeout, unsupported and cancelled tasks are reported with the denominator (total, completed, failed, plus each failure class). Failed tasks are never removed; a rerun supersedes but never erases.             |
| Cold start                        | the first run after process start is recorded under a cold-start key and is excluded from steady-state p95 claims, but is always visible in the report.                                                                   |
| Editor attribution                | connection capture attributes only FigTether processes and code paths; the editor itself uses the network independently, so captures never claim to describe all Figma traffic.                                           |

### 1.3 Failure taxonomy

Expected errors are pre-declared from PRD 8.7 (BUSY, PAUSED, PLUGIN_OFFLINE, TIMEOUT, CANCELLED,
PAYLOAD_TOO_LARGE, SCOPE_TOO_LARGE, INVALID_LIMIT, CURSOR_EXPIRED, DOCUMENT_CHANGED,
CAPABILITY_UNAVAILABLE, NODE_NOT_FOUND, AUTH_EXPIRED). Everything else is unexpected; N05 passes only
when unexpected errors stay under 0.1 percent of classified operations on the valid workload.

## 2. Workloads

### 2.0 Profiles and stage decomposition

| Profile             | Basis                                                               | Feeds                             | Runtime                                                       |
| ------------------- | ------------------------------------------------------------------- | --------------------------------- | ------------------------------------------------------------- |
| W1 cached metadata  | 10,000 cached reads, sustained target >= 20 req/s                   | N01                               | one daemon; real plugin or mock harness; real editor optional |
| W2 fresh sequential | 1,000 sequential fresh reads within 60 min, all reaching the plugin | N02; fresh share of N05           | real editor session mandatory                                 |
| W3 export           | 100 PNG exports 1024x1024 with an isolated queue                    | N03                               | real editor mandatory                                         |
| W4 mixed soak       | 60 min, 2 MCP clients, 4 mock sessions, >= 1 real-editor session    | N04 peak, N05, FR22 soak evidence | mixed classes; stats always split per class                   |
| W5 pilot            | >= 10 pilot users, first plus repeat onboarding                     | N10, AT03 evidence                | release bundle artifacts on Windows and macOS                 |

Statistical honesty note: W4 mixes operation types for stability evidence only. It is never the
evidence base for N01..N03 p95 claims; those claims require their dedicated profiles W1..W3.

Timing decomposition, recorded for every request in every profile:

| Stage            | Definition                                                       | Recorded                                               |
| ---------------- | ---------------------------------------------------------------- | ------------------------------------------------------ |
| queue_ms         | enqueue to scheduled start; includes backlog and fair scheduling | always                                                 |
| transport_ms     | daemon-plugin frame roundtrip excluding handler work             | always                                                 |
| plugin_ms        | plugin sandbox handler plus the Plugin API call                  | always; 0 recorded for pure cache hits (never imputed) |
| normalization_ms | daemon-side model build, validation and schema check             | always                                                 |
| render_ms        | export render inside the editor                                  | export class only                                      |
| e2e_ms           | client-observed total                                            | always                                                 |

### 2.1 N01 protocol (W1)

| Entry                | Specification                                                                                                                                                                                                                                                                                       |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Target               | cached metadata response p95 <= 100 ms; every metadata response <= 8 KiB; sustained average >= 20 req/s across all 10,000 reads                                                                                                                                                                     |
| Setup                | release daemon, pinned small (100 node) and medium (1,000 node) fixtures; cache warmed and TTL configuration recorded (the default 2 s live TTL would expire mid-run, so the cached profile records its explicit config; expired entries trigger fresh reads that count only under the fresh class) |
| Steps                | 1) ten warm-up fresh reads, excluded from primary denominators; 2) 10,000 cached reads (list_nodes, get_nodes with fixed field projection) alternating fixtures every 1,000 requests for balance; 3) per-response logging; 4) stop and export the raw log                                           |
| Timing decomposition | queue_ms, transport_ms, normalization_ms, e2e_ms; plugin_ms recorded as 0 for hits; eviction and expiry events logged                                                                                                                                                                               |
| Sample size          | full population of 10,000 responses; warm-up excluded                                                                                                                                                                                                                                               |
| Metrics              | p50/p95/p99 of e2e and each stage; bytes per response; hit rate; eviction count; sustained req/s per sliding 60 s window; CPU percent, RSS each 1 s; plus CPU/OS/Figma version/document size/operation/bytes on every record                                                                        |
| Pass thresholds      | p95 <= 100 ms; maximum metadata response <= 8 KiB; sustained mean >= 20 req/s; expiry-triggered fresh reads reported rather than hidden                                                                                                                                                             |
| Failures reported    | blocked, timeout, unsupported, cancelled counts; INVALID_LIMIT and eviction storms; driver-generated BUSY excluded from the product denominator                                                                                                                                                     |
| Evidence files       | benchmarks/results/N01/<commit-short>/run-<n>.raw.jsonl; summary-<os>-<figma_version>.md; profile export N01-profile.md                                                                                                                                                                             |

### 2.2 N02 protocol (W2)

| Entry                | Specification                                                                                                                                                                                                                                                                                                    |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Target               | fresh read p95 <= 2 s for <= 100 simple nodes / <= 64 KiB; 1,000 sequential fresh reads complete within 60 min; every read reaches the plugin                                                                                                                                                                    |
| Setup                | real editor session only; small fixture; grant root; scheduler idle; read deadline default 20 s per docs/04-data-contracts/06-resource-limits.md; consistency=fresh with no cache injection                                                                                                                      |
| Steps                | 1) pair and grant; 2) run 1,000 sequential fresh reads of <= 100 node batches (<= 50 IDs per batch, so 100 nodes means two batches) with fixed projection; 3) per read verify plugin receipt: request_id logged with plugin_ms > 0; 4) wall clock measured against the 60 min budget (average budget 3.6 s/read) |
| Timing decomposition | queue_ms, transport_ms, plugin_ms, normalization_ms, e2e_ms                                                                                                                                                                                                                                                      |
| Sample size          | full population of 1,000 reads                                                                                                                                                                                                                                                                                   |
| Metrics              | p50/p95/p99 per stage; bytes per response; reached_plugin flag mandatory; retry count (max 1 per idempotent read); DOCUMENT_CHANGED events; CPU/OS/Figma version/document size/operation/bytes                                                                                                                   |
| Pass thresholds      | 1,000 of 1,000 reads reached the plugin; wall clock <= 60 min; p95 <= 2 s; zero cache substitution (any hit voids the read and is reclassified)                                                                                                                                                                  |
| Failures reported    | blocked, timeout, unsupported, cancelled; retries with outcomes; PLUGIN_OFFLINE interruptions cut the trial and are rerun and annotated, not deleted                                                                                                                                                             |
| Evidence files       | benchmarks/results/N02/<commit-short>/run-<n>.raw.jsonl; summary-<os>-<figma_version>.md; profile export N02-profile.md                                                                                                                                                                                          |

### 2.3 N03 protocol (W3)

| Entry                | Specification                                                                                                                                                                                                                          |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Target               | PNG 1024x1024 export p95 <= 5 s on the simple fixture; 100 exports; isolated export queue; render cost logged separately                                                                                                               |
| Setup                | real editor; export fixture with image fills, gradients, SVG and a text node; export queue isolated with one global export op; export limits (16 megapixels, scale 0.25 to 2) default per docs/04-data-contracts/06-resource-limits.md |
| Steps                | 1) hash baseline fixture state; 2) 100 sequential export_assets requests PNG 1024x1024; 3) hash each asset, verify MIME and dimensions; 4) run editor responsiveness probes (N06 method) during the run                                |
| Timing decomposition | queue_ms, transport_ms, render_ms, e2e_ms; bytes recorded per asset                                                                                                                                                                    |
| Sample size          | full population of 100 exports                                                                                                                                                                                                         |
| Metrics              | p50/p95/p99; bytes per asset; hash matches; editor stall probes; CPU/RSS each 1 s; CPU/OS/Figma version/document size/operation/bytes                                                                                                  |
| Pass thresholds      | p95 <= 5 s; asset hash matches the recorded export; INVALID_LIMIT rejections stay in the denominator as blocked                                                                                                                        |
| Failures reported    | blocked (INVALID_LIMIT, SCOPE_TOO_LARGE), timeout (60 s), unsupported format, cancelled                                                                                                                                                |
| Evidence files       | benchmarks/results/N03/<commit-short>/run-<n>.raw.jsonl plus asset manifest (hashes and sizes, not binaries); summary-<os>-<figma_version>.md                                                                                          |

### 2.4 N04 protocol

| Entry                | Specification                                                                                                                                                               |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Target               | daemon idle RSS <= 50 MiB; mixed-load peak RSS <= 256 MiB; 1 s sampling; allocator overhead documented; cache bounded as configured                                         |
| Setup                | release daemon with no sessions for the idle phase (10 min), then W4 soak for the peak phase; sampler reads RSS (plus working set/PSS where available) externally every 1 s |
| Steps                | 1) measure idle phase; 2) run W4 while sampling; 3) extract idle p95/max and windowed soak peak; 4) record allocator behavior and cache-bytes curve with eviction events    |
| Timing decomposition | not applicable; resource profile, although stage latencies are cross-checked against N01/N02 references                                                                     |
| Sample size          | ~600 idle samples; full soak sample set                                                                                                                                     |
| Metrics              | RSS idle p50/p95/max; soak peak; CPU; page faults where available; eviction event count; cache-bytes curve over time exported separately                                    |
| Pass thresholds      | idle p95 and max <= 50 MiB; soak peak <= 256 MiB; no OOM (cross-checked with N05)                                                                                           |
| Failures reported    | limit breaches, OOM candidates, eviction storms, allocator anomalies                                                                                                        |
| Evidence files       | benchmarks/results/N04/<commit-short>/rss-<run>.csv; summary-<os>-<figma_version>.md                                                                                        |

### 2.5 N05 protocol (W4)

| Entry                | Specification                                                                                                                                                                                                                                                                 |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Target               | no crash or OOM; unexpected errors < 0.1% on the valid workload; 60 min soak; 2 clients; 4 mock sessions; >= 1 real-editor session                                                                                                                                            |
| Setup                | W4 profile; error taxonomy from section 1.3 preloaded into the classifier; 2 MCP clients; 4 mock sessions over scripted transport; 1 real editor session repeating the same mix                                                                                               |
| Steps                | 1) start all sessions; 2) run mixed cached, fresh, snapshot and export tasks continuously (stats split per class); 3) scheduled disturbances: plugin close/reopen in mock harness, sleep/wake when supported; 4) live error classification; 5) final report with denominators |
| Timing decomposition | per class as in W1..W3 rules; soak latency figures never become N01..N03 evidence                                                                                                                                                                                             |
| Sample size          | full 60 min population; denominator is all classified operations                                                                                                                                                                                                              |
| Metrics              | unexpected error count and rate; crash count; RSS peak; per-client queue fairness; heartbeat gaps; CPU/OS/Figma version/document size/operation/bytes                                                                                                                         |
| Pass thresholds      | unexpected < 0.1%; zero crashes; zero OOM; FR22 fairness observations recorded for the G3 window                                                                                                                                                                              |
| Failures reported    | every classified and unclassified event; cancel/revoke storms annotated separately                                                                                                                                                                                            |
| Evidence files       | benchmarks/results/N05/<commit-short>/soak-<n>.jsonl; summary-<os>-<figma_version>.md; error ledger error-ledger.csv                                                                                                                                                          |

### 2.6 N06 protocol

| Entry                | Specification                                                                                                                                                                                                                                                                        |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Target               | no editor freeze > 1 s caused by our traversal on the reference fixture; the editor's own internal export timing is reported separately and never hidden inside our numbers                                                                                                          |
| Setup                | real editor; frozen reference traversal fixture (1,000 nodes); traversal slicing target 8 ms with yield between slices per docs/04-data-contracts/06-resource-limits.md; screen recording plus scripted input probes; normal runtime, no Developer VM [S11]                          |
| Steps                | 1) heavy list_nodes full-subtree traversal, inspect_design and an export on the fixture; 2) input responsiveness probes every 250 ms plus a manual typing/scroll test by the operator; 3) capture plugin main-thread blocking traces; 4) repeat at least 3 times per heavy operation |
| Timing decomposition | plugin_ms split into host_call_ms and slice_ms; slice count and longest slice recorded                                                                                                                                                                                               |
| Sample size          | >= 3 repetitions per heavy operation per fixture                                                                                                                                                                                                                                     |
| Metrics              | longest input stall; stall distribution; slicing behavior; CPU/OS/Figma version/document size/operation                                                                                                                                                                              |
| Pass thresholds      | no input stall > 1 s attributable to our traversal; editor-internal export time reported in its own field                                                                                                                                                                            |
| Failures reported    | every stall >= 1 s with its trace attached                                                                                                                                                                                                                                           |
| Evidence files       | benchmarks/results/N06/<commit-short>/trace-<run>.json; summary-<os>-<figma_version>.md                                                                                                                                                                                              |

### 2.7 N07 protocol

| Entry                | Specification                                                                                                                                                                                                                                                           |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Target               | logical cancel/revoke response <= 1 s; zero payload bytes sent after revoke; bytes already delivered before revoke are noted but are not counted as a new leak (PRD 2.1)                                                                                                |
| Setup                | instrumented daemon and plugin logging cancel-ack and frame-write timestamps; two scenarios: cancellation of an in-flight read; revoke during a chunked export transfer (32 MiB transfer cap)                                                                           |
| Steps                | 1) start a large read, cancel it at a random point, record cancel-to-stop time; 2) start an export, revoke mid-transfer via UI and via CLI in separate trials; 3) record last_payload_ms against the revoke timestamp; 4) count any payload bytes received after revoke |
| Timing decomposition | cancel_to_stop_ms, revoke_to_last_payload_ms, queue_clear_ms                                                                                                                                                                                                            |
| Sample size          | >= 30 cancellations and >= 10 mid-transfer revokes across scenarios                                                                                                                                                                                                     |
| Metrics              | p50/p95/p99 cancel and revoke latency; post-revoke payload bytes (target 0); cache and asset invalidation confirmations; CPU/OS/Figma version/operation                                                                                                                 |
| Pass thresholds      | <= 1 s logical latency; 0 post-revoke payload bytes; REVOKED answers per contract                                                                                                                                                                                       |
| Failures reported    | any late frame accepted after revoke, any surviving cursor or asset URI                                                                                                                                                                                                 |
| Evidence files       | benchmarks/results/N07/<commit-short>/cancel-<run>.jsonl; summary-<os>-<figma_version>.md                                                                                                                                                                               |

### 2.8 N08 protocol

| Entry                | Specification                                                                                                                                                                                                         |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Target               | offline detected <= 15 s (heartbeat every 5 s, 3 missed hearts allowed); reconnect readiness <= 10 s after the transport recovers                                                                                     |
| Setup                | real plus mock sessions; scripted scenarios: sleep/wake, kill plugin, daemon restart, port collision; wall clocks instrumented                                                                                        |
| Steps                | per scenario: establish a healthy read, trigger the disturbance, measure detect time, recover, measure ready time (auth plus grant revalidation plus new connection_epoch; no blind replay); >= 5 trials per scenario |
| Timing decomposition | detect_ms, ready_ms                                                                                                                                                                                                   |
| Sample size          | >= 5 trials per scenario across 4 scenarios                                                                                                                                                                           |
| Metrics              | p50/p95 detect and ready; epoch bump verification; duplicated-request count (target 0); CPU/OS/Figma version                                                                                                          |
| Pass thresholds      | detect <= 15 s; ready <= 10 s; zero stale-epoch payloads accepted; port collision fails loudly with --port guidance, never silently rebinds                                                                           |
| Failures reported    | detection misses over 15 s, manual interventions, replay-after-reconnect instances                                                                                                                                    |
| Evidence files       | benchmarks/results/N08/<commit-short>/reconnect-<scenario>-<run>.jsonl; summary-<os>-<figma_version>.md                                                                                                               |

### 2.9 N09 protocol

| Entry                | Specification                                                                                                                                                                                                                                                                                            |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Target               | zero official MCP/REST requests from core on the exact commit; evidence by source review plus deny-egress mock plus per-process connection capture                                                                                                                                                       |
| Setup                | exact release commit; source review checklist over all core crates enumerating network-capable code and proving loopback-only by construction; harness that drops and flags every non-loopback egress attempt from FigTether processes; per-process connection capture from the same commit              |
| Steps                | 1) static review of the exact commit recorded as review notes; 2) run the AT01, AT02 and AT18 flows under deny-egress and capture; 3) attribute every observed connection; editor-originated traffic is recorded and excluded with an attribution note; 4) repeat capture at the exact G2 release commit |
| Timing decomposition | not applicable; boolean plus artifact evidence                                                                                                                                                                                                                                                           |
| Sample size          | every release commit published with performance or capacity claims; at minimum G0 and the G2 release candidate                                                                                                                                                                                           |
| Metrics              | official endpoint artifact count (target 0); non-loopback attempt count (target 0); capture log references; OS and Figma version context                                                                                                                                                                 |
| Pass thresholds      | zero official endpoint artifacts; any hit is a release blocker, never interpolated away; eligibility failures are access limitations (PRD 2.2), not accuracy failures                                                                                                                                    |
| Failures reported    | every capture discrepancy plus the matching adapter-code verdict                                                                                                                                                                                                                                         |
| Evidence files       | benchmarks/results/N09/<commit-short>/capture-<run>.log; review-<commit>.md; summary-<os>.md                                                                                                                                                                                                             |

### 2.10 N10 protocol (W5)

| Entry                | Specification                                                                                                                                                                                                                                                        |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Target               | first onboarding completion p90 <= 15 min; repeat session <= 2 min; >= 10 pilot users; per-step failures recorded (PRD 2.1)                                                                                                                                          |
| Setup                | distribution bundle per FR01: binary, plugin bundle, checksums; no Rust or Node toolchain present; coverage on Windows and macOS; scripted step list: install, doctor, daemon, manifest, pair, grant, config print, first agent read                                 |
| Steps                | 1) pilot user performs the first run guided only by the docs, recorder logs per-step times and failures; 2) every assisted fix is logged as an intervention; 3) repeat run the next day, unassisted; 4) 30-minute continuous session check with the plugin kept open |
| Timing decomposition | per step: install_ms, doctor_ms, pair_ms, grant_ms, config_ms, first_read_ms; plus overall first_ms and repeat_ms                                                                                                                                                    |
| Sample size          | >= 10 users, each one first run plus one repeat run                                                                                                                                                                                                                  |
| Metrics              | p50/p90 first run; p90 repeat; per-step failure counts; intervention log; OS and Figma version                                                                                                                                                                       |
| Pass thresholds      | first run p90 <= 15 min; repeat p90 <= 2 min; no step with total pilot failure (a step failing for every user is a release blocker)                                                                                                                                  |
| Failures reported    | step failures, missing doc steps, toolchain installs discovered, all user-identifying details stripped                                                                                                                                                               |
| Evidence files       | benchmarks/results/N10/<commit-short>/pilot-<user-id>-<run>.json sanitized; summary-<os>.md                                                                                                                                                                          |

## 3. Reporting template

Every report uses this template. All fields are mandatory; write not_applicable explicitly instead of
omitting a field. The field set mirrors the PRD release-evidence template (Appendix C of
FigTether_PRD_Confluence_v1.1) and is filled only with measured values.

```text
# N<hh> benchmark report - <profile id W1..W5>
commit                 <full core sha and plugin sha; exact commit required>
artifact_sha256        <binary and plugin bundle hashes of the tested artifacts>
schema_version         <envelope schema in use, per docs/04-data-contracts/01-mcp-envelope.md>
protocol_version       <bridge protocol version, per docs/04-data-contracts/03-bridge-protocol.md>
os                     <Windows version x64 or macOS version arm64/x64>
cpu                    <model plus logical core count; device profile 4 cores stated>
ram                    <8 GB device profile stated>
figma_version          <exact desktop version and build>
plugin_manifest_hash   <hash of the manifest actually loaded>
account_plan_seat_context  <plan/seat text plus eligibility probe result, no personal identity>
fixture_hash           <one hash per fixture used>
test_id                <N01..N10 plus linked ATxx where applicable>
workload_class         <cached | fresh | snapshot | export | mixed soak per 2.0>
commands               <exact commands including every config override>
expected               <thresholds and denominators fixed before the run>
actual                 <measured values>
duration               <wall clock>
latency                <p50/p95/p99 of e2e and queue/transport/plugin/normalization/render stages>
rss                    <idle p95 and max; mixed peak>
denominators           <total, completed, passed, failed, blocked, timeout, unsupported, cancelled>
errors                 <unexpected_count, expected_count, unexpected_rate>
network_path_evidence  <N09 capture and review references; loopback evidence for other tests>
known_limitations      <blocked or unsupported contexts, rerun history, operator interventions>
profile_refs           <benchmark result file names produced per section 2>
reviewer               <independent reviewer name or role>
tested_at              <UTC timestamp>
```

Reviewer rejection rules: a report missing any field, a cached/fresh mix inside one statistical base,
a denominator without its failure classes, or a pass computed after changing the pre-declared
threshold is rejected in review and rerun.

## 4. Publication policy

| Rule                   | Detail                                                                                                                                                  | Enforcement                                          |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| Tested profile only    | claims name the tested OS, device profile, Figma version, fixture and commit; anything above that profile carries the custom/unvalidated label          | wording review before publish; config digest visible |
| README and site claims | no unlimited promise, no all-features-free phrasing; measured limits published with tables and commit links                                             | AT16 blocks G2 until the claims pass                 |
| Number provenance      | every published number links commit, artifact hash and config, and shows p50/p95/p99 with denominators and failure classes                              | template compliance in section 3                     |
| Fresh versus cached    | cached workload numbers are never presented as live-read capacity; 10,000 cache hits are not 10,000 live reads (PRD 2.1)                                | workload_class field plus review                     |
| Quota accounting       | no quota-saving claim is interpolated from local read counts; reports show actual operation counts by source (cached, fresh, snapshot, export) instead  | publication review checklist                         |
| Capacity evolution     | raising a claim above the tested profile requires a full rerun under section 1 and an updated profile description; targets never quietly become results | traceability matrix rule and evidence slots          |
| Failed runs            | reruns supersede; failed runs remain filed as known limitations with reproduction steps                                                                 | evidence retention rule                              |
| Platform change        | a Figma version change triggers smoke tests plus re-measurement of the N subset feeding changed claims; last-tested version stays dated                 | pre-release checklist                                |

Revision history: v0.1, 2026-10-20, proposed together with docs/06-testing/03-traceability-matrix.md
and docs/06-testing/02-acceptance-catalog.md; no measured value exists yet anywhere in this plan.
