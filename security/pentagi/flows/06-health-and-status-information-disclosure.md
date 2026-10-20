# Flow 06: information disclosure through the health and status surfaces

Status: `Proposed`

This flow asks what the daemon tells a caller that reaches it without any credential. The health
endpoint is meant to be a sanitized status: counters, versions, and sizes, never a credential, never
a pairing secret, never design content, and never the identity of a session.

## Threat covered

| Item                 | Value                                                                     |
| -------------------- | ------------------------------------------------------------------------- |
| Threat rows          | `TH03` information disclosure, `TH13` repudiation                         |
| Requirements         | `FR18`                                                                     |
| Acceptance test      | `AT06` (nothing is served before the authentication), `AT13` (the parsers survive extreme input) |
| PRD reference        | `FGT-PRD-001` section 8.6.1 rows `TH03` and `TH13`, section 8.7           |
| Threat model row     | `docs/05-security/01-threat-model.md` section 7, attack surface inventory  |
| Target               | `127.0.0.1:39047`, the FigTether daemon on the assessment port            |

## Target description

The text between the markers is the exact string that is submitted to the `createFlow` mutation of
the PentAGI API.

<!-- BEGIN TARGET -->
Assess the information that the FigTether daemon that listens on 127.0.0.1:39047 on this machine hands out without any credential. The daemon is the target of this flow and nothing else is.

GET /health answers with a status in JSON and needs no credential. The status is meant to be sanitized: versions, counters, sizes, and the number of live things, never a credential, never a pairing secret, never design content, and never the identity of a session.

Your job is to find out what the status surfaces leak. Work only against 127.0.0.1:39047 and run at least these probes. Write down every request and every answer exactly as you sent and received them.

1. Call GET http://127.0.0.1:39047/health with no credential and write down every field of the answer and its value. Name the fields that an outsider could use to fingerprint the installation.
2. Call GET http://127.0.0.1:39047/health with a wrong Host header and with a foreign Origin header, and record the answer. A refused call that still carries information would be a finding.
3. Call GET http://127.0.0.1:39047/health with an Authorization header, with a query string, with a path that walks out of the route, for example "/health/../daemon.json" and "/../figtether.log", and with a different method, for example HEAD and POST. Record every answer. The daemon has to answer with the same sanitized status and with nothing else, and it must not serve a file of the runtime directory.
4. Mint a pairing secret with POST http://127.0.0.1:39047/control/pair, open a WebSocket to ws://127.0.0.1:39047/bridge, spend the secret, and call GET http://127.0.0.1:39047/health again. Compare the two answers field by field. Write down every field that changed, and say whether any field carries the session identity, the session credential, the granted roots, or a node name.
5. Look at the capabilities array of the status before and after the session exists, and write down every entry. Say whether a capability name, a reason, or a state could tell an outsider what the editor or the account can do.
6. Check the size of the answer and the shape of the counters. Say whether the counters could be used to follow the activity of a user, for example the number of exports, the number of bytes served, or the mean latency.

Report every finding as three parts: the request you sent, the exact answer you received, and the conclusion you draw from it. A status that carries no secret is a result, not a weakness. Do not report a problem you did not observe, and do not report a field name as a leak when the value carries nothing sensitive.
<!-- END TARGET -->

## Rules that the flow has to follow

1. The only target of this flow is the FigTether daemon at `127.0.0.1:39047`. Every request goes to
   that address and to that port, and to nothing else.
2. Do not scan, ping, enumerate, or connect to any other host, IP address, domain name, or port.
   Do not run a port scan or a host discovery tool at all.
3. Do not touch anything on the machine that is not the FigTether daemon: no other port, no other
   service, no file outside the flow container, no other process, no other container. In particular,
   do not try to read the runtime directory of the daemon or any other file of the host.
4. Do not install software, do not change the daemon configuration, and do not try to make the
   daemon bind another address or another port.
5. The only secret the flow may use is the one time pairing secret that the flow mints itself
   through the control endpoint. Do not guess, brute force, or look for any other credential.
6. Never write the value of a secret or of a session credential into the report.
7. Every action has to be reproducible from the report: write the exact request and the exact answer
   into the report.
8. When a step cannot be carried out inside these rules, do not attempt it. Write
   "not attempted: out of scope" in the report and continue with the remaining steps.
9. Stop as soon as the daemon stops answering. Do not restart it and do not use another service to
   restart it. The operator restarts the target.

## What the report has to contain

1. The full field list of the health answer, with the value of every field, and the mark of the
   fields that a stranger could use to fingerprint the installation.
2. The two answers of step 4, side by side, with every field that changed.
3. The list of the paths and methods that the daemon answered with something other than the
   sanitized status.
4. The capabilities array before and after the session exists.
5. A final statement, in one paragraph, of what the status surface of the daemon on
   `127.0.0.1:39047` tells a caller that holds no credential.

## Expected outcome

The health answer carries versions, the instance identity, the port, the bind address, uptime,
counters, and sizes, and it carries no credential, no pairing secret, no session identity, no
granted root, and no design content. A request with a wrong `Host` header or with a foreign
`Origin` header is refused with the status code `403` and the body `{"code":"PERMISSION_DENIED"}`
and carries no field of the status. A path that walks out of the route and a wrong method are
answered with the status code of the router and with no file of the runtime directory. The counters
move with the activity, so a finding is only "the status leaks" when the flow shows a value that
carries a secret, an identity, or design content.
