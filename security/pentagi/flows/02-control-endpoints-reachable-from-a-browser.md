# Flow 02: the control endpoints reached from a browser

Status: `Proposed`

This flow asks whether the two control endpoints of the daemon, the one that mints a pairing secret
and the one that stops the daemon, can be driven by a page that a browser loads from a foreign web
site. Both endpoints are plain HTTP posts without a body, so a browser can send them as a simple
request, and a simple request does not need a preflight.

## Threat covered

| Item             | Value                                                                     |
| ---------------- | ------------------------------------------------------------------------- |
| Threat rows      | `TH01` spoofing and information disclosure, `TH03` information disclosure |
| Requirements     | `FR02`, `FR19`                                                            |
| Acceptance test  | `AT06` (a wrong `Origin` is refused, nothing is served before the auth)   |
| PRD reference    | `FGT-PRD-001` section 8.6.1 rows `TH01` and `TH03`, section 8.5.4         |
| Threat model row | `docs/05-security/01-threat-model.md` section 7, attack surface inventory |
| Target           | `127.0.0.1:39047`, the FigTether daemon on the assessment port            |

## Target description

The text between the markers is the exact string that is submitted to the `createFlow` mutation of
the PentAGI API.

<!-- BEGIN TARGET -->

Assess the two control endpoints of the FigTether daemon that listens on 127.0.0.1:39047 on this machine. The daemon is the target of this flow and nothing else is.

POST /control/pair mints a one time pairing secret that a plugin user interface spends to open a session. POST /control/shutdown asks the daemon to stop gracefully. Both endpoints answer with a small JSON body, both take no request body, and both are plain posts, so a browser can send them as a simple request without asking the daemon for permission first. A simple request that carries no readable answer is still enough to stop the daemon, and that is the risk this flow has to measure.

Work only against 127.0.0.1:39047 and run at least these probes. Write down every request and every answer exactly as you sent and received them.

1. POST http://127.0.0.1:39047/control/pair with an ordinary Host header and no Origin header. Record the status code and the body, and note whether the body carries a secret.
2. POST http://127.0.0.1:39047/control/pair with "Origin: https://evil.example", then with "Origin: null", then with "Origin: http://127.0.0.1:39047". Record the status code and the body of each.
3. The same three requests against POST http://127.0.0.1:39047/control/shutdown. Be careful: a successful shutdown stops the daemon, so do this part last, and be ready to report that the daemon stopped answering.
4. A request with the method GET against both control routes, and a request with the method POST against GET /health, to see which routes and methods the router accepts.
5. A request that uses the Host header "localhost:39047" and one that uses "[::1]:39047", because the daemon admits more than one loopback name.
6. A reasoning step about the browser: explain, in the report, whether a page on a foreign site could send the two control requests without a preflight, which of the checks would stop it, and what an attacker would gain from a stop request. Consider that a response without a cross origin header cannot be read by the page, but the effect of the request still happens.

Report every finding as three parts: the request you sent, the exact answer you received, and the conclusion you draw from it. Do not report a refusal as a weakness, and do not report a problem you did not observe.
<!-- END TARGET -->

## Rules that the flow has to follow

1. The only target of this flow is the FigTether daemon at `127.0.0.1:39047`. Every request goes to
   that address and to that port, and to nothing else.
2. Do not scan, ping, enumerate, or connect to any other host, IP address, domain name, or port.
   Do not run a port scan or a host discovery tool at all.
3. Do not touch anything on the machine that is not the FigTether daemon: no other port, no other
   service, no file outside the flow container, no other process, no other container.
4. Do not install software, do not change the daemon configuration, and do not try to make the
   daemon bind another address or another port.
5. A pairing secret that the flow mints itself stays inside the flow. Write it down only as a fact
   about whether it was issued, never as its value.
6. Every action has to be reproducible from the report: write the exact request and the exact answer
   into the report.
7. When a step cannot be carried out inside these rules, do not attempt it. Write
   "not attempted: out of scope" in the report and continue with the remaining steps.
8. Stop as soon as the daemon stops answering. Do not restart it and do not use another service to
   restart it. The operator restarts the target.

## What the report has to contain

1. A table with one row per request: the method, the route, the headers, the status code, and the
   body.
2. The list of the requests that were refused, with the exact status code and body.
3. The reasoning about the browser semantics, with the conclusion whether a foreign page can mint a
   pairing secret or can stop the daemon.
4. A final statement, in one paragraph, of what a foreign page can reach on
   `127.0.0.1:39047` and what it cannot reach.

## Expected outcome

Both control endpoints refuse a request with a foreign `Origin` header with the status code `403`
and the body `{"code":"PERMISSION_DENIED"}`, and they mint or stop only for a request that comes
from the loopback interface with an admitted `Host` header. The interesting result for the reviewer
is the `Origin: null` case: a sandboxed frame of a foreign page sends it, and the daemon admits it,
so a finding is only justified when the flow shows that a stop request with that header really
stops the daemon and that a pair request with that header really issues a secret.
