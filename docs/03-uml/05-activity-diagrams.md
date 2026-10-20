# UML Activity Diagrams

Status: `Proposed`

Purpose: render three flows as flowchart based activity diagrams: onboarding with its failure
exits, the scheduler tick with its priority lanes, and snapshot capture with its guard outcomes.
A1 follows PRD 5.1 step by step and targets N10 onboarding p90 at most 15 minutes. A2 encodes the
priority order and starvation control of PRD 8.5.5. A3 encodes the snapshot guards of PRD 8.3 and
8.5.5. Decision and activity nodes are rectangles and rhombi in Mermaid flowchart TD; semantics
follow UML activity notation. Gate G0 has not run, so these are normative proposals.

## A1: end user onboarding per PRD 5.1

Read as: install, verify, diagnose, bind port, pair within the windows, grant, print config, and
read, with every failure exit loud and recoverable rather than silent.

```mermaid
flowchart TD
    s1["Begin onboarding per PRD 5.1 steps 1 to 8"] --> s2["Download binary and plugin bundle and verify checksum"]
    s2 --> d1{"doctor passes on versions port runtime dir and manifest"}
    d1 -- "no" --> s3["Apply doctor guidance and regenerate manifest with figtether manifest"]
    s3 --> d1
    d1 -- "yes" --> d2{"port 38947 free and loopback bind ok"}
    d2 -- "no" --> s4["Fail loudly with --port hint and regenerate manifest, no silent port switch"]
    s4 --> f1["Onboarding retry or abort at this step"]
    d2 -- "yes" --> s5["Run figtether daemon on the chosen port"]
    s5 --> s6["Open the plugin in Figma Design"]
    s6 --> s7["Run figtether pair and copy the 256 bit secret"]
    s7 --> d3{"secret submitted inside 3 s window and 5 min TTL"}
    d3 -- "no" --> s8["AUTH_REQUIRED timeout, pair again with a fresh secret"]
    s8 --> s7
    d3 -- "ok" --> s9["Session PAIRED then AWAITING_GRANT"]
    s9 --> d4{"user selects a root and presses Allow session read"}
    d4 -- "reject" --> s10["Grant rejected, session stays AWAITING_GRANT or moves to REVOKED"]
    s10 --> f2["No reads happen until the user grants"]
    d4 -- "grant" --> s11["Session READY with grant_epoch at 1"]
    s11 --> s12["figtether config print client generic flag and paste entry into agent"]
    s12 --> s13["Agent lists sessions and reads a summary before details"]
    s13 --> end1["Onboarding complete, N10 pilot logs per step failure"]
```

| Element | Reference |
| Checksum and doctor steps | FR01, FR18, PRD 5.1 steps 1 to 2, AT03 |
| Port collision loud failure | PRD 5.1 step 3, PRD 8.7 port collision row |
| 3 s and 5 min pairing guards | FR02, PRD 8.5.4 |
| Grant step and no silent widening | FR03, PRD 5.1 step 6 |
| Config print for MCP entry | FR17, PRD 5.1 step 7 |
| Summary before detail reads | FR04, FR12, PRD 5.1 step 8 |

## A2: scheduler tick with priority lanes

Read as: control work drains first, then small reads under deadlines, then one serial export;
aging keeps starved jobs alive and a full queue answers BUSY instead of hanging.

```mermaid
flowchart TD
    t1["Scheduler tick begins"] --> t2["Lane 1 control work has highest priority"]
    t2 --> t3["Process revoke cancel and heartbeat messages first"]
    t3 --> d1{"control work pending"}
    d1 -- "yes" --> t4["Bump epochs, cancel jobs, purge cache and assets, then continue"]
    t4 --> t5["Lane 2 read lane checked next"]
    d1 -- "no" --> t5
    t5 --> t6["Scan queues with 64 total and 16 per client caps"]
    t6 --> d2{"deadline already spent while waiting"}
    d2 -- "yes" --> t7["Kill without executing and answer TIMEOUT, late results are never delivered"]
    t7 --> t8["Apply aging to prevent starvation of large jobs"]
    d2 -- "no" --> t8
    t8 --> d3{"job fits session READY and grant gates"}
    d3 -- "no" --> t9["Reject with PAUSED REVOKED or PLUGIN_OFFLINE as structured result"]
    t9 --> t10["Lane 3 export lane evaluated last"]
    d3 -- "yes" --> t11["Dispatch small reads to the plugin with 8 ms slices and progress"]
    t11 --> t10
    t10 --> d4{"global serial export slot free"}
    d4 -- "yes" --> t12["Dispatch exactly one export job under 60 s deadline"]
    t12 --> t13["Collect outcomes and rejection reasons"]
    d4 -- "no" --> t13
    t13 --> t14["Close tick, BUSY replies carry retry_after_ms, return to lane 1 next tick"]
```

| Element | Reference |
| Revoke cancel heartbeat priority | PRD 8.5.5 scheduling note, FR15 |
| Read lane caps and deadline kill | FR13, FR14, PRD 8.5.5 |
| Aging prevents starvation | PRD 8.5.5 |
| One serial export globally | PRD 8.5.5 operation limits, N03 |
| BUSY carries retry_after_ms | FR13, PRD 8.7 error table |

## A3: snapshot capture flow with guard outcomes

Read as: capture is user driven and gated on session state, session cap, scope size and payload
size; every rejecting branch leaves the session usable and returns a structured error.

```mermaid
flowchart TD
    c1["User presses Capture snapshot in the plugin UI"] --> d0{"sessions below the cap of 4"}
    d0 -- "no" --> c2["SESSION_LIMIT answer, user closes or frees a session"]
    c2 --> f1["No capture performed, session remains READY"]
    d0 -- "yes" --> d1{"session state is READY"}
    d1 -- "PAUSED or REVOKED or OFFLINE" --> c3["Structured PAUSED REVOKED or PLUGIN_OFFLINE answer, no new capture"]
    c3 --> f1
    d1 -- "READY" --> c4["Estimate node count inside the grant scope"]
    c4 --> d2{"estimate within 10k nodes and depth 32"}
    d2 -- "no" --> c5["SCOPE_TOO_LARGE, ask the user for a narrower root projection"]
    c5 --> f2["Capture declined early, no traversal cost paid"]
    d2 -- "yes" --> c6["Create the observation window tagged best_effort"]
    c6 --> c7["Traverse with 8 ms slices and hash the canonical payload"]
    c7 --> d3{"payload within the 32 MiB transfer guard"}
    d3 -- "no" --> c8["PAYLOAD_TOO_LARGE, cleanup and keep the session READY"]
    c8 --> f3["Derived memory released, user narrows the capture"]
    d3 -- "yes" --> c9["Chunked upload with transfer_id, frame cap 1 MiB, final checksum"]
    c9 --> c10["Daemon reassembles bound, stores immutable snapshot"]
    c10 --> c11["Publish snapshot_id content hash captured_at and idle TTL 30 min"]
    c11 --> c12["UI shows capture metadata labeled best_effort"]
```

| Element | Reference |
| Session state gate | FR15, PRD 8.3 pause and revoke paragraph |
| Session cap 4 | PRD 8.5.5 session limit |
| Traversal size guards | PRD 8.5.5, SCOPE_TOO_LARGE row |
| Payload transfer guard | PRD 8.5.4, PAYLOAD_TOO_LARGE row |
| best_effort labeling and TTL | PRD 8.3, R14 |

Notes:

- Capture always labels consistency best_effort because edits can land during capture; the
  snapshot stays immutable once stored and never claims live freshness (PRD 8.3, FR05).
- Guard evaluation happens before any heavy allocation, per the commit that every cap is
  enforced at insertion (docs/00-overview/01-prd-analysis.md section 10 item 5).
- N06 and N04 stay in force during capture: slicing and bounded reassembly exist to protect the
  editor and the daemon memory profile.

Open questions:

1. Whether doctor should offer a flags-based auto fix recommending manifest regeneration without
   writing anything per PRD 5.1 step 4 is an ops decision; A1 keeps regeneration explicit.
2. Whether A3's scope estimate can be exact for dynamic-page documents before traversal is unknown
   until G0 probes run; the flow treats estimates as conservative guards.
3. A2 lane thresholds for aging increments are intentionally unspecified; they need a scheduler
   owner decision plus an AT08 soak before values are frozen.
