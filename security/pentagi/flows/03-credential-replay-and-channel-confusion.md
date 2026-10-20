# Flow 03: credential replay and credential confusion between the two channels

Status: `Proposed`

This flow asks whether the credential of one channel is accepted by the other channel, and whether
a pairing secret can be spent twice. The daemon has two WebSocket channels with two different
credentials: the plugin channel on `/bridge`, which spends a one time pairing secret, and the local
client channel on `/client`, which presents the installation credential that lives in the runtime
directory of the daemon.

## Threat covered

| Item                 | Value                                                                     |
| -------------------- | ------------------------------------------------------------------------- |
| Threat rows          | `TH03` information disclosure, `TH04` information disclosure, `TH11` tampering |
| Requirements         | `FR02`, `FR16`                                                            |
| Acceptance test      | `AT06` (a wrong, an expired, and a replayed secret are all refused)      |
| PRD reference        | `FGT-PRD-001` section 8.6.1 rows `TH03` and `TH04`, section 8.5.4         |
| Threat model row     | `docs/05-security/01-threat-model.md` section 2, assets `AS2` and `AS3`  |
| Target               | `127.0.0.1:39047`, the FigTether daemon on the assessment port            |

## Target description

The text between the markers is the exact string that is submitted to the `createFlow` mutation of
the PentAGI API.

<!-- BEGIN TARGET -->
Assess the credential handling of the FigTether daemon that listens on 127.0.0.1:39047 on this machine. The daemon is the target of this flow and nothing else is.

The daemon has two WebSocket channels with two different credentials. GET /bridge is the plugin channel. Its first frame must be a hello_auth frame whose payload carries the secret, the client id, and the protocol version, and the daemon answers with a session identity, a session credential, a connection epoch, and the capabilities of the daemon. The secret is one time: it may be spent once, and it expires after five minutes. GET /client is the local client channel. Its first frame must carry the credential of the local installation, the client id, and the protocol version. The two credentials belong to different worlds and neither may work on the other channel.

Work only against 127.0.0.1:39047 and run at least these probes. Write down every request, every frame, and every answer exactly as you sent and received them. Never write the value of a secret into the report, only the fact that a value was accepted or refused.

1. Mint a pairing secret with POST http://127.0.0.1:39047/control/pair. Open a WebSocket to ws://127.0.0.1:39047/bridge and spend the secret in the hello_auth frame. Record the session identity, the connection epoch, the grant epoch, and the capabilities of the answer. Keep the session credential of the answer, it belongs to the session you just opened.
2. Replay the same secret in a second hello_auth frame on a second WebSocket to /bridge. Record the answer. A replay has to be refused, and the refusal must not open a session.
3. Present the same pairing secret on the /client channel as if it were the installation credential, and present a made up value as a pairing secret on the /bridge channel. Both have to be refused. Record both answers.
4. Open a second WebSocket to /bridge and send a hello_auth frame that carries a wrong protocol version, then one that carries a wrong client id, then one that is not a hello_auth frame at all. Record every answer, and check that no session metadata and no capability information arrives before the authentication.
5. Send a frame on an authenticated /bridge connection that carries a connection epoch that is not the one of that connection, and then a frame that carries the connection epoch of the connection but a session identity that belongs to another session. Record whether the daemon answers, drops the frame, or closes the connection.
6. A timing note, not a measurement: describe how the daemon compares the two credentials and whether the answer of the /client channel differs in any way from the answer of the /bridge channel.

Report every finding as three parts: the request or frame you sent, the exact answer you received, and the conclusion you draw from it. A refusal is a result, not a weakness. Do not report a problem you did not observe.
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
5. The only secret the flow may use is the one time pairing secret that the flow mints itself
   through the control endpoint. Do not guess, brute force, or look for any other credential.
6. Never write the value of a secret, of a session credential, or of a pairing secret into the
   report. Write only whether a value was accepted or refused.
7. Every action has to be reproducible from the report: write the exact frame and the exact answer
   into the report.
8. When a step cannot be carried out inside these rules, do not attempt it. Write
   "not attempted: out of scope" in the report and continue with the remaining steps.
9. Stop as soon as the daemon stops answering. Do not restart it and do not use another service to
   restart it. The operator restarts the target.

## What the report has to contain

1. A table with one row per probe: the channel, the frame or request you sent, the answer, and the
   session identity or error code you received.
2. The list of the answers that were refused, with the exact error code.
3. The statement of whether a pairing secret can be spent twice, and whether a credential of one
   channel is accepted by the other channel.
4. A final statement, in one paragraph, of the state of the credential lifecycle of the daemon on
   `127.0.0.1:39047`.

## Expected outcome

A replayed pairing secret is refused with the error code `AUTH_REQUIRED`, a wrong or an unknown
secret is refused with the same code, an expired secret is refused with `AUTH_EXPIRED`, and the
pairing secret is refused on the client channel while the installation credential is refused on the
plugin channel. Nothing but a session identity arrives before the authentication of a channel, and
a frame that carries a foreign connection epoch or a foreign session identity is dropped without an
answer. A finding is only "the two channels accept each other" when the flow shows an answer that
opens a session or serves data.
