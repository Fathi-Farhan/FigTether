# Harness2 Overview: Recursive Agent Harnessing

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

Status: `Proposed`

Purpose: This document is the entry point of `docs/08-harness2/`. It explains the Harness2
methodology (Google Research) at the depth FigTether needs to adopt it, separates upstream facts
from FigTether plans, and links the other documents in this folder: the integration architecture
(`docs/08-harness2/02-integration-architecture.md`), the harness pack spec
(`docs/08-harness2/03-harness-pack-spec.md`), the evaluation protocol
(`docs/08-harness2/04-evaluation-protocol.md`), and the operator runbook
(`docs/08-harness2/05-runbook.md`). Nothing here changes tool, envelope, or source contracts in
`docs/04-data-contracts/`; those remain frozen after `G0` per the PRD (section 8.5, `D08`).

## 1. Why FigTether cares

An agent that consumes FigTether is always more than its model. What makes the agent useful is the
system around the model: the instructions it reads, the tool surface it can call, the memory it
keeps, the skills it loads, and the middleware that shapes its context. The Harness2 paper calls
this system the harness and calls the activity of improving it harnessing [H2].

FigTether's MCP tools (`docs/04-data-contracts/02-tool-catalog.md`) are one slice of a client
harness. The remaining slices decide whether the agent behaves well: freshness discipline, summary
before detail, pagination, honest handling of `truncated`, and refusal to invent missing data.
PRD problems `P04` (giant JSON floods agent context), `P06` (wrong tab confusion), and `P08`
(agents invent information that was never available) are harness failures as much as product
failures. FigTether therefore ships a harness pack so that any client harness driven by FigTether
stays disciplined, and adopts the Harness2 recipe to keep improving that pack without retraining
models.

Upstream summary quoted as fact from the repository README [H2]: Harness2 is a plug-and-play
framework for recursive agent harnessing in an open world, where agents meet tasks unseen during
harness design, and it connects two levels of recursion while keeping model weights fixed.

## 2. The two recursions

### 2.1 Task-level recursion: Propose-Probe-Compose

Per the paper [H2], task-level recursion refines the execution harness for the current task
through a contrastive Propose-Probe-Compose step:

1. Propose: edit the harness. Upstream distinguishes invariant edits (edits that should hold on
   every execution, for example "never answer from expired cache") and domain edits (edits tied to
   the workload, for example "spend the first call on a summary before detail").
2. Probe: candidate probes execute the edited harness on the same task. Executions are compared.
3. Compose: a refined harness is composed from behavioral contrasts between executions, without
   ground-truth feedback. No answer key is needed at this stage; observable differences between
   candidate runs are the evidence.

FigTether adoption of this step is a maintainer-side workflow acting on our pack, not on the
model; see `docs/08-harness2/02-integration-architecture.md` section 3.

### 2.2 Harnessing-level recursion: the improvement harness

Per the paper [H2], harnessing-level recursion updates a persistent improvement harness: a store
that keeps adopted harnesses plus evidence-supported lessons from each task, and uses them to
guide future refinement. After a task is adopted, reflection writes the adopted harness and the
lessons into a per-domain library. Tasks inside a domain run in order and read only earlier
entries; different domains can run concurrently. Upstream separates such libraries by benchmark,
executor, mode, budget, and seed (`docs/08-harness2/04-evaluation-protocol.md` section 6).

FigTether adoption: a lessons journal for the pack lives next to the pack and feeds future pack
versions; see `docs/08-harness2/03-harness-pack-spec.md` section 7 and
`docs/08-harness2/04-evaluation-protocol.md` section 6.

## 3. The eight editable components

Harness2 exposes eight editable components around the model [H2]. The grouping below is upstream;
the right column previews FigTether's answer (full mapping in document 02).

| Runtime role               | Components                                | What it controls                                     | FigTether provision                                                  |
| -------------------------- | ----------------------------------------- | ---------------------------------------------------- | -------------------------------------------------------------------- |
| Construct context          | system prompt, guardrails, memory, skills | What the model sees and believes before acting       | Pack files under `benchmarks/harness-pack/` (document 02, section 2) |
| Define the tool interface  | tool guidance, scripts                    | How tools are called and what local operations exist | FigTether MCP tools plus thin CLI wrappers                           |
| Orchestrate delegated work | subagents                                 | How work is split across agent sessions              | Pack delegation guidance                                             |
| Control middleware         | plugins                                   | Context shaping inside the tool loop                 | Response redaction and freshness enforcement guidance                |

Roles, wide reading: system prompt, guardrails, memory, and skills construct context; tool
guidance and scripts define the tool interface; subagents orchestrate delegated work; plugins
control middleware. All eight are editable with weights fixed [H2].

## 4. Executors

Upstream executes harness runs through an agent executor:

- OpenCode is the default executor (upstream default; used for baseline and refinement runs).
- Codex is optional via `--substrate cx`; upstream runs Codex with its sandbox disabled and
  exposes a Responses API endpoint through a local bridge (`harness2-bridge`).

Both executors run with unrestricted network access upstream [H2, EXPERIMENTS]. Before evaluation,
graders must configure network restrictions so the executor cannot reach grading data. The
upstream experiment guide records that in a validation run a Codex rollout downloaded its own
task's rubric from a public host without being instructed to. FigTether applies the same rule to
every harness run, including our own benchmark runs. Configuration guidance lives in
`docs/08-harness2/05-runbook.md` section 8.

## 5. Scaling modes and budget

Upstream scales task-level recursion two ways [H2, EXPERIMENTS]:

| Option                          | Behavior (upstream)                                                                                                                                             |
| ------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--k 1`                         | One Propose-Probe-Compose step, followed by reflection after adoption                                                                                           |
| `--k K --mode parallel`         | Probe K general (invariant) candidates and K domain-specific candidates; each of K independent compositions uses the full candidate pool and execution evidence |
| `--k K --mode sequential`       | Refine the harness through K successive Propose-Probe-Compose steps, carrying the refined harness and accumulated evidence into the next step                   |
| `--substrate cx`                | Use Codex instead of the default OpenCode executor                                                                                                              |
| `--seed N`                      | Shuffle the task stream reproducibly; does not control model sampling                                                                                           |
| `--workers N`                   | Run N domains concurrently; tasks within each domain stay ordered                                                                                               |
| `--only-domains`, `--max-tasks` | Restrict the run to named domains; cap tasks per domain, not per run                                                                                            |
| `--dry-run`                     | Preview task selection without model calls                                                                                                                      |

Upstream framing: parallel recursion explores wider; sequential recursion refines deeper. Each run
root (`HARNESS2_RUNS_DIR`, default `runs/`) should isolate one configuration so that a later judge
or report command reads the same configuration it was produced with.

## 6. Upstream benchmarks

Upstream ships three professional benchmarks; each installs separately [H2, BENCHMARKS, SETUP]:

| Benchmark       | CLI name | Work domain                                             | Install                          | Extra dependencies upstream needs                                                 |
| --------------- | -------- | ------------------------------------------------------- | -------------------------------- | --------------------------------------------------------------------------------- |
| JobBench        | `jb`     | Professional work                                       | `harness2-setup jb`              | Hugging Face dataset (public, ungated); document extraction deps                  |
| LAB             | `lab`    | Legal work                                              | `harness2-setup lab`             | Pandoc and Poppler                                                                |
| WorkBuddy-Bench | `wb`     | Office, web, and code (subsets `office`, `web`, `code`) | `harness2-setup wb --subset <s>` | Docker Engine; upstream Tencent license (territorial limitation, read before use) |

Judging style varies upstream and FigTether takes the honest parts: LAB reports Partial (fraction
of criteria passed) and All-pass (all criteria passed), with failed judge calls remaining unscored
rather than zero; JobBench scores by upstream rubric weights and reports unscored tasks separately.
Changing the judge model changes the measurement, so judge configuration must be held fixed while
comparing runs.

The benchmark code names, setup commands, subsets, and dependency needs above describe upstream
tooling, not FigTether requirements. FigTether ships Rust and TypeScript; Python appears only in
this folder's evaluation workflow. See `docs/08-harness2/05-runbook.md` section 1.

## 7. Upstream install and run examples

These commands are upstream usage examples, cited for context. Software dependencies listed here
are needed by the harness2 evaluation environment, not by FigTether users or the FigTether build.

Linux, Python 3.12, uv from the harness2 repository root [H2, README]:

```
uv venv --python 3.12 .venv
source .venv/bin/activate
uv pip install -e '.[render]'
harness2-demo --output runs/demo --k 2
```

The local demo runs both recursion modes with deterministic editors and no model credentials,
which makes it the first credibility check on any harness2 installation.

Model-backed runs additionally need Node.js 20 or newer and a Google Cloud CLI login [H2, SETUP]:

```
cp .env.example .env
gcloud auth application-default login
```

One benchmark end to end [H2, BENCHMARKS]:

```
harness2-setup lab
harness2-check --bench lab
harness2 --bench lab --k 1 --mode parallel --only-domains antitrust-competition --max-tasks 1
harness2-judge --bench lab --k 1 --mode parallel
harness2-report --bench lab --k 1 --mode parallel
```

`harness2-check` verifies dependencies, credentials, and configuration without model calls;
refinement runs repeat these checks before starting and exit nonzero when incomplete.

## 8. Upstream results and attribution

Reported by the Harness2 authors on their benchmarks and configurations [H2]: a single Harness2
step improves GLM-5 and Gemini 3.5 Flash over their base harnesses, with gains of up to 11.5
points on WorkBuddy-Bench, 4.5 points on JobBench, and 11.8 percentage points on LAB's partial
pass rate; increasing the task-level recursion budget brings further gains.

Attribution rules for FigTether documents and marketing:

1. These numbers belong to the upstream authors. FigTether has no measured pack deltas yet.
2. FigTether claims may reference the method ("harnessing keeps weights fixed and improved
   upstream benchmarks") but never blend upstream deltas into FigTether expectations.
3. FigTether pack measurements land later, as G2 evidence, per
   `docs/06-testing/04-benchmark-plan.md` and `docs/08-harness2/04-evaluation-protocol.md` section 8. Until then any pack claim stays `experimental` (PRD definition of ready, section 8.10).

## 9. License and disclaimer

Upstream facts (README of the harness2 repository [H2]):

- License: Apache 2.0. Third-party code under `third_party/`, benchmarks, and datasets retain
  their upstream licenses.
- Disclaimer: not an officially supported Google product; not eligible for the Google open source
  vulnerability rewards program.

FigTether usage stance: we adopt the published methodology and cite it; we do not import harness2
code into FigTether at runtime, and if any code or fixture were ever copied we would keep the
upstream license and attribution per the PRD open source rules (section 8.11).

Plain text citation (adapted; subscript removed for our style rules):

Xu, Ruiyao; Chen, Yanfei; CuiZhu, Zhongying; Dalvi Mishra, Bhavana; Ming, Yifei; Yu, Han; Han,
Rujun; Lee, Chen-Yu; Pfister, Tomas. Harness2: Recursive Agent Harnessing for an Open World.
arXiv:2610.14224, 2026. Repository: https://github.com/google-research/harness2

## 10. How it maps onto FigTether today

FigTether already implements the tool interface slice. Envelope fields
(`schema_version`, `source`, `observation`, `coverage`, `warnings`) in
`docs/04-data-contracts/01-mcp-envelope.md` make harness-grade discipline auditable: every result
states its age (`cache_age_ms`), its coverage (`truncated`, `next_cursor`, `unavailable_fields`),
and its origin (`source.kind`, session and grant epoch). The remaining seven harness slots are
filled by the pack that FigTether ships:

| Harness slot                             | Filled by                                              | Where                                                     |
| ---------------------------------------- | ------------------------------------------------------ | --------------------------------------------------------- |
| Tool interface (tool guidance + scripts) | MCP tool catalog plus CLI operations guidance          | `docs/04-data-contracts/02-tool-catalog.md`; `crates/cli` |
| Skills                                   | figtether skill playbooks                              | `benchmarks/harness-pack/skills/figtether/`               |
| Guardrails                               | freshness and scope rules mirroring envelope semantics | `benchmarks/harness-pack/guardrails/`                     |
| Memory                                   | observation ledger schema                              | `benchmarks/harness-pack/memory/`                         |
| System prompt                            | usage etiquette guidance                               | `benchmarks/harness-pack/system/`                         |
| Subagents                                | delegation patterns and capacity rules                 | `benchmarks/harness-pack/subagents/`                      |
| Plugins (middleware)                     | response shaping, redaction, freshness enforcement     | `benchmarks/harness-pack/plugins/`                        |
| Harnessing (method)                      | Propose-Probe-Compose refinement of the pack           | `docs/08-harness2/02-integration-architecture.md`         |

The pack targets failure modes the PRD named: stale decisions from unlabeled data (`P03`, `R03`),
context flooding (`P04`), wrong-tab reads (`P06`), invented fields (`P08`), and unbounded claim
language (`P14`).

## 11. Running it

To run Harness2 experiments against the FigTether workspace, follow the operator runbook:
`docs/08-harness2/05-runbook.md`. In one sentence: set up the upstream environment, point the
OpenCode executor at the FigTether workspace via the MCP entry printed by
`figtether config print --client generic`, run the loop
(`harness2-setup`, `harness2-check`, `harness2`, `harness2-judge`, `harness2-report`, or
`--dry-run` first), and keep results isolated per configuration under `runs/`.

## 12. References

1. [H2] harness2 repository, README, BENCHMARKS.md, EXPERIMENTS.md, SETUP.md (upstream repo paths),
   at https://github.com/google-research/harness2 (accessed 2026-10-20).
2. Paper: Harness2: Recursive Agent Harnessing for an Open World. arXiv:2610.14224, 2026.
3. FigTether PRD v1.1 (`FGT-PRD-001`), sections 2.2, 2.3, 8.3, 8.5, 8.6, 8.10.
4. `docs/00-overview/01-prd-analysis.md` (committed interpretations) and
   `docs/06-testing/04-benchmark-plan.md` (product benchmark track).
