# Pull request quality checks

[CI](../.github/workflows/ci.yml) runs for new pull requests, updated commits, reopened pull
requests, and transitions to ready for review. It also runs on pushes to `main`, merge-queue
check requests, and manual dispatch. Draft PRs run the checks too. No path filters omit checks
for documentation-only changes. GitHub may require maintainer approval before running a
first-time fork contributor's workflow.
[GitHub trigger documentation](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#pull_request).

## Automated coverage

| Job                 | What blocks a passing result                                                                                                                                                     |
| ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `rust-fmt`          | Rust formatting drift                                                                                                                                                            |
| `rust-clippy`       | Compiler/lint warnings across workspace targets; dependency resolution must respect `Cargo.lock`                                                                                 |
| `rust-test`         | Workspace unit, integration, adversarial and documentation tests on Linux, Windows x64, macOS x64 and macOS arm64                                                                |
| `schema-contract`   | Missing generator, missing/empty schema sets, mismatched or uncommitted generated contracts                                                                                      |
| `plugin-typescript` | Missing plugin workspace/lockfile, failed clean dependency installation, type errors, lint errors, bundle failure, or failing plugin tests                                       |
| `docs-format`       | Formatting, publication regression tests, CI gate tests, design detector tests, public-file policy, local documentation links, stale generated figures, or forbidden punctuation |
| `dependency-audit`  | Rust advisory/source/license policy failures and high/critical npm advisories in root and plugin dependencies, including development tools                                       |
| `sbom`              | Failure to generate/upload the Rust CycloneDX dependency inventory                                                                                                               |
| `license-check`     | Missing license files, incompatible dependency licenses, or changed workspace license metadata                                                                                   |
| `secret-scan`       | Gitleaks findings in reachable checkout history, with findings redacted                                                                                                          |
| `workflow-lint`     | Invalid Actions syntax, expressions, runner labels or embedded shell detected by pinned actionlint and available shellcheck                                                      |

The final job, **`CI required`**, waits for every job above and checks their results even if
another job fails. Failure, cancellation, skipped work, empty input, malformed input, and
unknown statuses cannot produce success. Its job summary lists prerequisite results.
The four Rust matrix legs run independently with fail-fast disabled, so one failed platform
does not prevent the others from producing evidence.

Rust and plugin jobs retain test evidence for 30 days, including runner/compiler information
for Rust and a JUnit report for the plugin. GitHub retains job logs according to repository
settings. Artifacts are available when produced; an installation failure can occur before a
test report exists. Missing reports do not turn a failed test job green. SBOM artifacts also
have 30-day retention. Record the workflow run, attempt, commit, job, environment and workload
when comparing results; shared-runner timings alone are not a performance regression verdict.
No model billing or token savings are inferred from CI duration.

## Require checks before merging

Running CI and enforcing a merge restriction are separate settings. After the updated workflow
has run on GitHub, a repository administrator must configure a branch ruleset or branch
protection rule for `main`:

1. Require a pull request before merging.
2. Require the status check named **`CI required`**, selecting GitHub Actions as its source
   when the setting is available.
3. Require the branch to be current with `main`, or use the merge queue. This workflow handles
   the queue's `merge_group` event.
4. Require maintainer review for intent, scope and behavior that automated tests do not cover.
   Review changes to workflows, test exclusions, schema generators and security policies closely.
5. Set bypass permissions to match the repository's maintainer policy. A check cannot prevent
   a merge by someone explicitly allowed to bypass its protection.

Workflow files cannot enable those server-side settings. GitHub describes required status
checks and review/bypass settings in [protected branches](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches).
If the check has not appeared in the selector, wait for its first workflow run. Do not select
an unrelated release job with a similar name.

## Reproduce a failure

Use the commands in [Contributing](../CONTRIBUTING.md#run-the-checks). Additional CI-specific
checks are:

```sh
npm run ci:test
sh scripts/schema-check.sh
actionlint -color
```

The pinned actionlint version is 1.7.11. On Windows, use Git Bash for POSIX shell scripts or
`powershell -File scripts/schema-check.ps1` for the schema check. Root and plugin `npm ci`
install their respective lockfiles. A local run on one OS does not substitute for the hosted
matrix. The intended Rust minimum version is not separately tested by this matrix; it uses
the repository's pinned compiler.

Run `npm run ci:test` from Git Bash on Windows, or set `FIGTETHER_TEST_SH` to your Git Bash
executable when using PowerShell. The schema regression tests need a POSIX shell; they create
isolated fixtures and do not modify the repository's schemas.

Fix reported defects or update a deliberately changed contract and its tests. Do not remove
a required job, weaken an advisory policy, skip a failing test, or regenerate unrelated lockfiles
just to obtain a passing status. Public-fork PRs use the normal `pull_request` event and read-only
repository permissions, with persisted checkout credentials disabled; they do not need Figma
credentials or an LLM API key.

## What a passing check establishes

It establishes that the configured checks passed for that workflow's checkout and environment.
The suite contains an explicitly ignored real-Figma scenario; it is not counted as a native
editor success. CI does not prove real Figma compatibility, design originality, complete
accessibility, or absence of unknown vulnerabilities. Native approval/undo behavior and task
usability need separate editor or interaction evidence. There is no automatic AI review or
automatic merge in this pipeline; maintainers assess the remaining context.

The [21 October 2026 local validation record](audits/2026-10-21-ci.json) retains command
outcomes, timings, source hashes, test totals, corrected draft failures, and explicit limits.
It records local Windows execution, not a completed hosted Actions run or activated branch rule.
