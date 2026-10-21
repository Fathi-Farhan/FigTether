# Requirement-based interaction review

Use this reference for behavior, routes, resources, or keyboard operation. It extends the
review workflow; the static evaluator does not execute these tests.

## Define the oracle before the probe

For each relevant requirement, record a stable case ID, initial state, fixture, target,
action, expected observable result, allowed side effects, completion condition/timeout,
and reset procedure. Include viewport, theme, browser/editor and artifact versions.
Map missing cases to untested requirements; do not divide only by the cases that ran.

An action is successful when the specified outcome occurs. An unrelated animation, changed
timestamp, or DOM mutation is insufficient. Some valid actions produce no DOM change:
copying, downloading, or rejecting an already-invalid operation need another observation.
Observe the correct destination, payload, state, or error rather than guessing from activity.

Use isolated, resettable fixtures for action sweeps. Apply the existing authorization and
write-approval boundaries to live actions. An automated click sweep must not approve a
Figma write or grant a wider scope as a side effect of checking the UI.

## Choose checks that fit the surface

- **Resources and routes:** inventory required routes and media. Confirm the expected content
  and destination, not only HTTP 200; a fallback page can return 200 for a broken route.
  Wait for image loading/decoding before classifying failures; lazy images may need scrolling.
  Require offline resource closure only when the product brief requires it.
- **Behavior and recovery:** exercise the relevant transition and its reverse/reset where the
  feature promises reversibility. Wait for a declared completion signal instead of requiring
  synchronous state changes. Recheck layout/contrast in the resulting state.
- **Isolation:** declare permitted shared changes, such as a global status banner, derived
  counter, or accessibility announcement. Flag unexpected effects; do not require every
  mutation to remain inside the clicked control's DOM subtree.
- **Errors:** retain page/runtime/console errors with their triggering case. Distinguish expected
  validation failures, missing fixtures, and infrastructure errors from product defects.
- **Keyboard and focus:** follow the intended navigation order, including composite widgets,
  modal containment and return focus. Inspect visible focus in its rendered context. A nonzero
  outline or a monotonically increasing DOM index alone does not prove correct navigation.
- **Responsive geometry:** sample intended widths and breakpoint boundaries. Classify intended
  overlap, scroll containers, writing direction, and clipping before failing a layout. A 20 px
  sampling interval can miss a narrower failure; report sampled coverage explicitly.

## FigTether cases

These are review specifications, not claims that the present browser harness implements them.

| Case | Initial state and action                                                   | Expected result                                                                         | Evidence boundary                                                                                 |
| ---- | -------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| B01  | Unpaired; focus the pairing-help disclosure and press Enter, then close it | Help reveals the daemon and pair commands, then collapses; the trigger remains operable | Existing browser harness checks opening only; closing and focus return need additional evidence   |
| B02  | Local fixture; submit an invalid/expired pairing secret                    | Clear retry guidance; no session/grant; secret absent from captured logs                | Requires a credential-safe protocol fixture; no real secret in a snapshot                         |
| B03  | Paired fixture; choose root A, grant read, then select root B              | Grant stays bound to A; B is not silently added                                         | Inspect authoritative granted IDs, not only the selection highlight                               |
| B04  | Write-enabled fixture; preview plan, reject it                             | No document mutation; user can revise or retry                                          | Plugin/protocol tests and native editor evidence are distinct                                     |
| B05  | Explicitly approved plan in a disposable document; apply once              | Exact declared changes, bounded scope, recorded result                                  | Native Figma execution with the plan's approval; do not automate approval through a generic sweep |

The existing eight browser captures cover visual states and a narrow help-opening action.
They do not establish end-to-end pairing, grant enforcement, native write approval, undo,
or user task success. Preserve that distinction when reporting existing results.

## Evidence and cost record

Save case results with observed versus expected outcomes, applicability, selectors/node IDs,
before/after evidence, exceptions, omissions, and raw-artifact hashes. Keep credentials out of
captures. Record runtime, sample count, host/tool versions, and workload. Store model tokens,
provider prices, and billed cost only when observed; otherwise use unmeasured, not zero.

To evaluate this skill itself, use paired tasks with and without the skill under matched
model/harness, fixture, budget, and environment settings. Report failures and token/time
regressions alongside success. Script repeatability tests establish repeatability, not skill
efficacy or improved usability.

The rationale and source limitations are in [the research review](research.md#research-merge-21-october-2026).
