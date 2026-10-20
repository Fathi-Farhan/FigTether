# Flow 05: resource exhaustion and the single editor operation slot

Status: `Proposed`

This flow asks how much work the daemon accepts before it refuses, and whether the refusal is
structured. The daemon runs one editor operation at a time inside a session, one export lane for
the whole installation, a bounded queue, and a bounded frame size. The flow checks that the bounds
hold, that the daemon keeps answering while it is under pressure, and that it does not crash.

## Threat covered

| Item             | Value                                                                                                       |
| ---------------- | ----------------------------------------------------------------------------------------------------------- |
| Threat rows      | `TH06` denial of service, `TH11` tampering                                                                  |
| Requirements     | `FR07`, `FR13`, `FR14`                                                                                      |
| Acceptance test  | `AT08` (queue overload and mixed soak stay inside the contract), `AT13` (the parsers survive extreme input) |
| PRD reference    | `FGT-PRD-001` section 8.6.1 row `TH06`, section 8.5.5                                                       |
| Threat model row | `docs/05-security/01-threat-model.md` section 4, row `TH06`                                                 |
| Target           | `127.0.0.1:39047`, the FigTether daemon on the assessment port                                              |

## Target description

The text between the markers is the exact string that is submitted to the `createFlow` mutation of
the PentAGI API.

<!-- BEGIN TARGET -->

Assess the resource limits of the FigTether daemon that listens on 127.0.0.1:39047 on this machine. The daemon is the target of this flow and nothing else is.

The daemon runs one editor operation at a time inside a session, one export lane for the whole installation, a queue that holds a bounded number of requests, and a bounded frame size. A request that does not fit the budget has to be refused with a structured answer that names a code and a retry hint, and the daemon has to keep answering while it is under pressure.

Work only against 127.0.0.1:39047. Run at least these probes and write down every request and every answer exactly as you sent and received them. Keep the pressure low enough that the machine stays usable, and stop as soon as the daemon stops answering.

1. Open many WebSocket connections to ws://127.0.0.1:39047/bridge and to ws://127.0.0.1:39047/client and never send a frame on them. The daemon waits a few seconds for the first frame of every connection. While the connections are open, call GET http://127.0.0.1:39047/health repeatedly and write down how long the answer takes and whether the answer still comes. Record how many connections were open at the same time.
2. Mint a series of pairing secrets with POST http://127.0.0.1:39047/control/pair without spending them, and watch the field of the health answer that counts the pending pairing secrets. Record the values before and after, and the number of secrets you minted. Do not mint more than a few hundred.
3. Send a frame on an authenticated /bridge connection that is larger than one megabyte, then a frame that is not valid JSON, then a frame whose message type does not exist, then a payload with a deeply nested JSON structure, then a payload with a very long string, then a payload with a number that is larger than the largest number a machine holds. Record the answer of each attempt, and whether the daemon is still healthy afterwards.
4. Open a connection on /client and send a first frame that carries a wrong credential, then a first frame that carries no credential at all, then stay silent. Record the answers and how long the daemon keeps the connection.
5. If, and only if, the operator attached the file /work/uploads/figtether-client-credential.txt to this flow, use the credential it holds to open a client session on ws://127.0.0.1:39047/client, and then send many read requests at the same time so that the single editor operation slot of the session and the single export lane of the installation are occupied. Record the answers, the codes, and the retry hints. If the file is not there, do not look for the credential anywhere else: write "not attempted: the local client credential was not provided" in the report and continue with the other steps.

Report every finding as three parts: the request or frame you sent, the exact answer you received, and the conclusion you draw from it. A structured refusal is a result, not a weakness. Do not report a problem you did not observe.
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
   through the control endpoint, and the local client credential when the operator attached it to
   the flow. Do not guess, brute force, or look for any other credential.
6. Keep the pressure low. The point of the flow is to observe the limits, not to break the machine.
   Stop when the daemon answers slower than a few seconds, and never open more than a few hundred
   connections at the same time.
7. Never write the value of a secret or of a credential into the report.
8. Every action has to be reproducible from the report: write the exact request and the exact answer
   into the report.
9. When a step cannot be carried out inside these rules, do not attempt it. Write
   "not attempted: out of scope" in the report and continue with the remaining steps.
10. Stop as soon as the daemon stops answering. Do not restart it and do not use another service to
    restart it. The operator restarts the target.

## What the report has to contain

1. A table with one row per probe: the number of concurrent connections or requests, the answer, the
   elapsed time, and the code of the refusal.
2. The values of the health answer before and after the pressure, with the counters that moved.
3. The list of the frames that the daemon refused, with the exact error code.
4. The statement of whether the daemon kept answering while it was under pressure, and whether it
   crashed.
5. A final statement, in one paragraph, of the state of the resource contract of the daemon on
   `127.0.0.1:39047`, and of the parts of the flow that could not be carried out.

## Expected outcome

Every overload is refused with a structured answer that carries a code, a message, and a retry hint,
and the daemon keeps answering the health endpoint while it refuses the excess. The pending pairing
secrets are counted in the health answer, so a reviewer can see that the table of pending secrets
grows with every minted secret. A frame that is too large, that is not valid JSON, or that carries
an unknown message type is dropped without a crash and without a session. A finding is only "the
daemon can be exhausted" when the flow shows that the daemon stopped answering or that a limit was
crossed without a structured refusal.
