# Contributing to FigTether

Thank you for helping to build FigTether. This guide describes how a change is proposed,
reviewed, and merged. It is short on purpose: the rules below are the ones that protect the
project's contracts, its evidence trail, and its users' data.

Status: `Proposed`. The project has not run Gate `G0` yet, so the process described here is
the intended one and not a report of what has already happened.

## 1. Before you start

1. Read `docs/01-agile/01-sdlc-process.md` for the process model and the CI gate list.
2. Read `docs/02-architecture/adr/ADR-000-template.md` if your change touches a contract.
3. Install the toolchain with `just bootstrap`, or with `sh scripts/bootstrap.sh` or
   `pwsh -File scripts/bootstrap.ps1`. The tasks need Rust (pinned in `rust-toolchain.toml`),
   Node.js LTS, and `just`. The audit, SBOM, and release tasks also need
   `cargo install cargo-deny cargo-audit cargo-cyclonedx`.
4. Make sure `just check` passes before you open a pull request.

## 2. ADR first for contract changes

A contract change is any change to a published schema, an MCP tool signature, the bridge
protocol, the error taxonomy, a resource limit, a grant rule, or a credential lifetime.

Contract changes follow the ADR first rule:

1. Open an ADR in `docs/02-architecture/adr/` with the next free number and the
   `ADR-000-template.md` structure.
2. Get the sign-off of the owning role from the approval record in
   `docs/99-appendices/03-approval-record.md`.
3. Only then change the contract, and change the traceability matrix in the same change set.

The ADR records the problem, the options, the decision, the consequences, and the verification
method. A pull request that changes a contract without an ADR is not ready for review.

## 3. Traceability for every requirement

Every requirement you touch carries a row in `docs/06-testing/03-traceability-matrix.md` that
links:

- the requirement id (`FRxx`, `Nxx`, `ATxx`, `Pxx`, or `Rxx`),
- the acceptance test id that proves it,
- the implementation location,
- the evidence that the test ran.

A new requirement without a traceability row, or a changed behavior without an updated
acceptance test, is incomplete. If a requirement cannot be tested yet, say so in the pull
request description and mark the row as pending with the reason.

## 4. Evidence

The project ships measured claims only. Unmeasured is unclaimed.

- Tests: attach the command and the outcome. `just test` and `just e2e` are the entry points.
- Benchmarks: benchmarks are split by workload class. Cached reads, fresh reads, snapshot
  reads, and asset exports are never averaged together. Attach the configuration, the raw
  result file, and the summary. The procedure lives in
  `docs/06-testing/04-benchmark-plan.md`.
- Release evidence: the template of PRD Appendix C is printed by
  `sh scripts/release-evidence.sh` or `pwsh -File scripts/release-evidence.ps1`. It records the
  commit, the artifact hashes, the schema version, the environment, and the reviewer.
- Never claim a performance number that was not measured on the exact commit under review.

## 5. Formatting and lint rules

| Area              | Rule                                                                 | Command                                                 |
| ----------------- | -------------------------------------------------------------------- | ------------------------------------------------------- |
| Rust              | `rustfmt` with the repository `rustfmt.toml`, no manual reformatting | `cargo fmt --all` and `cargo fmt --all --check`         |
| Rust              | Clippy is clean with warnings denied                                 | `cargo clippy --workspace --all-targets -- -D warnings` |
| Markdown and JSON | Prettier with the repository `.prettierrc.json`                      | `npm run format` and `npm run format:check`             |
| TypeScript        | The plugin lint and typecheck rules of `plugins/figma/`              | `just plugin-typecheck` and `just plugin-lint`          |

Prettier is configured with `printWidth: 100`, `proseWrap: preserve`, and `endOfLine: lf`. Run
`npm run format` before pushing, and never hand edit a file that a formatter owns.

## 6. The no em dash rule

Do not use the em dash character anywhere in the repository: not in Markdown, not in Rust
comments, not in a workflow file, not in a commit message. Use an ASCII hyphen, a colon, a
comma, or a full stop instead. The same rule applies to en dashes.

The rule is enforced in CI by `sh scripts/scan-em-dash.sh` or
`pwsh -File scripts/scan-em-dash.ps1`. The reason is that the document set has to render the
same way in Confluence, in a terminal, and in a PDF.

## 7. Pull request size

Keep a pull request small enough to review in one sitting.

- Target 400 changed lines or fewer per pull request, excluding generated files.
- One concern per pull request: a fix, a feature, a refactor, and a formatting sweep do not
  belong together.
- Split anything larger into a stack of pull requests, each of which builds and passes CI on
  its own. The first pull request in the stack may be a pure refactor with no behavior change.
- Generated files (the files under `schemas/`) are committed with the change that produced
  them, and their diff is reviewed for meaning, not for line count.
- A pull request that only bumps a dependency is still a pull request: it needs the audit and
  the license check to pass.

## 8. Commit and branch hygiene

- One topic per branch, named `feature/<short-topic>`, `fix/<short-topic>`, or
  `chore/<short-topic>`.
- Commit messages use the imperative mood and explain the why, not the what.
- Do not commit build output, editor state, credentials, or client designs. The `.gitignore`
  file already excludes the build directories; keep it that way.
- Lockfiles are tracked on purpose. `Cargo.lock`, `package-lock.json`, and
  `plugins/figma/package-lock.json` are part of the build contract and are reviewed like code.

## 9. Data handling

- Never attach a client design, a screenshot of a client design, a token, a pairing secret, or
  a credential to an issue or a pull request.
- Reproduce a problem with a public fixture from `fixtures/`, or with a synthetic fixture that
  you create for the report.
- Logs and diagnostics are sanitized by design. If you find a path that leaks design content
  into a log, that is a security issue: report it privately as described in `SECURITY.md`.

## 10. Local task reference

| Task             | `just`                  | POSIX shell                                                        | PowerShell                                   |
| ---------------- | ----------------------- | ------------------------------------------------------------------ | -------------------------------------------- |
| Build            | `just build`            | `cargo build --workspace --release --locked`                       | `cargo build --workspace --release --locked` |
| Test             | `just test`             | `cargo test --workspace --locked`                                  | `cargo test --workspace --locked`            |
| Lint             | `just lint`             | `cargo clippy --workspace --all-targets -- -D warnings`            | same                                         |
| Format           | `just fmt`              | `cargo fmt --all`                                                  | `cargo fmt --all`                            |
| Format check     | `just fmt-check`        | `cargo fmt --all --check`                                          | `cargo fmt --all --check`                    |
| Schemas          | `just schema`           | `cargo run -p figtether-model --example export_schemas -- schemas` | same                                         |
| Schema check     | `just schema-check`     | `sh scripts/schema-check.sh`                                       | `pwsh -File scripts/schema-check.ps1`        |
| Benchmarks       | `just bench`            | `cargo bench --workspace --locked`                                 | same                                         |
| Plugin build     | `just plugin-build`     | `npm --prefix plugins/figma run build`                             | same                                         |
| Plugin test      | `just plugin-test`      | `npm --prefix plugins/figma test`                                  | same                                         |
| Docs check       | `just docs-check`       | `npm run format:check`                                             | `npm run format:check`                       |
| Audit            | `just audit`            | `cargo deny check`                                                 | `cargo deny check`                           |
| SBOM             | `just sbom`             | `cargo cyclonedx --all-features --format json`                     | same                                         |
| End to end       | `just e2e`              | `cargo test -p figtether-e2e --locked`                             | same                                         |
| Bootstrap        | `just bootstrap`        | `sh scripts/bootstrap.sh`                                          | `pwsh -File scripts/bootstrap.ps1`           |
| Full check       | `just check`            | `sh scripts/check.sh`                                              | `pwsh -File scripts/check.ps1`               |
| Release evidence | `just release-evidence` | `sh scripts/release-evidence.sh`                                   | `pwsh -File scripts/release-evidence.ps1`    |

The `just` recipes run through `sh`, so on Windows run them from Git Bash. The PowerShell
scripts are the supported alternative on Windows.

## 11. Reporting problems

- Bugs and feature requests: use the issue templates in `.github/ISSUE_TEMPLATE/`.
- Security issues: follow `SECURITY.md`. Never use a public issue for a security report.
- Behaviour that contradicts the documented contract: open an issue first, then a pull request
  with the ADR.
