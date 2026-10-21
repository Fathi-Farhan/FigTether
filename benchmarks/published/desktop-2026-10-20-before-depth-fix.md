# Figma Desktop website test - 2026-10-20

Created a 1440×1640 landing page in the supplied Untitled Figma file using FigTether's local MCP stdio adapter, native daemon and development plugin. The frame contains 71 editable child layers (72 scene nodes including the frame). Its text and typography were read back through MCP. A full-resolution native Figma PNG was visually inspected.

## Measured workloads

| Lane                     | Samples |  Successful | p50 ms | p95 ms | p99 ms |
| ------------------------ | ------: | ----------: | -----: | -----: | -----: |
| W1-warmup                |       1 |         1/1 | 199.45 | 199.45 | 199.45 |
| W1-cached-root           |   10000 | 10000/10000 |   2.31 |   2.92 |   3.31 |
| W2-fresh-72-nodes        |    1000 |   1000/1000 | 117.58 | 139.14 | 178.44 |
| W3-website-png-1440x1640 |     100 |     100/100 | 573.83 | 635.93 | 655.53 |

Percentiles use nearest rank and include failed calls. Cached/fresh/export lanes stay separate. Initial setup and earlier-build failures remain in the JSON report and raw log. The build records include exact commit, dirty state, source diff hash and binary/plugin/UI hashes.

## Resource and cost data

291 resource samples at one-second intervals. Daemon peak Windows working set: 27.04 MiB; peak private bytes: 20.93 MiB. Idle samples: 12. This is a shared workstation measurement.

Model token usage and USD cost: **NOT_MEASURED**. Billing information is not exposed here. The runner performs no model inference or paid API calls; this does not imply the chat itself is free.

## Compatibility findings

The real editor exposed and prompted fixes for the invalid manifest reasoning key, numeric loopback domain rejection, missing showUI startup, non-inlined UI assets, a permission-protected teamLibrary getter, incorrect iframe routing, top-level editor message delivery, synchronous lookup under dynamic-page, requested font style not being assigned, batch export normalization, and old credentials overriding an explicit new pairing secret. Regression coverage was added for startup, routing, lookup, typography, pairing and export batches.

## Scope and limits

- One Windows machine and Figma Desktop 126.10.10; no macOS/seat/client matrix.
- Website PNG is 1440x1640, not the planned N03 1024x1024 fixture.
- No 60-minute soak, editor freeze profiling, CPU isolation, or multi-client contention test.
- Call elapsed includes MCP, daemon queue, bridge, editor and normalization; these phases are not individually instrumented.
- The PNG file was obtained with Figma native Copy as PNG; MCP export measurements validate render/transport/asset metadata, not client download delivery.
- Writes used UI automation to approve each exact preview under the user’s explicit desktop testing authorization.
- Synthetic preview content and diagram status labels are artwork, not benchmark observations.

These results do not certify every N01–N10 requirement. In particular, the export fixture differs from N03 and this run does not establish N05/N06.

## Reproduce and retain versions

See [the desktop testing guide](../desktop-testing.md). Raw calls and one-second resource samples stay under ignored `benchmarks/results/desktop-2026-10-20/`. The PNG, fixture, desktop screenshots, state mapping and live request specs stay under ignored `tmp/desktop-validation/`. Reviewed aggregates are in [the JSON report](desktop-2026-10-20-before-depth-fix.json). Never overwrite a prior run directory for a different version.
