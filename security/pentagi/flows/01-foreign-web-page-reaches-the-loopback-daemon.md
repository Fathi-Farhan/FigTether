# Flow 01: a foreign web page that reaches the loopback daemon

Status: `Proposed`

This flow asks whether a page that a browser loads from a foreign web site can reach the
FigTether daemon that listens on the loopback interface. The daemon is expected to refuse every
request that does not come from the Figma plugin UI or from a local client, and the flow checks
the three gates that enforce that: the bind address, the `Host` header, and the `Origin` header.

## Threat covered

| Item                 | Value                                                                     |
| -------------------- | ------------------------------------------------------------------------- |
| Threat rows          | `TH01` spoofing and information disclosure, `TH02` DNS rebinding         |
| Requirements         | `FR02`, `FR19`                                                            |
| Acceptance test      | `AT06` (wrong `Host` and wrong `Origin` are refused)                      |
| PRD reference        | `FGT-PRD-001` section 8.6.1 rows `TH01` and `TH02`, section 8.5.4         |
| Threat model row     | `docs/05-security/01-threat-model.md` section 3, boundary `T3`            |
| Target               | `127.0.0.1:39047`, the FigTether daemon on the assessment port            |

## Target description

The text between the markers is the exact string that is submitted to the `createFlow` mutation of
the PentAGI API.

<!-- BEGIN TARGET -->
Assess the local trust model of the FigTether daemon that listens on 127.0.0.1:39047 on this machine. The daemon is the target of this flow and nothing else is.

The daemon is a loopback service with five routes: GET /health, POST /control/pair, POST /control/shutdown, GET /bridge (the WebSocket of the Figma plugin), and GET /client (the WebSocket of the local clients). It is supposed to be reachable only from this machine, and only from the paired plugin user interface or from a local client that holds a credential.

Your job is to find out whether a page that a browser loads from a foreign web site, or a name that an attacker controls, can reach the daemon on 127.0.0.1:39047 and make it do something. Work only against 127.0.0.1:39047.

Run at least these probes and write down every request and every answer exactly as you sent and received them.

1. A plain request to http://127.0.0.1:39047/health with an ordinary Host header. Record the answer and the fields it carries.
2. The same request with a forged Host header that names a foreign host, for example "Host: attacker.example", and with a Host header that names the right host but the wrong port, for example "Host: 127.0.0.1:9999".
3. The same request with an Origin header of a foreign site, for example "Origin: https://evil.example", with "Origin: null", and with no Origin header at all. A browser that loads a page in a sandboxed frame sends "Origin: null", so that case matters.
4. The same set of headers against the WebSocket routes GET /bridge and GET /client. Send the upgrade headers a browser sends, and check whether the connection is refused before any frame is served.
5. A request with a long Host header, with a Host header that contains a port suffix, and with a Host header that contains a userinfo part such as "Host: attacker.example@127.0.0.1:39047".
6. A short reasoning step about DNS rebinding: the daemon only listens on the loopback address, so write down under which conditions a browser can be made to send a request to 127.0.0.1:39047 and which of the checks above stops it.

Report every finding as three parts: the request you sent, the exact answer you received, and the conclusion you draw from it. Say plainly when a check refuses you, because that is the result the maintainers need. Do not report a problem you did not observe, and do not report a refusal as a weakness.
<!-- END TARGET -->

## Rules that the flow has to follow

1. The only target of this flow is the FigTether daemon at `127.0.0.1:39047`. Every request goes to
   that address and to that port, and to nothing else.
2. Do not scan, ping, enumerate, or connect to any other host, IP address, domain name, or port.
   Do not run a port scan or a host discovery tool at all, not even against 127.0.0.1. Use single
   requests to the five named routes.
3. Do not touch anything on the machine that is not the FigTether daemon: no other port, no other
   service, no file outside the flow container, no other process, no other container.
4. Do not install software, do not change the daemon configuration, and do not try to make the
   daemon bind another address or another port.
5. Do not guess, brute force, or look for credentials that the flow did not mint itself.
6. Every action has to be reproducible from the report: write the exact request and the exact answer
   into the report.
7. When a step cannot be carried out inside these rules, do not attempt it. Write
   "not attempted: out of scope" in the report and continue with the remaining steps.
8. Stop as soon as the daemon stops answering. Do not restart it and do not use another service to
   restart it. The operator restarts the target.

## What the report has to contain

1. A table with one row per probe: the headers you sent, the status line and the body you received.
2. The list of the header combinations that were refused, with the exact status code and body.
3. The reasoning about DNS rebinding, with the condition under which a browser would be able to send
   a request to the loopback port.
4. A final statement, in one paragraph, of whether a foreign web page can reach the daemon on
   `127.0.0.1:39047` and what it can achieve when it does.

## Expected outcome

The daemon refuses every request with a wrong `Host` header and every request with a foreign
`Origin` header with the status code `403` and the body `{"code":"PERMISSION_DENIED"}`, and it
refuses the WebSocket upgrade on the same grounds before any frame is served. A request without an
`Origin` header is admitted, because a non browser client sends none. `Origin: null` is admitted as
well, and that is the case a reviewer has to judge, because a sandboxed frame of a foreign page
sends it. A finding is therefore only "a foreign page can drive the daemon" when the flow shows a
request with a browser header set that the daemon accepts and acts on.
