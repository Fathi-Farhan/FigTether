# ADR-013: Bidirectional write path through the local plugin

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

Status: `Proposed` (requires product approval; supersedes the read-only rule of `D03` for a
future release)

## Context

FigTether exists to give an agent Figma design context without consuming the quota of the official
Figma MCP server. The read path proves that this is possible: the plugin reads the document through
the documented Figma Plugin API, inside the user's own editor session, and the core never calls the
official MCP or REST endpoints (`FR19`, `AT01`, `AT02`).

The same reasoning applies to writing, and the platform supports it. The official Plugin API
typings (`@figma/plugin-typings`, vendored in `plugins/figma/node_modules`) expose the full
mutation surface: `figma.createFrame`, `createText`, `createRectangle`, `createComponent`,
`createVector`, `createSection`, `createPage`, `createGroup`, `createSlice`, `createLine`,
`createEllipse`, `node.appendChild`, `node.insertChild`, `node.remove`, and the writable property
set (`x`, `y`, `width`, `height`, `rotation`, `opacity`, `fills`, `strokes`, `effects`,
`cornerRadius`, `layoutMode`, `itemSpacing`, `paddingLeft` and friends, `characters`,
`setRangeFontSize`, `setRangeFills`, `setRangeFontName`, `resize`, and more).

Two properties of that API shape the whole design:

1. **Plugin actions are not undoable by default.** The typings state it plainly: "By default,
   plugin actions are not committed to undo history. Call `figma.commitUndo()` so that triggered
   undos can revert a subset of plugin actions." Without an explicit `figma.commitUndo()`, an agent
   edit would leave the user with no editor level way back.
2. **Mutations are constrained by the document.** `appendChild` documents its refusal cases
   (parenting cycles, readonly internal nodes, reparenting inside an instance, component cycles),
   and text mutation requires `figma.loadFontAsync` before `characters` can be set.

The PRD currently forbids writing: `D03` decides "Read-only v0.1/v0.2, no eval", `FR24` requires
that a read never mutates, and the release boundary table lists write-back as "not available" until
a separate RFC with permissions and mutation testing. This ADR is that RFC, and it proposes a
narrower and safer shape than "let the agent edit the document".

## Decision

FigTether gains a write path that is **plan based, previewed, explicitly approved, budgeted,
audited, and undoable in Figma itself**. The write path uses the same plugin transport as the read
path, so it consumes no official MCP or REST quota and needs no personal access token.

The rules:

1. **Two grant modes.** A session holds a read grant, a write grant, or both. The write grant is a
   separate user action in the plugin UI, scoped to the same roots the read grant covers. A read
   grant never authorizes a mutation, and no tool parameter can widen a grant.
2. **No direct mutation tool.** An agent never mutates. It submits a declarative **write plan**: an
   ordered list of typed operations. The plan is data, not code.
3. **Validate, then preview.** The daemon validates the plan against the write grant, the scope,
   the operation budget, and the destructive-operation policy. The plugin then resolves the plan
   against the live document **without mutating it** and returns a **preview**: the nodes that would
   be created, changed, moved, or removed, with the affected counts and the changed key values.
4. **Explicit approval.** The user sees the preview in the plugin UI and approves or rejects it. The
   daemon issues an **approval token** bound to the plan hash, the session, the grant epoch, the
   approval id, and an expiry. A write without a valid token is refused.
5. **Apply as one undo step.** The plugin executes the approved plan and calls `figma.commitUndo()`
   exactly once at the end, so the whole plan is a single step in Figma's native undo history. The
   plugin may also call `figma.saveVersionHistoryAsync` before applying, when the seat allows it.
6. **Bounded operations.** Per plan: a maximum operation count, a maximum number of created nodes,
   a maximum text length, a maximum number of changed properties, and a maximum affected subtree
   size. Deletions and moves out of the granted roots need an explicit destructive flag in the
   plan, and a separate user toggle in the UI.
7. **Audited, content free.** The daemon records the applied plan: plan id, plan hash, approval id,
   the number of operations, the counts of the created, changed, removed, and moved nodes, and the
   timestamp. The node ids stay in the result the caller already holds. Never layer names, text, or
   other design content.
8. **No self invented revert.** FigTether does not expose `figma.triggerUndo()` to an agent: it
   reverts to the last committed state, which is not necessarily the agent's write. The recovery
   path is Figma's own undo, plus the version history when the plan recorded one.

## Rationale

- The quota objection does not apply. The official MCP service is not involved in either
  direction. The plugin API is the transport, and it is the same transport that already works for
  reads.
- The failure mode of a write is not a wrong answer, it is a destroyed design. A read cannot
  destroy anything; a write can. The response to that is not fewer capabilities but a different
  control shape: the user sees what will change, before it changes, and can undo it afterwards.
- The preview is what makes the feature usable. An agent that can only write blind is worse than
  useless in a design tool. A preview lets the user correct the plan ("use the existing component
  instead of drawing a rectangle") instead of undoing a mess.
- `commitUndo()` is the reason a write can be safe enough to ship. It converts the agent's edit
  into a single, editor native undo step, which is the recovery mechanism designers already
  trust. Figma's own version history is a second net when the seat offers it.
- Separate grant modes keep the existing read story intact. A user who grants read access does not
  silently gain write access, and the read path keeps its current threat model and tests.

## Consequences

Positive:

- The agent can design in Figma, not only read from it, with no Figma MCP quota and no personal
  access token.
- The write path reuses the transport, the session model, the grant model, and the scope
  enforcement that already exist, so most of the machinery is proven.
- The preview and the approval step make the write loop collaborative: the user and the agent
  converge on a plan before the document changes.

Negative:

- The read-only invariant changes. `FR24` becomes "reads never mutate; writes mutate only inside
  an approved write grant". The MCP tool surface now contains mutating tools, so the adapter
  cannot mark every tool as read only. The read tools keep `readOnlyHint: true`; the write tools
  carry `readOnlyHint: false` and `destructiveHint: true`, and the tool catalog has to separate
  them.
- The plugin UI grows a second critical surface: the approval panel, with its own accessibility
  and hygiene requirements.
- The threat model gains a new class: a compromised or confused agent that submits a destructive
  plan. The budget, the scope check, the preview, and the approval are the four layers that stand
  between that plan and the document.
- The plugin must handle the documented refusal cases of the mutation API, and it needs
  `loadFontAsync` before text changes, which is an async step inside the operation sequence.

Neutral:

- The plan and approval vocabulary is a new contract surface. It needs its own schema, its own
  error codes, and its own acceptance tests.

## Alternatives considered

- **Direct mutation tools** (for example a `set_text` tool that writes immediately): rejected.
  It gives the agent an unguarded write path, produces no reviewable artifact, and leaves the
  user with a document they did not approve.
- **Rely on Figma undo only, without an approval step**: rejected. Undo protects the document
  after the fact; it does not let the user refuse a destructive plan, and it does not help when
  the agent is mid-loop across many small edits.
- **Write through the official Figma MCP server**: rejected. It reintroduces exactly the quota
  and cost problem the product exists to avoid.
- **Write through the Figma REST API with a personal access token**: rejected. It needs a token
  the product explicitly refuses to ask for (`FR02`), and it bypasses the editor session.
- **Expose `figma.triggerUndo()` as a tool**: rejected. It reverts to the last committed state,
  which is not necessarily the agent's write, so it can destroy the user's own work.
- **Let the agent write into a scratch page only**: rejected as the only mode. It is a good first
  step of the rollout and is kept as the initial write scope, but it is too limiting to be the
  whole feature.

## Verification

- Contract tests: plan serialization, plan hash stability, approval token binding, and the
  rejection of a plan that exceeds a budget.
- Security tests: a write without a write grant, a write without an approval token, a write
  outside the granted roots, a write that exceeds the budget, and a destructive operation without
  the flag are all refused.
- Mutation tests: a fixture document is byte identical after a preview, and the planned change is
  present after an approved apply.
- Editor tests: the applied plan is a single step in Figma's undo history after
  `figma.commitUndo()`. This needs a real Figma Desktop session and cannot be proved by the
  automated suites.

## References

- PRD `D03`, `FR24`, and the release boundary table (write-back "not available" until a separate
  RFC with permissions and mutation testing).
- `docs/02-architecture/adr/ADR-004-grant-scope-model.md` (grant model, extended with a mode).
- `docs/02-architecture/adr/ADR-008-resource-budgets.md` (budget model, extended with write
  budgets).
- `docs/05-security/01-threat-model.md` (extended with the write threat classes).
- `docs/11-write-path/01-write-path-design.md` (the concrete contracts of this ADR).
- Figma Plugin API typings, vendored at
  `plugins/figma/node_modules/@figma/plugin-typings/plugin-api.d.ts`.
