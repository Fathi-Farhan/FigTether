# ADR-000: ADR template, usage rules, and numbering policy

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

| Field                | Value      |
| -------------------- | ---------- |
| Status               | Accepted   |
| Created              | 2026-10-20 |
| Related decision IDs | -          |
| Supersedes           | -          |

## Context

`docs/README.md` change management requires an ADR before a contract-affecting change, and PRD
8.10.1 (Definition of Ready) requires schema, grant, credential, and resource-limit decisions to
be approved through an ADR maintainer. The PRD decision register `D01..D12` names topics and
closure owners, but the PRD does not define what an ADR contains or how ADRs are numbered. Without
a fixed shape, each ADR invents its own sections, reviews lose comparability, and traceability
from decision to test evidence becomes inconsistent.

## Decision

Every ADR in `docs/02-architecture/adr/` uses the structure below. Angle brackets are placeholders
and must never survive into a published ADR.

```markdown
# ADR-NNN: <Title>

| Field                | Value      |
| -------------------- | ---------- |
| Status               | Proposed   |
| Created              | YYYY-MM-DD |
| Related decision IDs | DXX, DYY   |
| Supersedes           | -          |

## Context

What forces the decision now, with PRD IDs cited verbatim.

## Decision

The rule that binds code and docs. Normative, numbered sentences.

## Rationale

Why this rule and not the alternatives, with PRD sections and committed interpretations.

## Consequences

- Positive: ...
- Negative: ...
- Neutral: ...

## Alternatives considered

Each alternative with the concrete reason it was rejected.

## Verification

Evidence: named artifact, test, or benchmark.
AT refs: acceptance tests that observe the decision.
Gate: G0 through G4 point where the evidence must exist.

## References

PRD sections, source register IDs (S1..S11), related ADRs and docs paths.
```

Usage rules:

1. One decision per ADR. A second decision needs a second ADR and a cross reference.
2. Write the ADR before implementation. A decision without gate evidence on record stays
   `Proposed` even when the PRD language itself is decided.
3. Status values are `Proposed`, `Accepted`, and `Superseded`. `Accepted` records PRD-decided
   semantics or closure evidence; it never waives the verification gate.
4. IDs are copied verbatim from the PRD (`G0..G4`, `E01..E10`, `FR01..FR24`, `N01..N10`,
   `AT01..AT20`, `P01..P14`, `R01..R14`, `D01..D12`, `S1..S11`). Never reinterpret an ID.
5. English only. No em or en dashes anywhere; plain hyphens only.
6. Length: keep an ADR between 60 and 120 lines. Push detail down to dedicated documents
   (contracts in `04-data-contracts/`, tests in `06-testing/`) instead of inflating the ADR.
7. When a decision changes, write a new ADR that names the old one in `Supersedes`; never rewrite
   a superseded ADR history after the fact.

Numbering policy:

- `ADR-000` is this process record. The initial decision set is `ADR-001` through `ADR-012`,
  numbered in the order of the PRD section 8 decision topics and this doc set writing plan.
- New ADRs take the next unused number. Numbers are never reassigned, reused, or reordered.
- Superseded ADRs keep their number and file; their Status row moves to `Superseded` with a
  pointer to the successor in that row.

## Rationale

A fixed shape makes review mechanical: reviewers check the same seven sections in the same order
and can compare decisions at a glance. The 60 to 120 line budget keeps records reviewable inside
PRs and matches the constraint used for the initial set, pushing detail into documents that already
own that responsibility. Verbatim IDs preserve the traceability discipline of
`00-overview/01-prd-analysis.md` section 11.

## Consequences

- Positive: uniform reviews; each decision has an owner, a gate, and a test mapping.
- Negative: rigid sections can feel awkward for process-only records such as this one.
- Neutral: Status transitions are also tracked in `99-appendices/03-approval-record.md` per README.

## Alternatives considered

- Free-form ADRs: faster to start, but review depth and Supersedes handling diverge per author.
- MADR-style heavy template: too much ceremony for a small pre-implementation team with one PRD.
- ADRs inside each design doc: rejected; README requires the dedicated `02-architecture/adr/`
  folder so contracts and decisions are addressable independently.

## Verification

Evidence: each contract PR contains an ADR with all seven sections present, verbatim IDs, and
length within budget; this is a review checklist item, not an automated test.
AT refs: none (process record; distributes test coverage to ADR-001 through ADR-012).
Gate: standing check at every gate review from G1 onward.

## References

- PRD 4.3, 8.5 (contract freeze after G0), 8.10.1, 8.11, 8.12.
- `docs/README.md` (conventions and change management), `docs/00-overview/01-prd-analysis.md`
  section 8 (decision register D01..D12).
