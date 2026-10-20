# Flow 04: scope escape between two sessions

Status: `Proposed`

This flow asks whether one session can reach data, a cursor, or an asset that belongs to another
session. The daemon serves up to four live sessions at the same time, every session carries its own
grant, its own grant epoch, and its own connection epoch, and the scope of a session is checked
again when a scope moves.

## Threat covered

| Item             | Value                                                                           |
| ---------------- | ------------------------------------------------------------------------------- |
| Threat rows      | `TH05` elevation of privilege, `TH10` cross session confusion, `TH11` tampering |
| Requirements     | `FR03`, `FR04`, `FR16`                                                          |
| Acceptance test  | `AT05` (two sessions stay isolated), `AT17` (membership is checked fresh)       |
| PRD reference    | `FGT-PRD-001` section 8.6.1 rows `TH05` and `TH10`, section 8.5.4               |
| Threat model row | `docs/05-security/01-threat-model.md` section 4, rows `TH05` and `TH10`         |
| Target           | `127.0.0.1:39047`, the FigTether daemon on the assessment port                  |

## Target description

The text between the markers is the exact string that is submitted to the `createFlow` mutation of
the PentAGI API.

<!-- BEGIN TARGET -->

Assess the session isolation of the FigTether daemon that listens on 127.0.0.1:39047 on this machine. The daemon is the target of this flow and nothing else is.

The daemon serves several live sessions at the same time. A session is opened when a plugin spends a one time pairing secret on the WebSocket route GET /bridge. The answer of the daemon carries the session identity, a session credential, a connection epoch, and the capabilities of the daemon. The granted scope of a session arrives later, in a grant_update frame that carries the root node ids the user granted and that the daemon answers with the grant epoch of the new grant.

Your job is to find out whether one session can reach anything that belongs to another session: a different scope, a cursor, an asset, a capability, or a frame that carries a foreign identity. Work only against 127.0.0.1:39047.

Run at least these probes and write down every frame and every answer exactly as you sent and received them.

1. Mint two pairing secrets with POST http://127.0.0.1:39047/control/pair. Open two WebSockets to ws://127.0.0.1:39047/bridge, spend one secret on each, and use a different client id on each. Write down the session identity, the session credential, the connection epoch, and the capabilities of both answers. The two session identities have to be different.
2. Send a grant_update frame on each connection with a different set of root node ids, for example the root "1:2" on the first session and the root "1:2" on the second session as well, because two sessions with identical node ids are the hardest case. Record the grant epoch that the daemon answers with on each connection.
3. On the first connection, send a frame that carries the session identity of the second session, then a frame that carries the connection epoch of the second session, then a grant_update frame that names the root ids of the second session. Record whether the daemon answers, drops the frame, or changes the state of the second session. Compare the state of the second session before and after by sending its own grant_update frame again and by reading its grant epoch.
4. On the first connection, send a grant_update frame that widens the scope, for example a root that was not granted before, and then one that narrows the scope. Record the grant epoch of each answer. A moved scope has to produce a new grant epoch, and the queued work of the old scope has to be dropped.
5. Open a third WebSocket to /bridge and spend the first pairing secret again, then open a fourth and spend the second one again. Record the answers. A session that reconnects keeps its identity but gets a new connection epoch, and the old connection epoch has to stop working.
6. A check of the limits: open more sessions than the daemon allows, and record the error code of the refusal. The daemon serves a small number of live sessions at the same time.

Report every finding as three parts: the frame you sent, the exact answer you received, and the conclusion you draw from it. A refusal is a result, not a weakness. Do not report a problem you did not observe.
<!-- END TARGET -->

## Rules that the flow has to follow

1. The only target of this flow is the FigTether daemon at `127.0.0.1:39047`. Every request and
   every frame goes to that address and to that port, and to nothing else.
2. Do not scan, ping, enumerate, or connect to any other host, IP address, domain name, or port.
   Do not run a port scan or a host discovery tool at all.
3. Do not touch anything on the machine that is not the FigTether daemon: no other port, no other
   service, no file outside the flow container, no other process, no other container. In particular,
   do not try to read the runtime directory of the daemon or any other file of the host.
4. Do not install software, do not change the daemon configuration, and do not try to make the
   daemon bind another address or another port.
5. The only secret the flow may use is the one time pairing secrets that the flow mints itself
   through the control endpoint. Do not guess, brute force, or look for any other credential.
6. Never write the value of a secret or of a session credential into the report.
7. Every action has to be reproducible from the report: write the exact frame and the exact answer
   into the report.
8. When a step cannot be carried out inside these rules, do not attempt it. Write
   "not attempted: out of scope" in the report and continue with the remaining steps.
9. Stop as soon as the daemon stops answering. Do not restart it and do not use another service to
   restart it. The operator restarts the target.

## What the report has to contain

1. A table with one row per session: the client id, the session identity, the connection epoch, the
   granted roots, and the grant epoch.
2. The list of the frames that the daemon dropped, and the list of the frames that it answered.
3. The statement of whether a session was able to change the state, the scope, or the grant epoch of
   another session.
4. A final statement, in one paragraph, of the state of the session isolation of the daemon on
   `127.0.0.1:39047`, and of the limits that refuse further sessions.

## Expected outcome

Two sessions with identical node ids keep two different session identities, two different grant
epochs, and two different capabilities. A frame that carries the identity or the connection epoch of
another session is dropped without an answer and without a change of the other session. A grant
update that moves the scope produces a new grant epoch. A session that reconnects keeps its
identity and receives a new connection epoch, and the old epoch stops working. A session beyond the
limit is refused with the error code `SESSION_LIMIT`. A finding is only "a session escaped its
scope" when the flow shows that the state of one session changed because of a frame of another
session.
