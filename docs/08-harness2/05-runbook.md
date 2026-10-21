# Harness2 Runbook for FigTether

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

Status: `Proposed`

Purpose: This document is the developer runbook for running Harness2 experiments against the
FigTether workspace: preparing the environment, pointing an OpenCode executor at FigTether, and
executing the run loop (`harness2-setup`, `harness2-check`, `harness2`, `harness2-judge`,
`harness2-report`) with `--dry-run` previews. The rule that governs everything here: FigTether
runs harness2 to evaluate the harness pack (`docs/08-harness2/03-harness-pack-spec.md`); the
FigTether core never depends on harness2 at runtime, and no Python code is shipped in the product.

## 1. Scope and non-dependency

- harness2 belongs to the developer evaluation workflow (`docs/08-harness2/04-evaluation-protocol.md`).
- FigTether distribution (`FR01`), MCP surface (`FR17`, `AT14`), and contracts
  (`docs/04-data-contracts/`) work identically without harness2.
- Commands in this document are upstream usage examples [H2]; they are cited so a maintainer can
  reproduce them, not as product requirements.
- Product benchmark numbers and pack numbers stay in separate tracks (document 04, section 7).

## 2. Environment prerequisites

Upstream requires Linux for benchmark runs, with Python 3.12 (the package also supports 3.13)
[H2, SETUP]. On a Windows host, use WSL2 with the FigTether workspace reachable under the usual
`/mnt/...` mount path; keep the harness2 checkout, venv, and `runs/` directory inside the Linux
filesystem for performance.

| Requirement             | Needed when          | Notes                                                       |
| ----------------------- | -------------------- | ----------------------------------------------------------- |
| Linux (or WSL2)         | always               | Upstream benchmark setup is Linux-first                     |
| Python 3.12 + uv        | always               | `uv venv --python 3.12 .venv`; Python package supports 3.13 |
| Node.js 20 or newer     | model-backed runs    | Required by the upstream executor tooling                   |
| Google Cloud CLI        | model-backed runs    | Login plus keeping `gcloud` on `PATH`                       |
| Pandoc + Poppler        | LAB only             | `harness2-setup lab` needs both                             |
| Docker Engine           | WorkBuddy-Bench only | Docker permission such that `docker info` works             |
| FigTether local install | FigTether runs       | Prebuilt binary plus plugin bundle, or a local dev build    |

LAB, JobBench, and WorkBuddy-Bench each install their own pinned environments inside their
upstream checkouts; setup handles interpreters automatically, so do not activate benchmark
virtual environments manually [H2, SETUP].

## 3. Obtain and install harness2 (upstream commands)

In a dedicated evaluation directory, outside the FigTether product tree:

```
git clone https://github.com/google-research/harness2.git
cd harness2
uv venv --python 3.12 .venv
source .venv/bin/activate
uv pip install -e '.[render]'
harness2-demo --output runs/demo --k 2
```

The demo runs both recursion modes with deterministic editors and no model credentials [H2]; it
validates the toolchain before any model spend. Reference the upstream README for the render extra
and for `harness2-bridge` (needed only for the optional Codex executor path:
`uv pip install -e '.[render,bridge]'`).

## 4. Credentials and .env

Copy `.env.example` to `.env` and authenticate [H2, SETUP]:

```
cp .env.example .env
gcloud auth application-default login
```

Key settings (shell env overrides `.env`):

| Variable                                                                    | Meaning                                                          |
| --------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| `HARNESS2_SOLVER_MODEL`                                                     | Executor model in `provider/model` format as OpenCode expects it |
| `HARNESS2_IMPROVER_MODEL`                                                   | Improver model, same format, defaults to the executor model      |
| `HARNESS2_JUDGE_MODEL`                                                      | Bare judge model ID; the adapter adds the Vertex prefix          |
| `HARNESS2_RUNS_DIR`                                                         | Output root, defaults to `runs/`                                 |
| `GOOGLE_CLOUD_PROJECT`                                                      | Vertex project; `GOOGLE_CLOUD_LOCATION` defaults to `global`     |
| `HARNESS2_CODEX_BASE_URL`, `HARNESS2_CODEX_API_KEY`, `HARNESS2_CODEX_MODEL` | Only when using the Codex executor; the bridge prints these      |

Use placeholder values like `google-vertex/<your-gemini-model>` until you own real model IDs;
the WorkBuddy setup additionally requires a Vertex Gemini solver and judge ID format [H2,
BENCHMARKS]. Authentication alternatives (service account keys, impersonation) are upstream
documented; keep `gcloud` on `PATH` in all cases.

## 5. Point the OpenCode executor at the FigTether workspace

1. Build or obtain FigTether, then verify locally: `figtether doctor`. Start the bridge if doing
   agent work: `figtether daemon` (loopback, default port 38947; a collision fails with a
   `--port` hint rather than silently picking another).
2. Open Figma Desktop, run the FigTether plugin, complete pairing, and grant the planned root.
   The agent never sees secrets (`FR02`).
3. Print the MCP entry for the client:

   ```
   figtether config print --client generic
   ```

   Paste the printed `command` and `mcp --stdio` arguments into OpenCode's MCP configuration as a
   server entry named `figtether`. The CLI never writes other applications' configs silently (PRD
   section 5.1).

4. Give the executor a workspace that contains: the FigTether repository (so pack files under
   `benchmarks/harness-pack/` are loadable), the FigTether-Bench task files under
   `benchmarks/figtether-bench/tasks/` (document 04), and the requested fixtures. Grading data
   stays outside this workspace.
5. Smoke test: ask the executor for a session listing; the first tool call after registration
   must be `list_sessions` or `get_bridge_status`, never a guessed session read (`FR04`, `PR-10`
   of document 03).

Note: judging runs must reuse the executor configuration recorded at refinement time; if the
model or task selection changes, use a new runs root (section 7).

## 6. The run loop

Upstream keeps refinement, judging, and reporting as separate commands that must share one
configuration [H2]. Shape of the loop (with a chosen benchmark):

```
harness2-setup <bench>
harness2-check --bench <bench>
harness2 --bench <bench> --k 1 --mode parallel --only-domains <domain> --max-tasks 1
harness2-judge --bench <bench> --k 1 --mode parallel
harness2-report --bench <bench> --k 1 --mode parallel
```

- Add `--dry-run` to `harness2` to preview task selection without model calls.
- `--substrate cx` switches the executor from OpenCode to Codex (must be passed consistently to
  setup, check, run, judge, and report).
- WorkBuddy commands take one subset at a time (`--subset office|web|code` after setup).
- Concurrency wrapper scripts exist upstream: `SUBSTRATE`, `K`, `MODE`, and `SUBSET` variables
  feeding `bash scripts/run_<bench>.sh` [H2, EXPERIMENTS].
- Upstream task partitioning is available for stratified splits:

  ```
  python scripts/split_tasks.py --bench lab --seed 42 --output splits/lab.json
  harness2 --bench lab --split-file splits/lab.json --split test --mode parallel
  ```

FigTether-Bench status: FigTether tasks plug into the same executor-plus-runner structure; until
the pack adapter exists (planned with the G1 iteration window, document 04 section 8), use the
upstream benchmarks only for environment calibration and use `harness2-demo` for toolchain checks.
Say "proposed" for FigTether-Bench runs in evidence logs until the adapter exists and API support
is verified, mirroring the PRD habit of separating untested claims (`D12`, `P14`).

## 7. Results, resume, and reproducibility

Upstream layout [H2]: results live under `runs/`, organized by benchmark and subset
(`runs/lab/`, `runs/jb/`, `runs/wb/<subset>/`); contents include task summaries, deliverables,
adopted harnesses, reflection notes, and evaluation results.

Operating rules:

1. Resume: repeat the same command; the saved configuration prevents incompatible resumes.
2. Isolation: set a distinct `HARNESS2_RUNS_DIR` whenever models or task selection change, for
   example `HARNESS2_RUNS_DIR=runs/fbt-k3-parallel-seed42`; a fresh root keeps configurations
   separable.
3. Judge and report parity: reuse the same benchmark, executor, subset, k, mode, and seed as the
   refinement run; mismatches produce unreliable comparisons.
4. Baseline comparison: upstream `harness2-judge --bases` scores a run namespace's reference
   executions, writing `base_judged.json` next to `judged.json` for base-harness comparison
   [H2, EXPERIMENTS]; `harness2-report` covers the refined harness only.
5. Failed evaluator stays unscored rather than counted as zero; judge failures write
   `judge.log`; a Vertex `429 RESOURCE_EXHAUSTED` means rerun `harness2-judge` later.
6. JobBench records the downloaded dataset revision in
   `dependencies/job-bench-eval/dataset/main/.harness2-download.json`; upstream instructs to
   report that revision alongside results [H2, EXPERIMENTS].
7. Changing the judge model changes the measurement; hold judge configuration fixed inside a
   comparison window.

## 8. Network restrictions (configure before evaluation)

Executors run with unrestricted network access upstream, and Codex runs with its sandbox
disabled; a net-level control is required [H2, EXPERIMENTS]. Apply the upstream guidance:

1. Run setup and check first (they legitimately need `github.com`, the npm registry, Hugging Face,
   and possibly a Docker registry or `astral.sh`).
2. Before the first executor run, block for the executor host the hosts that could serve grading
   data: upstream names `huggingface.co`, `hf.co`, `*.hf.co`, `github.com`, and
   `raw.githubusercontent.com`.
3. Enforce with an egress firewall or DNS policy on the executor host, not with tool settings.
4. Verify with a deny test (a blocked host fetch must fail), then record the deny rules in the
   run report (document 04 section 10).
5. FigTether-Bench grading material is local-only; deny everything that would let an executor
   fetch it remotely.

## 9. Common failures

| Symptom                                                          | Cause                                                                                 | Action                                                                                                                    |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| Daemon start fails with a port message                           | Port 38947 in use                                                                     | Follow the printed `--port` hint; regenerate the manifest; never auto-pick silently                                       |
| `harness2-check` fails on credentials                            | gcloud login or PATH issue                                                            | Re-run `gcloud auth application-default login`; keep `gcloud` on `PATH`                                                   |
| Executor cannot call FigTether tools                             | MCP entry missing or wrong workspace                                                  | Reprint with `figtether config print --client generic`; verify the server entry and pairing state with `figtether doctor` |
| Tool answers `PLUGIN_OFFLINE` or `SESSION_NOT_FOUND`             | Plugin closed, slept, or stale session (state machine, FR16, section 8.7 error table) | Re-run the plugin, pair again, call `list_sessions`; do not guess sessions                                                |
| Tool answers `PAUSED` or `REVOKED`                               | User action                                                                           | Stop; only the user resumes or re-grants (UI or CLI `revoke` path)                                                        |
| Tool answers `BUSY` cadence                                      | Queue full or session busy                                                            | Honor `retry_after_ms`; never tight-loop; per-session capacity rules in the pack                                          |
| `harness2-check` reports stale Codex configuration after changes | Endpoint or model changed under `dependencies/codex/`                                 | Re-run setup with `--substrate cx` as upstream requires                                                                   |
| Judge run exits nonzero, output in `judge.log`                   | Quota (`429`) or transient failure                                                    | Rerun `harness2-judge` later; failed evaluation stays unscored                                                            |
| Comparison looks inconsistent                                    | Judge, seed, subset, k, or mode changed between run and report                        | Rerun the full triplet with one recorded configuration; use a new `HARNESS2_RUNS_DIR`                                     |
| Docker check fails                                               | WorkBuddy subset without Docker Engine                                                | Install Docker or restrict this window to LAB and JobBench calibration                                                    |
| WorkBuddy license question                                       | Upstream territorial license                                                          | Read the upstream license before downloading or using the benchmark [H2, BENCHMARKS]                                      |
| Pack probe contradicts an envelope field                         | Pack text drifted from a frozen contract                                              | Fix the pack text by citation; if intent really changed, open an ADR first (`docs/README.md` change rules)                |

## 10. Attribution, license, and citation

Attribution summary for anything this runbook produces: the methodology, executor flags, and
results follow-up belong to the Harness2 authors; FigTether consumes the method.

- Paper: Harness2: Recursive Agent Harnessing for an Open World, arXiv:2610.14224, 2026.
- License: Apache 2.0; third-party code, benchmarks, and datasets keep their upstream licenses;
  not an officially supported Google product [H2].
- Repository: https://github.com/google-research/harness2 (README, BENCHMARKS.md,
  EXPERIMENTS.md, SETUP.md accessed 2026-10-20).

FigTether-wise, this runbook does not create a runtime dependency: the Rust workspace
(`crates/`) and plugin (`plugins/figma/`) never import harness2 or call benchmark hosts, and the
core's no-official-endpoint rule (`FR19`, `AT01`, `AT02`) plus deny-egress validation (`N09`)
remain in force for the product.

## 11. Run checklist

Before a run:

1. Preconditions frozen: fixture revision, FigTether build commit, plugin manifest, model IDs,
   executor, k, mode, seed, and subset recorded.
2. FigTether healthy: `figtether doctor` clean; session granted on the planned root.
3. MCP entry registered in the executor; smoke call executes without errors.
4. Network deny rules applied to the executor host and verified.
5. `--dry-run` of the first command previews the intended tasks.

After a run:

1. Report exists with the full denominator table (document 04 section 5), including blocked,
   timeout, unsupported, cancelled, and unscored rows.
2. `runs/` root isolated per configuration; the resume command recorded.
3. Lessons journal updated (when the run adopted a pack version).
4. Evidence filed at the active gate per `docs/08-harness2/04-evaluation-protocol.md` section 8
   and the release slots in `docs/07-ops/04-release-process.md`.
5. No claim published beyond what evidenced rows support (`P14`, `AT16`).
