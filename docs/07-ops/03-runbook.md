# Operations Runbook

Status: `Proposed`

This runbook defines what an operator or maintainer watches (metrics), how diagnostics stay
hygienic, and step-by-step procedures for graceful shutdown, crash recovery, port conflicts,
upgrade and rollback, incident response, and support intake. Gate `G0` has not run, so `figtether`
command shapes are the **proposed CLI surface**; numbers stated anywhere are targets until
measured, never results.

## 1. Metrics to watch

FigTether exposes local metrics only (FR18); there is no telemetry endpoint. All counters are
per-instance and reset on daemon restart, which is acceptable because the runbook consumes them
interactively.

| Metric                                | Definition                                                                                                                                 | Why it matters                                                                                           |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------- |
| Requests per operation and per source | Counter split by tool operation and `source.kind` (`plugin_live`, `plugin_snapshot`, later `offline_bundle`)                               | Actual usage evidence instead of "official calls saved" style claims, per PRD 8.7                        |
| Cache hits                            | Served from cache; only exists for explicitly requested `consistency=cached` reads                                                         | Confirms fresh-first (`D04`) is not quietly violated and separates cached workloads in reporting         |
| Fresh plugin reads                    | Requests that reached the plugin as new reads                                                                                              | Direct `N02` coverage signal; a low ratio to requests hints at queue starvation                          |
| Queue depth and queue wait            | Current queued requests and their wait times. Per client (16) and total (64) budget live in `docs/04-data-contracts/06-resource-limits.md` | Forecast `BUSY` pressure and verify backpressure works (`FR13`)                                          |
| p95 latency per operation             | Latency split by operation class                                                                                                           | Against targets `N01` (cached p95), `N02` (fresh p95), `N03` (export p95); never averaged across classes |
| Bytes served                          | Response and transfer sizes                                                                                                                | Context-size control story (`P04`); confirms responses stay under the 64 KiB MCP budget                  |
| Cancels and revokes                   | Counter and time-to-effect                                                                                                                 | Verify logical effect within 1 s per `N07`; a spike in cancels may indicate `R04` editor strain          |
| Event gaps                            | Watch-only; gap and resync counters (`FR20`, G3)                                                                                           | Guard against stale truth if watch is ever exposed; live reads are the default answer (`R03`)            |
| RSS                                   | Resident memory sampled during benchmarks at 1 s per `N04`; periodic lightweight samples otherwise                                         | Detect drift toward the 50 MiB idle / 256 MiB mixed-load envelope                                        |

Recommendation for alerting spirit: thresholds are derived from the measured baseline recorded in
`docs/06-testing/04-benchmark-plan.md` per gate; fixed CI thresholds are published only after `G2`
evidence.

## 2. Diagnostic log hygiene

Summary of the binding rules (PRD 8.6.2, FR18); the full data-handling table lives in
`docs/05-security/03-privacy-data-handling.md`.

1. Logged content by default: error code, correlation id, request timing, byte counts, queue depth,
   versions, and resource counts. Never by default: layer or text content, file ids, node names,
   credentials, prompts.
2. Rotation: local files, maximum retention 7 days or 10 MiB, whichever hits first; rotation is
   enforced by size checks on write, not by a background timer alone.
3. Correlation: every loggable error carries a sanitized correlation id usable for support intake
   (section 5.2 below); stack traces of internal bugs never reach agent-visible results.
4. Crash reports are opt-in, user-reviewed before send, and sanitized; the product never ships
   design content off the device implicitly.
5. Doctor output obeys the same hygiene when preparing support material.

### 2.1 Failure signatures at a glance

First-response mapping from what an operator typically observes to the metric that explains it.
The benchmark plan in `docs/06-testing/04-benchmark-plan.md` holds the per-class measurement
methods; this table is the interactive reading order.

| Observed signature                               | First metric to check      | Likely contributing factor                                                                            | Immediate operator step                                                                                                                   |
| ------------------------------------------------ | -------------------------- | ----------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Fresh reads degrade into repeated `BUSY`         | Queue depth and queue wait | One client or one slow export exhausting the queue budget                                             | Identify the holding session; request a narrower scope; confirm exports are not starving reads                                            |
| Cache-hit rate near zero and trending down       | Cache hits                 | Nothing unusual: fresh is the live default (`D04`); cached reads exist only when explicitly requested | Do not "fix"; report cached and fresh workloads separately in any review                                                                  |
| Fresh p95 latency above target on small fixtures | p95 latency per operation  | Plugin offline intervals or editor strain (`R04`)                                                     | Check plugin connection state; re-verify the fixture class before escalating                                                              |
| RSS climbs toward the mixed-load ceiling         | RSS samples                | Large transfer reassembly or asset cache growth                                                       | Confirm limits are enabled at documented defaults; a daemon restart is a stopgap; file an issue with the sampled curve, no design content |
| Cancel counter spikes from one client            | Cancels and revokes        | Agent client timeout shorter than documented request deadlines                                        | Verify the client honors `retry_after_ms` and the published deadline envelopes                                                            |

## 3. Operational procedures

Before any operator action, propose or verify no AT evidence depends on the current process state;
procedures below are written to be repeatable.

### 3.1 Graceful shutdown

Proposed CLI shape and ordered steps (PRD 8.7 shutdown row):

```sh
figtether daemon --shutdown-graceful
```

1. Stop accepting new requests and MCP sessions; reply Busy to any late MCP client connect.
2. Cancel queued work in priority order: first `revoke` and `cancel` intents, then heartbeat, then
   small reads, then exports; wait bounded for in-flight plugin operations to report or expire via
   their deadlines.
3. Revoke session credentials; close socket handles; terminate pairing state.
4. Clear the runtime directory of session-scoped files. Stale runtime files from a previous run
   must be detected as untrusted on the next start, not blindly accepted (PRD 8.7).
5. Release design-derived memory caches; confirm RSS drop in a quick `figtether status`.

### 3.2 Daemon crash recovery

1. Start a new daemon instance. It generates a new `daemon_instance_id` and fresh credentials; old
   plugin and client credentials are rejected, and clients must re-pair (fail-closed).
2. On boot the new instance detects stale runtime files from the crashed predecessor and does not
   trust them; it deletes or quarantines them instead (PRD 8.7 crash row).
3. Plugin auto-reconnects only after successful re-authentication; connection epoch increments and
   no queued result is blindly replayed (FR16); unresolved reads return `PLUGIN_OFFLINE` or
   `SESSION_NOT_FOUND` until re-paired.
4. Inspect crash artifacts locally; report templates in section 5.2.

### 3.3 Port conflict resolution

1. Diagnose what occupies the default `38947` loopback port using OS tooling that requires no
   privilege; the daemon's own error message may report what it safely observed (PRD 8.7: show
   owner process/port only where knowable without privilege).
2. Choose a different port explicitly; never silently fall back.
3. Regenerate the manifest for the new port; the plugin bundle needs a matching manifest (PRD 5.1
   step 3). Proposed shape:

```sh
figtether daemon --port 38948
figtether manifest generate --plugin-id "<your-plugin-id>" --port 38948
```

4. Re-run `figtether doctor`; then re-pair. Note IPv6 loopback binding behavior is decided by
   `ADR-002` at `G0`; this runbook assumes the IPv4 loopback baseline until that lands.

### 3.4 Upgrade and rollback

1. Compatibility check runs at startup: plugin and daemon versions are exchanged; an incompatible
   major version fails loudly and refuses degraded service (PRD 8.7).
2. Keep the previous binary; the upgrade procedure does not migrate private data by default, and
   there is nothing durable to migrate by the PRD 8.6.2 retention model.
3. Rollback procedure: stop the new daemon, run the previously installed binary, and re-pair since
   the instance identity is regenerated on every start.
4. If rollback also fails, reproduce against the sanitized doctor output and open a support issue
   (section 5.2) with the public fixture.

### 3.5 Incident response

Trigger: any confirmed security or correctness defect in a released version (scope leak, data leak,
crash, misrouting per `R05`) or a supply-chain compromise of a published artifact.

1. Revoke all sessions and any grants the affected version created; if the plugin cannot be
   reached, the proposed CLI `figtether revoke` path is the dependable route.
2. Stop distribution of the problematic version: mark release assets as withdrawn, freeze affected
   tags, and publish a clear advisory in the changelog and announcement channels.
3. Build and publish a patch or advisory with a sanitized reproduction: public fixture plus
   environment details, never private designs (PRD 8.7 support row).
4. Record the incident timeline in `docs/release-evidence/` (implementation mirror of
   `99-appendices/02-source-register.md` evidence template) and review residual exposure with the
   threat model.

### 3.6 Support intake

Intake is issue-template-based; we never request design files, PATs, tokens, or full logs containing
design content (PRD 8.7; private security reports use `SECURITY` private channel per PRD 8.11).
Required fields per template:

| Field              | Expected content                                                                    |
| ------------------ | ----------------------------------------------------------------------------------- |
| FigTether version  | Daemon, plugin bundle, and MCP adapter versions from `figtether doctor`             |
| OS                 | Windows x64, macOS arm64 or x64, Linux (core CLI only)                              |
| Capability context | Relevant probed capabilities as shown in the plugin UI (never seat assumptions)     |
| Error id           | Structured code and correlation id from the user-visible message                    |
| Reproduction       | Steps against a public fixture; sanitized repro with the public fixture is required |

Intake is never blocked on a private client file; if a private design is needed for a maintainer to
understand a defect, reproduce against the curated `fixtures/` equivalent instead (`R13`).

## 4. Residual risks

For residual risks that operations cannot eliminate (same-user malware, OS administrator, Figma-side
changes, editor resource behavior, model side of the AI client), read `docs/05-security/
01-threat-model.md`; this runbook assumes its boundaries rather than re-arguing them. The bridge is
a support tool for the maintainer, not evidence that a risk is closed.

## 5. Open questions

1. Whether graceful shutdown is a flag as shown, a separate proposed subcommand (`figtether daemon
stop`), or an OS-signal-only path; to be fixed with the `E01` CLI ADR.
2. Exact location and format of local metrics (structured log vs dedicated local endpoint) depends
   on the `E01`/observability decision; the metric list above is binding regardless.
3. Quarantine semantics for stale runtime files (delete vs rename-plus-report) need an ADR
   statement before `G1` because it touches fail-closed behavior.
