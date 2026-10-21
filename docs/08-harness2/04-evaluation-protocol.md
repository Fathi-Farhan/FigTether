# FigTether-Bench Evaluation Protocol

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

Status: `Proposed`

Purpose: This document defines FigTether-Bench, a focused task suite (20 to 30 tasks) for
measuring how well agent harnesses behave when driven by FigTether, and the Harness2-based loop
for improving the harness pack (`docs/08-harness2/03-harness-pack-spec.md`). It inherits the
comparative evaluation ground rules of PRD section 2.2 (frozen configuration, randomized balanced
ordering, honest denominators), the pilot rule of section 2.3 (failed tasks are never removed),
and the evidence semantics of section 8.3. Product performance numbers (N01 to N10) are measured
elsewhere (`docs/06-testing/04-benchmark-plan.md`) and are never mixed into pack metrics.

## 1. Ground rules inherited from the PRD

1. Freeze the fixture set, target commit, Figma version, account context that is eligible, device
   profile, configuration, and the success definition before running anything (PRD 2.2 rule 1).
2. Compare only workloads all compared paths genuinely support; report eligibility or quota
   failure as an access limitation, not as an accuracy failure (rule 2). For pack runs this means
   baseline pack versus refined pack on identical tasks.
3. Separate cold start, first setup, queue wait, cache, fresh read, and export timing here too
   when a task observes them; cached reads never enter a fresh-read denominator (rule 3).
4. Use a randomized or balanced task order and record operator skill and manual interventions
   (rule 4).
5. A human reviewer checks results against the fixture and deliberate changes rather than
   trusting product status messages alone (rule 5).
6. Report every task including blocked, timeout, unsupported, and cancelled, with denominators,
   latency distribution, resource peaks, and the exact commit (rule 6); PRD 2.3 adds: never
   delete failed tasks to make a target.

## 2. Task suite

### 2.1 Categories

Tasks use public fixtures per PRD section 2.1 (small 100 nodes, medium 1000 nodes, large 10000
nodes plus cross-page sizes; nested layouts, mixed runs, variants, aliases and modes, gradients,
masks, image fills, SVG, missing nodes, hidden or locked nodes, non-Latin text, unknown types).
Baseline composition (24 tasks, extendable to 30 as extra slots are added):

| Prefix | Category                    | Minimum | Fixture focus                                                 | Skills under test             |
| ------ | --------------------------- | ------- | ------------------------------------------------------------- | ----------------------------- |
| FBT-A  | Implement-from-frame        | 6       | small and medium with nested layout and mixed runs            | SK-1, SK-2, SK-6, SK-8        |
| FBT-B  | Token extraction            | 5       | medium with variables, styles, aliases and modes              | SK-4, PR-4                    |
| FBT-C  | Asset pipeline              | 4       | small asset-bearing roots, SVG and PNG cases                  | SK-5, PR-8                    |
| FBT-D  | Diff after change           | 5       | small and medium, deliberate change between reads             | SK-3, PR-2, PR-3              |
| FBT-E  | Multi-session investigation | 4       | two sessions and one snapshot on a cross-page medium document | SK-7, PR-6, subagent guidance |

Category editing rule: categories and counts change only between gate windows, never mid-window,
with a manifest version bump and a recorded reason.

### 2.2 Fixtures

Fixture identities are stable pack-local IDs: `FIX-SMALL-100`, `FIX-MEDIUM-1000`,
`FIX-LARGE-10000`, and named variants where a category needs special shapes (for example
`FIX-ALIAS-MODES`, `FIX-ASSET-EDGE`). Properties follow PRD 2.1: clearly licensed, no client
designs, public. Fixture files live under `fixtures/` in the repository with content hashes
recorded in the task manifest; `AT18` style no-mutation checks run before and after each task so
a read round cannot be confused with a fixture change.

### 2.3 Serialization

Each task is a JSONL record; grading expectations stay separate from the executor-visible part.
Example record (fields: id, category, fixture, revision, description reference, and the
observation expectations the agent must satisfy):

```json
{
  "task_id": "FBT-A-03",
  "category": "implement",
  "fixture": "FIX-MEDIUM-1000",
  "fixture_revision": "r1",
  "task": "tasks/FBT-A-03.md",
  "expected_observations": [
    { "field": "observation.observed_at", "req": "present" },
    { "field": "observation.consistency", "req": "label_matches_command" },
    { "field": "coverage.truncated", "req": "present_and_respected" },
    { "field": "coverage.unavailable_fields", "req": "surfaced_not_invented" }
  ],
  "success_definition": "correct component notes + confirmed fields only"
}
```

`expected_observations` name envelope fields from `docs/04-data-contracts/01-mcp-envelope.md`
rather than re-describing payloads, so a contract change (via ADR) updates expectations by
reference. Seed handling and the randomized balanced order: the runner shuffles the task list
with a recorded seed (analogous to upstream `--seed`, which shuffles task order without touching
model sampling [H2]); balance keeps categories and fixtures spread evenly across run halves so
ordering cannot carry the result.

## 3. Grading rubric

Every task is graded on the same five dimensions [H2 uses rubric-style grading on LAB and
JobBench; FigTether fixes its own table below]. Scores are 0, 1, or 2 with anchors; weights in the
last column produce a 0 to 10 task score; pass means 7 or more with zero safety violations.

| Dimension            | 0                                 | 1                                  | 2                                     | Weight |
| -------------------- | --------------------------------- | ---------------------------------- | ------------------------------------- | ------ |
| Correctness          | Invents fields or wrong structure | Mostly right, minor gaps           | Correct against fixture               | 0.40   |
| Freshness compliance | Uses stale as fresh               | Labels correctly but misjudges age | Correct mode, honest metadata (`P03`) | 0.20   |
| Scope compliance     | Skipped or read foreign content   | No leak, but noisy probing         | Clean scope behavior (`FR03`)         | 0.20   |
| Efficiency           | Context flood or thrash           | Acceptable, some waste             | Model-conserving reads (`P04`)        | 0.10   |
| Safety               | Guardrail breach                  | Wording breach only                | Clean (`AT16` tone)                   | 0.10   |

Partial credit follows the LAB practice: report the fraction of scored criteria passed (Partial)
alongside the all-criteria pass rate (All-pass) [H2]. A grading call that fails is unscored, not
zero. "Unsupported" results (for instance a token kind the seat does not expose) can be correct
answers when the task's fixture plan expects them.

## 4. Blocking, blinding, and egress denial

1. Grading data (expected observations, scores, rubrics) lives under
   `benchmarks/figtether-bench/grading/`, which is not part of any executor workspace. The
   executor never sees expectations before its run.
2. Blinding: grading reviewers read sanitized transcripts (session identifiers replaced) without
   knowing which pack version produced them where feasible; ordering is randomized per seed so
   review order does not reveal group assignment.
3. Egress denial: executors run in an environment where external fetches of grading or rubric
   material are blocked before evaluation. Upstream documents that executors have unrestricted
   network access by default and that a Codex rollout once downloaded its own task's rubric
   without being asked [H2, EXPERIMENTS]; FigTether applies the same lesson locally (document 05,
   section 8).
4. Model endpoints remain reachable; the FigTether bridge stays loopback-only. Deny-listing model
   providers would be a measurement error, not a safety feature.

## 5. Reporting and denominators

Every run publishes one report with: the exact commit, pack versions (baseline and refined),
executor configuration, seed and ordering balance proof, deny rules in force, per-task outcome,
and per-category aggregates.

| Outcome                                             | In denominator | Note                            |
| --------------------------------------------------- | -------------- | ------------------------------- |
| passed (score >= 7, no safety violation)            | yes            | Primary metric                  |
| failed (score below threshold, or safety violation) | yes            | Never removed from the report   |
| blocked (environment, access, or quota)             | yes            | Count by reason                 |
| timeout (deadline exceeded)                         | yes            | Count                           |
| unsupported (expected feature missing by design)    | yes            | Counted per PRD 2.3             |
| cancelled (operator or user action)                 | yes            | Count                           |
| unscored (grading call failed)                      | yes            | Reported separately, never zero |

Denominators are always per category plus overall. Any harness comparison that would require
dropping one of the rows above from a denominator is invalid by PRD 2.3.

## 6. Harness2 loop mapping

The loop mirrors upstream with FigTether-Bench slotting into the benchmark role [H2]:

| Element                   | Upstream analogue                               | FigTether setting                                                                                             |
| ------------------------- | ----------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| Benchmark                 | JobBench, LAB, WorkBuddy-Bench                  | FigTether-Bench tasks under `benchmarks/figtether-bench/tasks/`                                               |
| Domains                   | benchmark practice areas, subsets               | The five FBT categories                                                                                       |
| Baseline harness          | reference executions with the base harness      | Pack v0 sessions, evaluated optionally via the upstream baseline judging flag (`harness2-judge --bases`) [H2] |
| Budget                    | `--k`                                           | Sweep k of 1, 2, and 3 in the pilot window                                                                    |
| Modes                     | `parallel`, `sequential`                        | Run at least one of each before any mode claim                                                                |
| Executor                  | OpenCode by default; Codex optional             | Fix one executor across a comparison; record it                                                               |
| Ledger (task level)       | adopted harnesses and contrasts                 | Probe records under `benchmarks/figtether-bench/runs/` (probe transcripts and pack versions)                  |
| Ledger (harnessing level) | improvement harness lessons                     | `benchmarks/harness-pack/lessons/` journal, separate per category, executor, mode, k, and seed [H2]           |
| Runs root                 | `HARNESS2_RUNS_DIR` default `runs/`             | One root per configuration, named `runs/fbt-<executor>-k<k>-<mode>-seed<n>/`                                  |
| Judging parity            | same benchmark, executor, subset, k, mode, seed | Mandatory before any delta is published                                                                       |

Loop discipline: refinement, judging, and reporting must resolve to the same configuration triple
(benchmark, executor) plus k, mode, and seed; `--dry-run` previews task selection before any
model spend. Failed refinements and failed judge calls are reported as unscored, never zeroed.

## 7. Success metrics for pack improvement

All pack metrics are relative deltas against the pack v0 baseline, measured on the same frozen
tasks:

1. Correctness: task pass rate per category and overall.
2. Freshness compliance: share of mode-sensitive tasks with no mislabeled mode, no silent stale
   acceptance, and no age misuse.
3. Scope compliance: count of scopeicy violations or foreign-root reads, expected at zero.
4. Efficiency: median prompt or context bytes per task at equal correctness, plus edit distance
   toward cursor continuation instead of re-reads.
5. Safety: zero tolerance on PR-1 to PR-10 breaches; any breach voids a pass regardless of score.
6. Per-tool waste: repeated identical calls per task when a cursor already existed.

Separation rule: these are harness metrics. They do not substitute, re-state, or re-base any PRD
target (`N01` to `N10`, measured by `docs/06-testing/04-benchmark-plan.md`), and PRD reports never
import pack numbers. One shared item only: both tracks record the exact commit and configuration
to make evidence comparable over time.

## 8. Cadence per gate

- G0: baseline pack v0 (`docs/08-harness2/03-harness-pack-spec.md` contents, hand authored) and
  the FigTether-Bench task manifest may be written now, as docs work. This is not a PRD G0 exit
  criterion (`AT01`, `AT02`, `AT12`, `AT18` remain the gate owners).
- G1: first iteration window. Once `E07` exposes the MCP surface, probe pack candidates on
  fixtures; compose pack v1 with an evidence appendix; lessons journal starts.
- G2: the pilot report uses the refined pack; published pack deltas reviewed under `AT16`;
  FigTether-Bench results recorded with full denominators.
- G3, G4: optional windows for pack v2 with the lessons journal input; offline bundle (`FR21`)
  gains a pack section when it lands.

## 9. Publication rules

Tied to `AT16` and `P14`:

1. Never publish unlimited or always-working claims about the harness pack.
2. Every published delta carries the commit hash, configuration, and denominator table.
3. Upstream Harness2 numbers (document 01, section 8) may be cited only with their own
   attribution and never implied to transfer to FigTether.
4. A pack claim becomes marketing only after G2 evidence makes it; before that the word
   `experimental` on the manifest governs.

## 10. Network restriction configuration (before evaluation)

1. Identify everything an executor could fetch that would leak grading data: for upstream
   benchmark calibration, upstream names `huggingface.co`, `hf.co`, `*.hf.co`, `github.com`, and
   `raw.githubusercontent.com` as the hosts to block for executors [H2, EXPERIMENTS]; for
   FigTether-Bench, keep all grading material local and block anything that would make it
   fetchable.
2. Enforce at the network level: egress firewall or DNS policy on the executor host. Upstream runs
   Codex with its sandbox disabled, so tool-level policy is not a reliable control [H2].
3. Keep setup traffic separate: upstream setup legitimately downloads from those hosts; configure
   the deny rule after setup, before the first executor run (`harness2-setup` versus
   `harness2`).
4. Verify with a deny test: an executor probe that tries a blocked host must fail.
5. Record the deny rules in force with the result report; unrecorded restrictions are treated as
   restrictions absent.

## 11. Upstream benchmark calibration (optional, attribution)

Before trusting our own loop, the maintainer may replay one small upstream run (document 01
section 7 commands) to validate the evaluation environment end to end. Any such replay is
attributed to upstream and is never reported as a FigTether result. This step is optional and
cost-bearing; model-backed runs incur API costs per upstream guidance [H2].

## 12. Traceability

| Protocol element                               | PRD anchor                 |
| ---------------------------------------------- | -------------------------- |
| Frozen configuration rule                      | PRD 2.2 rule 1             |
| Balanced randomized order                      | PRD 2.2 rule 4             |
| Honest denominators, blocked and timeout rows  | PRD 2.2 rule 6, PRD 2.3    |
| No failed-task removal                         | PRD 2.3                    |
| Public fixtures                                | PRD 2.1 fixture paragraphs |
| Evidence semantics for fresh, cached, snapshot | PRD 8.3                    |
| No mutation during reads                       | `FR24`, `AT18`             |
| Claims discipline                              | `P14`, `AT16`              |
| Harness methodology                            | [H2] paper and repository  |
