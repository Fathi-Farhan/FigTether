# Adversarial Validation with PentAGI

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

Status: `Proposed`

Current assessment record: the [22 October feature/error review](../06-testing/end-user-feature-review.md)
uses pinned upstream PentAGI methodology for scoped testing. It did not execute the PentAGI
service or establish an independent security audit. The platform deployment and attack plan
below remain proposed.

This document defines how FigTether is validated against an autonomous attacker, and how the
PenAGI platform is used for it. The threat model in `docs/05-security/01-threat-model.md` states
what could go wrong; this document states how we find out whether it does.

## 1. Why an autonomous attacker

FigTether's whole security argument rests on a small, local trust boundary: a loopback daemon, a
one time pairing secret, a grant that a user approves in the Figma UI, and two epochs that make
stale answers detectable. Manual testing of such a surface is exactly where human reviewers miss
things, because the interesting cases are boring ones: a header, a port, a header spelling, a
second request with the same secret.

An autonomous pentesting platform does three things a manual pass does not:

1. it enumerates the variations of a request instead of the one variation in the author's head;
2. it keeps the state of an attack, so a replay, a second session, and a later reconnect are tried
   without a human having to remember them;
3. it produces a written report that a second person can review.

PentAGI (`https://github.com/vxcontrol/pentagi`, Apache-2.0, by VXControl) is a self hosted
multi agent platform for autonomous penetration testing. It runs its tools inside sandboxed Docker
containers, keeps its memory in PostgreSQL with pgvector, and exposes REST and GraphQL APIs with
Bearer token authentication. A penetration test is a _flow_: a persistent session with tasks inside
it. The flow is started from a plain language description of a target and a goal.

The upstream platform can also use MCP servers, but that is an Enterprise feature. This validation
does not depend on it: PentAGI attacks the daemon network surface directly, which is the more
meaningful test for a local trust boundary. The agent is the attacker, not a client of ours.

## 2. Scope, and the rules of engagement

In scope:

- the daemon HTTP and WebSocket surface on a dedicated assessment port;
- the three channels: the plugin bridge, the local client channel, and the control endpoints;
- the authentication, grant, epoch, and resource limit behaviour;
- the information that the daemon exposes through its health, status, and error surfaces.

Out of scope, and forbidden to the agent by the flow prompts:

- any host other than the assessment machine;
- the real Figma installation, the real plugin, and any real design data: the assessment runs
  against a fake plugin session and the public fixture corpus;
- the machine itself: no credential harvesting outside the runtime directory of the assessment, no
  persistence, no privilege escalation attempt on the host, no lateral movement.

The assessment runs against a daemon started with a dedicated port and a dedicated runtime
directory, so a finding can never touch the real installation.

## 3. Threat classes and the flow that tests them

| #   | Threat class                                   | PRD reference                               | Flow                                                                                                             | Expected outcome                                                                                                              |
| --- | ---------------------------------------------- | ------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| 1   | A foreign web page reaches the loopback daemon | 8.6.1 row 1, `FR02`, `AT06`                 | Forged `Host`, forged `Origin`, DNS rebinding style `Host`                                                       | Every request with a non loopback `Host` is refused with `PERMISSION_DENIED` before the upgrade                               |
| 2   | A browser reaches the control endpoints        | 8.6.1 row 1 and the `Origin: null` residual | `Origin: null` request, request without `Origin`, cross origin `POST` to `/control/pair` and `/control/shutdown` | The control endpoints refuse a request that carries an `Origin` at all, and refuse a request without the control header       |
| 3   | Credential replay                              | `FR02`, `AT06`                              | Use a minted pairing secret twice, use an expired secret                                                         | The second use is refused with `AUTH_REQUIRED`, the expired one with `AUTH_EXPIRED`                                           |
| 4   | Credential confusion between the channels      | 8.6.1 row 1, `AT06`                         | Present the client credential on the bridge, present a pairing secret on the client channel                      | Both are refused: the two channels do not accept each other's credential                                                      |
| 5   | Scope escape between two sessions              | `FR03`, `AT05`, `AT17`                      | Two sessions with the same node ids, read a node of the other, use a foreign cursor or asset id                  | Every cross session read is refused with `OUT_OF_SCOPE`, and no cursor or asset of another session resolves                   |
| 6   | Resource exhaustion                            | `FR13`, `FR14`, `AT08`                      | Many concurrent connections, a queue flood, an oversized frame, a chunk that lies about its size                 | The daemon answers `BUSY` with `retry_after_ms`, refuses oversized input with a structured error, and keeps serving `/health` |
| 7   | Information disclosure                         | `FR18`, `R13`                               | Read the health and status surfaces, read the logs, read the runtime directory                                   | The surfaces carry versions, counts, and sizes only; no layer name, no text, no file id, no token                             |
| 8   | Stale epoch acceptance                         | `FR16`, `AT09`, `AT15`                      | Replay a frame from a previous connection epoch, use a cursor from a previous grant epoch                        | The stale frame is discarded, the stale cursor is refused with `CURSOR_EXPIRED`                                               |

## 4. Evidence

Every flow produces a report. The evidence of a validation run is the set of:

- the PentAGI flow report per threat class, stored under `security/pentagi/evidence/<timestamp>/`;
- the daemon log of the assessment run, which must show the refusals with their error codes and no
  design content;
- the list of the flows with their identifiers, the model provider, and the exact commit of the
  daemon that was tested;
- the diff against the previous run, so a regression is visible.

A finding is only a finding after a human reviews the agent output. The agent is a tester, not an
authority: an autonomous report is a hypothesis until a person reproduces it. The
`collect-evidence.sh` script writes that rule into the manifest of every evidence directory.

## 5. The local layer: a deterministic adversarial suite

The PenAGI assessment needs Docker, an LLM provider, and an API token, and it produces results
that depend on the model. It is therefore not the only layer, and it is not the first layer.

The first layer is a deterministic adversarial suite in the repository
(`crates/e2e/tests/adversarial.rs`) that encodes the same eight threat classes against the
shipped daemon with no model in the loop. It runs in CI, it is fast, and it never flakes. When
the PenAGI run finds something, the deterministic suite is where the fix is proved.

The two layers answer different questions:

|             | Deterministic suite                          | PentAGI assessment                                          |
| ----------- | -------------------------------------------- | ----------------------------------------------------------- |
| Question    | Does the daemon refuse the documented cases? | Does an autonomous attacker find a way we did not think of? |
| Determinism | Total                                        | Model dependent                                             |
| Cost        | Seconds, in CI                               | Provider budget, human time                                 |
| Coverage    | The documented threat classes                | The undocumented variations                                 |
| Evidence    | A test result per threat                     | A written report per flow                                   |

The PRD acceptance tests `AT06`, `AT13`, and `AT17` are covered by the deterministic suite. The
PentAGI assessment is the adversarial counterpart of those tests: it does not confirm that a
requirement is met, it tries to break it.

## 6. Running the assessment

The procedure, the files, and the exact commands live in `security/pentagi/README.md`, and the
scripts live in `scripts/pentagi/`. The short version:

1. start the target: `scripts/pentagi/start-target.sh` brings up the daemon on a dedicated port
   with a dedicated runtime directory;
2. bring up PentAGI with the safe defaults in `security/pentagi/.env.example` and
   `security/pentagi/docker-compose.override.yml`;
3. create the flows: `scripts/pentagi/run-assessment.sh` submits one flow per threat class through
   the GraphQL API;
4. collect the evidence: `scripts/pentagi/collect-evidence.sh` gathers the reports, the daemon log,
   and the manifest.

Two safety properties of the upstream platform matter here and are pinned in the override file:
the PentAGI web console binds to `127.0.0.1`, and the flow sandbox ports are published on
`DOCKER_PUBLIC_IP`, which the override sets to `127.0.0.1` so a flow cannot offer a callback port
on every interface of the machine.

The one deliberate exposure needs a word of care. The daemon binds the loopback literal only, so a
sandbox on a Docker bridge network cannot reach it at all: such a sandbox reaches the host through
the gateway address, and the daemon never listens there. The assessment therefore runs the flow
sandboxes with `DOCKER_NETWORK=host`, so that `127.0.0.1` inside a sandbox is the loopback of the
host and the daemon is reachable exactly the way a local process would reach it.

The price of that choice is real and is stated in the runbook: with the host network stack, the
sandbox listeners are not port mapped, so `DOCKER_PUBLIC_IP` does not constrain them. The
assessment is therefore only ever run against a dedicated daemon on a dedicated port with a
dedicated runtime directory, on a machine whose owner has accepted that an autonomous agent can
open listeners on the loopback interface for the duration of the run.

## 7. Status

| Item                            | State                                                                                                                                                                                                                                                                                                                    |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Deterministic adversarial suite | Implemented in `crates/e2e/tests/adversarial.rs`, twenty tests, runs in CI                                                                                                                                                                                                                                               |
| PentAGI flows                   | Written in `security/pentagi/flows/`, six threat classes                                                                                                                                                                                                                                                                 |
| PentAGI automation scripts      | Written in `scripts/pentagi/`, shell and PowerShell twins                                                                                                                                                                                                                                                                |
| PenAGI run                      | **Not executed.** Docker is not available on the authoring machine, so no result of a PentAGI run is claimed anywhere in this repository. The first run belongs to a maintainer with Docker, an LLM provider, and the authority to test the local machine.                                                               |
| Findings so far                 | Three daemon defects, found by the deterministic suite and by reading the control path against the threat model: the control endpoints were reachable from a browser origin, the daemon performed no scope check of its own, and a chunk could announce an unbounded transfer. All three are fixed and covered by tests. |
| Gate                            | A PentAGI run is a release gate candidate for `G2`; it is not a gate for `G0`                                                                                                                                                                                                                                            |

## 8. The gap the validation found, and the fix

Reading the control endpoints against the threat model exposed a mismatch between the two
documents. The threat model says that a foreign site may reach the daemon and that the token
remains the authorization boundary, with the `Origin: null` case named as the residual risk. That
reasoning holds for the two authenticated channels, where a credential is required. It did not
hold for the control endpoints:

- `POST /control/pair` mints a one time pairing secret. A web page in a sandboxed iframe sends
  `Origin: null`, which the guard admitted, and the side effect happened even though the page
  could not read the response. The attacker learned nothing directly, but it could seed a
  phishing flow in which the user is asked to enter a pairing code.
- `POST /control/shutdown` stopped the daemon. The same page could stop the local bridge at will.

The deterministic adversarial suite reproduced both, and found two more defects in the same pass:
the daemon performed no scope check of its own, so the daemon half of the double enforcement
(`FR03`) was missing, and a chunk frame could announce a transfer of any size.

The fixes are in the daemon and in the callers, not in the documentation:

1. the control endpoints refuse any request that carries an `Origin` header at all, because a
   native client never sends one;
2. the control endpoints require a dedicated request header, which forces a cross origin browser
   request into a preflight that the daemon never answers;
3. the request builder for the control endpoints lives in the daemon, so no caller can forget the
   header: the CLI, the daemon tests, and the acceptance and adversarial suites all use it;
4. the daemon checks the scope of every node id on its own, against the tree it has proven from
   the answers it received, and refuses a node it has never seen inside the grant;
5. the daemon refuses a chunk that announces more than the transfer budget, and tells the peer
   with a `PAYLOAD_TOO_LARGE` error frame instead of dropping the announcement.

After the fix, a browser cannot reach the control endpoints, the residual risk named in the
threat model shrinks to the two authenticated channels, where a credential really is the
authorization boundary, and the daemon enforces the scope on both sides of the bridge. The
twenty adversarial tests are the evidence.
