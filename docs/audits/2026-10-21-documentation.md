# Documentation and design-evidence audit: 21 October 2026

## Scope and method

This review inventories the public Markdown corpus, screens known language/capability
regressions, resolves local links and explicit heading anchors, and reviews claim-bearing
passages against the current source and dated test evidence. The
[machine-readable inventory](2026-10-21-inventory.json) records every included file and its hash.

Semantic review focused on read/write capabilities, authorization, transport, packaging,
setup/recovery instructions, release status, and benchmark interpretation. This is not a
formal verification of every sentence, an external security audit, or revalidation of every
historical research claim. Original proposals and dated evidence are preserved as such.

## Corrections

| Area                   | Correction                                                                                                                                                            | Evidence or current guide                                                                                     |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| Language               | Translated the complete Bridge guide, tables, alt text, and example agent instructions into English; removed obsolete language labels.                                | [Bridge guide](../bridge-ui-guide.md)                                                                         |
| Native design creation | Corrected the main-sandbox README, code map, and overview so read invariants do not imply the absence of writes.                                                      | [Write-path verification](../write-path-verification.md), Rust adapter/daemon and plugin write executor       |
| Transport              | Distinguished the plugin UI's `localhost` endpoint from the daemon's numeric loopback bind, and described the adapter's outbound WebSocket accurately.                | [Plugin setup](../../plugins/figma/README.md), `plugins/figma/ui/src/protocol.ts`, `crates/mcp/src/daemon.rs` |
| Packaging              | Replaced obsolete untested-packaging language with the actual inline HTML build and limited recorded editor evidence.                                                 | `plugins/figma/build.mjs`, [desktop report](../../benchmarks/published/desktop-2026-10-20.md)                 |
| Security and approvals | Corrected claims that native writes had never been exercised; preserved the formal ADR release decision separately from implemented controls.                         | [Current documentation status](../documentation-status.md)                                                    |
| Historical documents   | Added a visible design-reference note across the numbered proposal corpus so planned commands, requirements, and rollout stages are not read as current availability. | [Current documentation status](../documentation-status.md)                                                    |
| Benchmarks             | Kept offline, native desktop, UI verification, and detector timings separate; retained unknown billing and failed/superseded attempts.                                | [Benchmark guide](../../benchmarks/README.md) and dated JSON reports                                          |
| Future maintenance     | Added an English documentation policy, repeatable documentation checks, a versioned design skill, and deterministic rule tests.                                       | [Contributor guide](../../CONTRIBUTING.md), [design review](../design-review.md)                              |

The root README, quickstart, plugin READMEs, implementation map, changelog, access-control
notes, threat model, PRD status discussion, and approval registry were checked for conflicting
current capability claims. The ADR history was not silently promoted to an accepted release.

## Design research and implementation

The [primary-source review](../../.agents/skills/design-evidence/references/research.md)
examines front-end generation benchmarks, an empirical UX study, W3C accessibility criteria
and test formats, provenance, and detector dataset bias. The
[skill](../../.agents/skills/design-evidence/SKILL.md) uses that evidence to support design
decisions and partial deterministic review without inferring AI authorship from style.

The evaluator has 14 rules: coverage/manual gaps, four standard-derived observations,
captured overflow, four project-contract checks, and three review-only patterns. Thirty-five
tests cover thresholds, exceptions, missing evidence, profile opt-in, stable ordering,
repeatability, and author-label invariance. No labeled authorship corpus, user study, or
classifier sensitivity/specificity is claimed.

## Observed browser exercise

Eight captures cover the built plugin's unpaired state at 320, 420, and 960 CSS-pixel requested
widths in light and dark themes, plus help-expanded states at 420 px. Browser AX names enrich
the DOM observations. Enter opened the focused help disclosure in both themes after the
isolated harness's focus/keyboard configuration was corrected.

The exercise found a password-field type-size drift caused by browser defaults. The input
now inherits the body font. A draft collector also counted hidden closed-disclosure content;
the collector was corrected and the draft captures were retained as superseded evidence.
The verified run found no failures in its configured observed subset; manual-review coverage
remains explicitly untested. All 800 repeated evaluations produced identical reports for
their respective fixed inputs.

See the [versioned browser record](../../benchmarks/published/design-evidence-2026-10-21.json)
for source/collector/profile/ruleset hashes, outcomes, local evaluator timings, draft findings,
and raw-evidence references. These are browser smoke tests, not new native Figma latency
benchmarks. Model cost and tokens remain unmeasured.

## Checks and remaining limits

The [validation record](2026-10-21-validation.json) retains test counts and durations, the
installed skill's hashes, browser evidence references, runtime versions, and unknown cost status.

```sh
npm run docs:check
npm run design:test
npm run public:test
npm run public:check
```

The skill's frontmatter validator and the plugin's 467 tests also passed. The local skill
validator required PyYAML; it was installed only into an ignored validation-dependency folder.
No global Python package or model credential was required.

The documentation checker detects a maintained set of known non-English/stale phrases;
it is not a general language detector or grammatical certification. It checks local link
targets and explicit Markdown heading anchors, not external URL availability or every
inline-code path. Historical proposals can still describe unavailable functionality because
their status notes explicitly preserve that distinction.

Outstanding product limits remain: selection/inspection integration failures, incomplete
snapshot/watch/offline workflows and full asset download delivery, manual undo restoration,
version checkpoints, wider platform/seat coverage, and soak tests. The design evaluator also
leaves task success, recovery, originality, and full accessibility conformance unassessed.
