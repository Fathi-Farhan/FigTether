# Published JSON Schemas

Status: `Generated`. Do not edit the files in this directory by hand.

Every file in this directory is the JSON Schema (draft 2020-12) of one published FigTether
contract. The files are generated from the Rust contract model in `crates/model/src`, and the
generator is the single source of truth for them.

## Generator

```sh
cargo run -p figtether-model --example export_schemas
```

The example binary writes one file per contract into `schemas/`, named `<name>.schema.json`, and
prints the file it wrote. It accepts two optional arguments, the output directory and the model
source directory, which default to `schemas/` and `crates/model/src`:

```sh
cargo run -p figtether-model --example export_schemas -- <out-dir> <model-src-dir>
```

The generator fails with a non-zero exit code and a diff when the Rust model and the published
schemas disagree. A changed field, a changed field type, a changed optionality, a new enum
variant, or a changed doc comment in `crates/model/src` is a hard error, not a silent drift. The
published files are therefore never newer than the model and never older.

The tool parameter schemas come straight from the `schemars` implementations of the Rust parameter
types in `crates/model/src/tools.rs`. The envelope, source, observation, coverage, error,
capability and normalized model schemas come from the contract mirror inside the generator, which
reproduces the Rust types field for field. The write path schemas come from the same mirror: the
plan, the operation, the preview, the approval, the result, the grant mode, and the parameters of
the three write tools are contracts of the write path (`ADR-013`,
`docs/11-write-path/01-write-path-design.md`). The mirror is verified against the Rust model source
on every run, so it cannot drift away from the model without the generator failing.

## Rules for consumers

1. Read the published schemas, never a hand written copy of a contract. The schema version travels
   with every response in `schema_version`, and the current value is `1.0`.
2. Tolerate unknown optional fields. A newer producer may add optional fields to any object.
   Readers must ignore fields they do not know, and must not fail on them. Unknown major versions
   are rejected with `UNSUPPORTED_SCHEMA`; unknown minor and patch versions are readable.
3. A missing field and a field with an empty value are different things. A field the platform did
   not expose is omitted and reported in `coverage.unavailable_fields` or in `warnings`, never
   returned as `null` posing as an empty value.
4. Identifiers are opaque strings. A node id is never parsed as a number, and the typed identifier
   newtypes (`SessionId`, `SnapshotId`, `ObservationId`, `AssetId`, `JobId`, `GrantId`) are
   serialized as plain strings.
5. The `data` member of the response envelope is tool specific and is typed as any JSON value in
   `envelope.schema.json`. The shape of a payload is fixed per tool in
   `docs/04-data-contracts/02-tool-catalog.md`.

## Published files

| File | Rust type | Purpose |
| --- | --- | --- |
| `envelope.schema.json` | `Envelope` | Common response envelope of every FigTether answer. |
| `source_ref.schema.json` | `SourceRef` | Provenance of one result. |
| `observation.schema.json` | `Observation` | Observation window of one read. |
| `coverage.schema.json` | `Coverage` | How much of the request a payload actually covers. |
| `warning.schema.json` | `Warning` | Non-fatal caveat attached to a result. |
| `error.schema.json` | `FigError` | Structured error object of a failed request. |
| `error_code.schema.json` | `ErrorCode` | Canonical error codes of the error taxonomy. |
| `capabilities.schema.json` | `Capabilities` | Capability report of one session. |
| `capability_state.schema.json` | `CapabilityState` | State of one probed capability. |
| `probe_outcome.schema.json` | `ProbeOutcome` | Outcome of one capability probe. |
| `normalized_node.schema.json` | `NormalizedNode` | Normalized design model of one node. |
| `node_summary.schema.json` | `NodeSummary` | Identity and tree position of one node. |
| `geometry.schema.json` | `Geometry` | Geometry of one node. |
| `layout.schema.json` | `Layout` | Auto layout description of one node. |
| `typography.schema.json` | `Typography` | Typography of one text node. |
| `text_run.schema.json` | `TextRun` | One styled run inside a text node. |
| `paint.schema.json` | `Paint` | One fill or stroke of a node. |
| `effect.schema.json` | `Effect` | One visual effect of a node. |
| `component_ref.schema.json` | `ComponentRef` | Reference to the component of an instance node. |
| `variable_ref.schema.json` | `VariableRef` | Reference to a variable or a style. |
| `asset_ref.schema.json` | `AssetRef` | Reference to an exported asset. |
| `export_settings.schema.json` | `ExportSettings` | Export settings of one asset. |
| `field_availability.schema.json` | `FieldAvailability` | Availability of one field of a record. |
| `list_nodes_params.schema.json` | `ListNodesParams` | Parameters of the `list_nodes` tool. |
| `get_nodes_params.schema.json` | `GetNodesParams` | Parameters of the `get_nodes` tool. |
| `inspect_design_params.schema.json` | `InspectDesignParams` | Parameters of the `inspect_design` tool. |
| `get_design_tokens_params.schema.json` | `GetDesignTokensParams` | Parameters of the `get_design_tokens` tool. |
| `export_assets_params.schema.json` | `ExportAssetsParams` | Parameters of the `export_assets` tool. |
| `get_selection_params.schema.json` | `GetSelectionParams` | Parameters of the `get_selection` tool. |
| `list_sessions_params.schema.json` | `ListSessionsParams` | Parameters of the `list_sessions` tool. |
| `get_bridge_status_params.schema.json` | `GetBridgeStatusParams` | Parameters of the `get_bridge_status` tool. |
| `get_snapshot_info_params.schema.json` | `GetSnapshotInfoParams` | Parameters of the `get_snapshot_info` tool. |
| `watch_scope_params.schema.json` | `WatchScopeParams` | Parameters of the `watch_scope` tool. |
| `get_changes_params.schema.json` | `GetChangesParams` | Parameters of the `get_changes` tool. |
| `get_job_status_params.schema.json` | `GetJobStatusParams` | Parameters of the `get_job_status` tool. |
| `grant_mode.schema.json` | `GrantMode` | Mode of a session grant on the write path. |
| `write_operation.schema.json` | `WriteOperation` | One operation of a write plan. |
| `write_plan.schema.json` | `WritePlan` | Declarative change set proposed for one session. |
| `write_preview.schema.json` | `WritePreview` | What a write plan would change, resolved against the live document. |
| `write_approval.schema.json` | `WriteApproval` | Approval of one write plan, bound to the plan hash and the grant. |
| `write_result.schema.json` | `WriteResult` | What an approved write plan changed. |
| `preview_write_params.schema.json` | `PreviewWriteParams` | Parameters of the `preview_write` tool. |
| `get_write_approval_params.schema.json` | `GetWriteApprovalParams` | Parameters of the `get_write_approval` tool. |
| `apply_write_params.schema.json` | `ApplyWriteParams` | Parameters of the `apply_write` tool. |

## Conventions

1. Every file declares `"$schema": "https://json-schema.org/draft/2020-12/schema"` and an
   `"$id"` of the form `urn:figtether:schema:<name>`. The identifier is a URN on purpose: the
   project name and the website are still provisional (`D10`), so the schemas do not claim a
   domain that does not exist yet.
2. Every file is self contained. A contract file only carries the definitions it needs in its own
   `$defs`, so a file can be consumed on its own.
3. Files are pretty printed with two space indentation, sorted keys, and one trailing newline, so
   a diff between two schema versions stays readable.
4. The generator is deterministic. The same Rust model always produces byte identical files.
