# UML Sequence Diagrams

Status: `Proposed`

Purpose: walk the six interactions that define FigTether end to end: pairing and grant, fresh
live read with coalescing and BUSY, snapshot capture, asset export with a mid transfer revoke,
heartbeat loss with an epoch guarded reconnect, and the MCP lifecycle with logical cancellation,
per PRD 4.2, 5, 8.3, 8.4, 8.5 and Appendix A. Gate G0 has not run, so the loopback transport stays D06 candidate until proven on Windows and macOS.

## S1: pairing and grant establishment

Read as: the CLI prints a single use secret, the plugin UI spends it inside the 3 second window, and the user grants roots so the session reaches READY with double enforcement live in both sides.

```mermaid
sequenceDiagram
    participant CLI as figtether CLI
    participant DA as figtether daemon
    participant UI as Plugin UI iframe
    participant PS as Plugin main sandbox
    Note over CLI,UI : Phase 1 the CLI prints the secret and the user copies it into the plugin UI
    CLI->>DA : Pair action verifies local credential and port
    DA-->>CLI : Emit 256 bit secret single use with 5 min TTL
    UI->>DA : Hello auth frame spends the secret on loopback WebSocket 38947 inside 3 s
    alt secret valid unused and fresh
        DA-->>UI : Auth ok with session_id and session credential
        DA->>DA : Mark PAIRED then AWAITING_GRANT and consume the secret
        Note over UI,PS : Phase 2 the user grants the distilled root
        UI->>DA : Grant update with root ids distilled from selection
        DA->>DA : Mint grant_id and set grant_epoch to 1
        DA-->>UI : Grant ack and session READY
        DA->>PS : Allowlist membership rules for plugin side enforcement
    else secret wrong reused expired or window missed
        DA-->>UI : AUTH_REQUIRED and close before any design data
    end
```

| Element                               | Reference            |
| ------------------------------------- | -------------------- |
| Secret issue single use and 5 min TTL | FR02, AT06           |
| 3 s auth window with no earlier data  | PRD 8.5.4            |
| Grant update and dual enforcement     | FR03, PRD 8.4        |
| Failure branch AUTH_REQUIRED          | PRD 8.7 errors, AT06 |

## S2: fresh live read with coalescing and BUSY

Read as: a fresh request always reaches the plugin, identical concurrent requests share one observation window, and a full queue answers BUSY with a retry hint instead of hanging.

```mermaid
sequenceDiagram
    participant MC as MCP client
    participant MA as mcp adapter process
    participant SC as daemon scheduler and cache
    participant PL as Plugin main sandbox
    MC->>MA : Tools call get_nodes with session_id node ids and fields
    MA->>SC : Request envelope with request_id and deadline 20 s
    SC->>SC : Check session READY grant_epoch and node membership
    alt queue has room under 64 total and 16 per client
        SC->>SC : Enqueue inside caps with priority and deadline
        alt identical request already in flight
            Note over SC : Phase 1 coalescing shares one observation window
            MA-->>MC : Shared result labeled with the shared observation_id
        else nothing in flight to join
            Note over SC,MC : Phase 2 fresh dispatch to the plugin
            SC->>PL : Dispatch read with 8 ms slicing hint
            PL->>PL : Traverse depth 32 batch 50 budget 10k nodes
            PL-->>SC : Result chunks with transfer checksum
            SC-->>MA : plugin_live envelope fresh_read and cache_age 0
            MA-->>MC : Result under 64 KiB with truncation never hidden
        end
    else queue full
        SC-->>MA : BUSY carrying retry_after_ms
        MA-->>MC : BUSY result with the retry hint
    end
```

| Element                                     | Reference               |
| ------------------------------------------- | ----------------------- |
| Fresh default never answered from cache     | FR06, PRD 8.3, D04      |
| Coalescing of identical concurrent requests | PRD 8.3 coalescing rule |
| Queue caps 64 and 16 with BUSY              | FR13, PRD 8.5.5         |
| Limits and retry max 1                      | PRD 8.5.5, FR14         |

## S3: snapshot capture, best effort

Read as: the user presses Capture in the plugin UI, the plugin collects and hashes the scope, bounded chunks travel to the daemon, and an immutable snapshot is stored labeled best_effort.

```mermaid
sequenceDiagram
    participant UI as Plugin UI iframe
    participant DA as figtether daemon
    participant PL as Plugin main sandbox
    Note over UI,DA : Phase 1 the user presses Capture snapshot
    UI->>DA : Capture request with session_id and current roots
    DA->>DA : Gate on state READY session cap 4 and scope estimate
    alt estimate above 10k nodes or payload risk
        DA-->>UI : SCOPE_TOO_LARGE with narrowing guidance
    else inside limits
        DA->>PL : Capture job tagged best_effort with observation window
        PL->>PL : Collect nodes in 8 ms slices and hash the canonical payload
        PL->>DA : Upload chunks with transfer_id sequence and frame cap 1 MiB
        DA->>DA : Bounded reassembly against 32 MiB with final checksum
        DA-->>UI : snapshot_id content hash captured_at and idle TTL 30 min
        Note over UI,DA : Phase 2 the snapshot stays immutable and reads only via source snapshot
    end
```

| Element                            | Reference           |
| ---------------------------------- | ------------------- |
| Capture is user initiated only     | FR05, PRD 8.5.1     |
| best_effort consistency labeling   | PRD 8.3, R14        |
| Chunking checksum and 32 MiB guard | PRD 8.5.4           |
| Snapshot TTL 30 min                | PRD 8.3 cache rules |

## S4: asset export with mid transfer revoke

Read as: an export runs serially under hard limits; a revoke during the transfer bumps the epoch, stops the job at a slice boundary, purges derived data, and bytes already delivered stay with the client.

```mermaid
sequenceDiagram
    participant MC as MCP client
    participant MA as mcp adapter process
    participant SC as daemon scheduler and asset store
    participant PL as Plugin main sandbox
    participant UI as Plugin UI iframe
    MC->>MA : Tools call export_assets with node ids format and scale
    MA->>SC : Request envelope with limits to verify
    alt dimension or format breach
        SC-->>MA : INVALID_LIMIT with 16 MP and scale 0.25 to 2 guidance
    else inside serial export budget
        Note over SC,PL : Phase 1 the global serial export slot opens
        SC->>PL : Export job with 60 s deadline
        PL-->>SC : PNG or SVG byte chunks under frame cap 1 MiB
        SC->>SC : Verify chunk counts and final checksum
        SC-->>MA : asset_id MIME bytes hash and grant bound resource URI
        MA-->>MC : Result plus resource listing
        opt user presses Revoke mid transfer
            Note over UI,MC : Phase 2 revoke interrupts the transfer
            UI->>SC : Revoke grant action
            SC->>SC : Bump grant_epoch cancel job purge cache and assets
            SC->>PL : Stop at the next 8 ms slice boundary
            MA-->>MC : Structured REVOKED result with no further bytes
        end
    end
    Note over SC,MC : Bytes delivered before the revoke cannot be recalled by the daemon
```

| Element                          | Reference                         |
| -------------------------------- | --------------------------------- |
| Serial export and 60 s deadline  | FR11, PRD 8.5.5, N03              |
| INVALID_LIMIT branch             | PRD 8.5.5 export limit row        |
| Revoke stops transfer and purges | FR15, N07, AT15                   |
| Delivered bytes not recallable   | PRD 8.4 and 8.6.1 residual limits |

## S5: heartbeat loss, offline, and reconnect with epoch bump

Read as: heartbeats every 5 s stop after three misses, the session turns OFFLINE within 15 s, late old epoch frames are discarded, and reconnect requires authentication plus grant revalidation with no blind replay.

```mermaid
sequenceDiagram
    participant UI as Plugin UI iframe
    participant DA as figtether daemon
    participant MA as mcp adapter process
    participant MC as MCP client
    Note over UI,DA : Phase 1 heartbeats every 5 s with ack then the editor sleeps and the network flakes
    DA->>DA : 3 consecutive misses mark session OFFLINE within 15 s
    DA->>DA : Cancel queued jobs and flag the old connection_epoch
    DA-->>MA : PLUGIN_OFFLINE for fresh reads
    MA-->>MC : Structured PLUGIN_OFFLINE and no snapshot posing as live
    Note over DA,MA : A heartbeat restored inside the grace window resets the miss count instead
    Note over UI,DA : Phase 2 the plugin redials and re-authenticates
    UI->>DA : Hello auth with the still valid in memory credential
    DA->>DA : Bump connection_epoch and revalidate grant membership
    DA-->>UI : Session returns to READY or PAUSED within the 10 s target
    Note over DA,MC : Phase 3 no blind replay and late old epoch frames discarded
    MC->>MA : New fresh get_nodes after reconnect
    MA-->>MC : Result carries the new connection_epoch
```

| Element                               | Reference        |
| ------------------------------------- | ---------------- |
| Heartbeat 5 s and offline at 3 misses | PRD 8.4, N08     |
| Epoch bump and revalidated grant      | FR16, FR03, AT15 |
| No blind replay of old work           | FR16, AT15       |
| Reconnect readiness 10 s target       | N08              |

## S6: MCP lifecycle with logical cancellation

Read as: the client handshakes over stdio, calls a P0 tool, and a cancellation notification reaches the scheduler, which cancels the job at a slice boundary so no partial data leaks.

```mermaid
sequenceDiagram
    participant MC as MCP client
    participant MA as mcp adapter process
    participant SC as daemon scheduler
    participant PL as Plugin main sandbox
    Note over MC,MA : Phase 1 the MCP handshake runs on stdio
    MC->>MA : initialize with a protocol version
    MA-->>MC : initialize result with server capabilities
    Note over MC,SC : Phase 2 a fresh tool call becomes a scheduler job
    MC->>MA : Tools call get_nodes with consistency fresh
    MA->>SC : Create request job with deadline 20 s
    SC->>PL : Dispatch after state and grant checks
    Note over MC,PL : Phase 3 client cancellation reaches the scheduler
    MC->>MA : Cancellation notification for the request_id
    MA->>SC : Cancel the matching job by id
    SC->>PL : Stop at the next 8 ms slice boundary
    PL-->>SC : Stopped and no further chunks emitted
    SC-->>MA : job CANCELLED and result discarded
    MA-->>MC : Cancelled completion with no partial data
    Note over MA,SC : Stdout carries protocol only and secrets stay in the runtime directory
```

| Element                             | Reference       |
| ----------------------------------- | --------------- |
| Stdio handshake and clean stdout    | FR17, AT14      |
| Cancellation forwarded to scheduler | FR14, PRD 8.5.1 |
| Stop inside an 8 ms slice           | PRD 8.5.5       |

Open questions:

1. Which exact MCP frames carry cancellation is unknown until D08 pins the rmcp version; the
   logical behavior above stays version independent.
2. Whether coalescing matches across clients or only within one client needs a scheduler owner
   decision, since PRD 8.3 allows identical concurrent requests to share an observation window.
   Exact bridge message names may shift after G0; phase structure and guards stay normative.
