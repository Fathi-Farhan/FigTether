# ADR-001: Rust core with a thin TypeScript plugin adapter

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

| Field                | Value         |
| -------------------- | ------------- |
| Status               | Accepted      |
| Created              | 2026-10-20    |
| Related decision IDs | D01, D02, D03 |
| Supersedes           | -             |

## Context

The Figma Plugin API is JavaScript-only (`S4`, `S7`), so the piece that touches the document is
TypeScript by force. `D01`, decided in the PRD, splits the product in two: a Rust core that holds
all domain policy, and a TypeScript adapter that exists only inside the Figma plugin. The driver
is `P01` and `P02`: the core path must avoid the official MCP server and REST so agents stop
consuming seats or quota (`D02`, `FR19`, `AT01`, `AT02`). PRD 8.4 fixes the principle as "core
holds business rules; adapter holds editor object access", says TypeScript produces
schema-validated DTOs without duplicated domain policy, keeps scope enforcement on both sides to
survive bridge input and bugs (`FR03`), and decides explicitly that WASM is not a v0.1
requirement because it adds build and runtime complexity without removing the JavaScript binding
need. `D03` keeps the whole chain read-only.

## Decision

1. Rust owns and implements: daemon lifecycle, MCP adapter (`figtether mcp`), CLI, pairing and
   auth, grant registry, scheduler, consistency and cache policy, pagination cursors, normalizer,
   asset store, error taxonomy, and resource budgets. Workspace sketch per PRD 8.4:
   `crates/{cli,daemon,mcp,core,protocol,model,assets}`, one binary exposing subcommands; crate
   boundaries may simplify as long as the domains stay separated.
2. TypeScript exists only in `plugins/figma/main` (sandbox) and `plugins/figma/ui` (iframe). It
   may: run allowlisted Plugin API read handlers, encode and decode schema-validated bridge frames
   (8.5.4), forward results through validated postMessage, and render the pairing, grant, and
   status UI.
3. TypeScript never holds domain policy: no scope rules, cache policy, freshness rules, or budget
   constants. The plugin re-verification demanded by `FR03` double enforcement consumes values
   carried in the frozen schemas; it does not invent local rules.
4. Rust never touches Figma REST endpoints or the official Figma MCP server: no adapter exists in
   core. `JSON_REST_V1` from `exportAsync` (`S4`) is an optional adapter input on the plugin side,
   never a core data source (`FR08`, `FR19`).
5. WASM-for-core is rejected for v0.1, following the explicit PRD 8.4 decision.

## Rationale

One policy home prevents drift: scope, epochs, limits, and freshness each have a single source,
while the plugin still re-checks per request so a plugin bug cannot widen access and a core bug
cannot read the editor behind the user's grant (fail-closed, committed interpretation 2 in
`00-overview/01-prd-analysis.md` section 10). Rust delivers the prebuilt single binary `FR01`
demands, bounded queues and memory for `FR13` and `FR14`, the `N04`/`N05` stability targets, and a
lockfile-based supply chain per 8.11. Keeping the adapter thin keeps it auditable: reviewers check
an allowlist, not a second brain.

## Consequences

- Positive: policy twins are testable in cross-language fixtures; the core path is provable free
  of official MCP and REST (`AT01`, `AT02`, `N09`); the TS surface stays small enough for scope
  review.
- Negative: every capability crosses a schema boundary, so two-way contract tests are mandatory
  (8.5.4); Rust and TS work must share one frozen contract after G0.
- Neutral: WASM remains openable by a later ADR if editor-side needs change; then the daemon copy
  of core still has to be justified for MCP and CLI.

## Alternatives considered

- WASM core compiled into the plugin: rejected for v0.1 per PRD 8.4; JS glue to the Plugin API
  remains, build complexity grows, and a daemon implementation is still needed for MCP and CLI.
- All-TypeScript core (a Node helper process): rejected; `FR17` stdio MCP and `FR01` (no Node
  install for end users) point at a Rust binary, and policy would be duplicated across languages.
- Rust wrapper around the official Figma MCP server: rejected outright; it re-anchors the product
  on quota and seat eligibility (`P01`, `S1`, `FR19`, `AT01`).
- CLI subprocess invocation per request instead of a daemon: rejected; pairing state, scheduler,
  cache, and heartbeats require one resident process (8.4 state machine).

## Verification

Evidence: G0 build and size report for the Rust binary plus plugin bundle (closure owner of
`D01`); egress audit and deny-egress capture at the exact commit; two-way schema contract fixtures
green (8.4, 8.5.4); handler allowlist review recorded.
AT refs: AT01, AT02 (path separation), AT05 (grant isolation), AT18 (read-only probe).
Gate: G0 (build, handshake, path proof), G1 (double enforcement fixtures), G2 (release audit at
exact commit).

## References

- PRD 3 (`S4`, `S7`), 4.3 (`FR01`, `FR03`, `FR08`, `FR19`, `FR24`), 8.4, 8.5.4, 8.5.5.
- `D01`, `D02`, `D03`; open questions section 7 in `docs/00-overview/01-prd-analysis.md`.
- ADR-003 (MCP SDK), ADR-008 (budget constants live in Rust), `docs/04-data-contracts/` (frozen
  bridge and tool schemas).
