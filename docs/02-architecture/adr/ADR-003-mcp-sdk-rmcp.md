# ADR-003: Official rmcp SDK with pinned version and MSRV

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

| Field                | Value      |
| -------------------- | ---------- |
| Status               | Proposed   |
| Created              | 2026-10-20 |
| Related decision IDs | D08        |
| Supersedes           | -          |

## Context

`FR17` requires MCP over stdio with a handshake validated against at least two MCP clients, where
stdout carries protocol only. `R09` warns about MCP client and SDK fragmentation, and PRD 8.4
states that the official Rust MCP SDK exists (`S8`) but that version and capability choices must
be tested against target clients rather than following the main branch at runtime. `D08` defers
the exact version, MSRV, and client conclusions to kickoff G0. The MCP adapter belongs to the Rust
core boundary set by `D01` and ADR-001, so the SDK choice is a core supply-chain decision, not a
tooling preference.

## Decision

1. `figtether mcp` implements the MCP server surface on the official `rmcp` SDK crates from the
   modelcontextprotocol/rust-sdk project (`S8`). No fork at v0.1.
2. Version pinned at G0 kickoff: record the crate names, the exact released versions, and the
   enabled feature flags; lock them in `Cargo.lock`; record the MCP protocol version the pin
   negotiates in the compatibility matrix.
3. MSRV rule: the project MSRV equals the lowest stable Rust toolchain that is actually tested
   building and passing the test suite with the pinned SDK; state it in the workspace manifest.
   Never follow `main` or nightly for the SDK, and never raise the MSRV without a tested reason.
4. Upgrade rule: SDK bumps travel through an ADR addendum with fresh two-client evidence, per 8.11
   dependency review; silent upgrades are release blockers.
5. Acceptance at G0: initialize, tools listing, resources, and cancellation all pass with two
   representative MCP clients selected at kickoff and recorded in the compatibility matrix
   (`AT14`, `FR17`).
6. Capability reconciliation: server capabilities advertised are only those the adapter truly
   implements (`tools`, `resources`, cancellation in v0.1); features a connected client does not
   support are disabled with a structured message, never misreported (`R09`).
7. Tool metadata carries `readOnlyHint=true` and `destructiveHint=false` on every read tool, with
   hints treated as advisory, never as authorization (8.5.1).

## Rationale

The official SDK tracks the MCP spec, which reduces protocol drift and lets the adapter spend
review time on product semantics (`R09`). A pinned release plus a tested MSRV floor is the 8.11
supply-chain pattern (lockfiles, pinned versions, audit, SBOM) applied to the dependency most
likely to move under us. Following `main` would make every build an experiment and destroy the
reproducibility that `FR18` diagnostics need; two-client evidence is what the PRD demands for
handshake truth.

## Consequences

- Positive: predictable protocol behavior across releases; one MSRV floor for contributors;
  cancellation and resource negotiation come from the SDK rather than hand-rolled code.
- Negative: spec enhancements wait on SDK releases; a security fix in the SDK still ships only
  after the pinned-review step.
- Neutral: `rmcp` is a dependency of the `mcp` crate; other crates depend on the protocol model,
  not on the SDK facade.

## Alternatives considered

- Hand-rolled JSON-RPC and MCP layer: rejected; high maintenance cost and exactly the fragment
  risk `R09` punishes, with cancellation and schema edge cases easy to get wrong.
- Community-maintained MCP crates as the primary: rejected; acceptable only as a fallback if the
  official crate fails G0 acceptance, and then via a new ADR.
- Shipping the reference Node MCP server implementation: rejected; violates `D01` and ADR-001
  (Rust core, TS adapter only) and adds a Node install for end users, breaking `FR01`.
- Embedding or proxying the official Figma MCP server: rejected outright; that is the quota-bound
  path the product exists to replace (`P01`, `P02`, `FR19`, `AT01`, `AT02`).

## Verification

Evidence: G0 kickoff record naming the pinned SDK versions, MSRV, and the two target clients;
handshake logs with clean stdout/stderr separation on both clients; a two-client soak entry in the
`N05` procedure; compatibility matrix row for the pinned protocol version.
AT refs: AT14 (handshake, tools, resources, cancellation, clean stdout), N05 (soak with real
clients).
Gate: G0 (pin plus handshake) then G2 (AT14 in release evidence); the untested-pin state cannot
ship because `D08` closure owns that evidence.

## References

- PRD 4.3 (`FR17`, `FR18`, `FR01`), 8.4 (observability and SDK note), 8.5.1 (tool hints), 8.9
  (`R09`), 8.10.1 (SDK pinned on tested version), 8.11 (supply chain), Appendix C (`S8`).
- `D08`; open question 3 in `docs/00-overview/01-prd-analysis.md` (capability negotiation versus
  client matrix).
- ADR-001 (core boundary), ADR-008 (budgets independent of SDK behavior), ADR-011 (structured
  tool results inside the SDK surface).
