# Documentation and skill review: 22 October 2026

Scope: the proposed public working tree, current source-install guides, project skills,
generated agent instructions and dated native testing records. This updates documentation
for public source review; it does not publish a GitHub repository or certify a release.

## What changed

- Updated the shared FigTether workflow source and regenerated its MCP instructions and both
  SKILL.md copies. Guidance covers discovery, current limits, exact-plan approval, eligible
  user-enabled Auto, error recovery, partial-write inspection, rendering and reload effects.
- Updated the project Design Evidence skill to require observed geometry and actual visible
  render evidence for consequential native changes. Tiny/clipped exports are retained as
  failed or insufficient evidence, not successful visual checks.
- Aligned setup, support, operations, UI help and implementation status with `figtether start`,
  manifest-owned ports, BOM-free generation and the canonical `dist/manifest.json` import.
  Removed obsolete `manifest generate` and `daemon --shutdown-graceful` command examples.
- Preserved dated benchmark reports and the formal status of proposals/ADRs. Current guides
  link newer native findings instead of silently rewriting historical evidence.
- Made unavailable snapshot/watch workflows and the GROUP schema gap explicit. The current
  bridge does not expose the whole Figma Plugin API. PentAGI methodology use is distinguished
  from running its service or commissioning an independent audit.

The later [visual refresh](visual-refresh-2026-10-22.md) records the user-selected banner and
matching charts/UI. The [pre-publication history mapping](history-provenance.md) preserves
source lookup after the owner requested corrected author/committer metadata and commit subjects.

## Validation and publication boundaries

The publication pass runs the generated-workflow drift check, skill validators, Markdown
formatting, relative-link/anchor audit, BOM/public-file checks and whitespace checks. Focused
CLI/MCP tests check that embedded guidance and host-setup behavior still work after regeneration.
Raw command logs and the documentation inventory remain in ignored local artifacts.

Recorded results:

- Public-file check: 527 proposed files, zero issues; documentation audit: 116 Markdown files,
  zero link/anchor issues. Generated README SVGs and all three workflow outputs are current.
- Three skill-validator passes: project FigTether Design, its integration copy, and project
  Design Evidence. PyYAML was installed only in ignored local artifacts for validation.
- Formatting and whitespace checks pass; all six public-check regression tests pass.
- Focused Rust suites pass: 139 CLI tests and 69 MCP tests.
- A rebuilt binary's real stdio initialization returns exactly the generated instruction text,
  and tool discovery returns 17 tools. This is guidance/transport evidence, not a native write.

The final native FILL width assertion remains pending restored user sharing/editing/Auto.
The source fix has regression coverage; this review does not promote that pending check to
a native success. The completed Studio Notes design and other native results are recorded in
the [feature/error review](../06-testing/end-user-feature-review.md).

Private manifests, runtime credentials, raw node data, companion capabilities and screenshots
are excluded from the proposed public set. Publication checks do not inspect all secret forms
or replace a history-aware scanner. The documentation-only pass did not run that scanner.
Subsequent publication preparation used the official Gitleaks 8.30.1 Windows binary, verified
against its release checksum: all seven pre-publication commits and a 527-file public snapshot
have zero findings under the existing configuration. Running the first directory scan with
absolute paths flagged three already documented public fixture values; running from the
snapshot root applied the existing exact exceptions. No new scanner exception was added.
Reports are retained outside the repository. This finite scan does not establish that no
secret of any form exists. Follow the [publication checklist](publication.md) for repository
settings, repeat scans and hosted CI.

Repository upload, GitHub security settings, signed artifacts and cross-platform editor testing
are outside this documentation pass. No commit, push or release is implied by a passing local
check.
