# Postman parity map

Research for the user's request (2026-10-16): "look exactly like Postman, with all the features, not just API testing". Sources are Postman's Learning Center (v12): [Navigating Postman](https://learning.postman.com/latest-v-12/docs/getting-started/basics/navigating-postman), [Postman elements](https://learning.postman.com/docs/getting-started/basics/postman-elements), [Responses](https://learning.postman.com/docs/sending-requests/response-data/responses/), [Authorization types](https://learning.postman.com/docs/sending-requests/authorization/authorization-types/), plus release notes up to October 2026.

"Look like Postman" means the same layout and workflow: regions, tabs, panes, labels. Ferry keeps its own name, icon and colors, and copies no Postman logos or artwork.

Each feature falls into one of three tiers:
- **A**: already in the Ferry spec (F01–F13).
- **B**: runs locally, but is a v1 anti-goal in `docs/kickoff.md`. Building it means reversing that anti-goal.
- **C**: needs Postman's cloud. There's no local equivalent without running a server.

## 1. Layout

| Region | Postman | Tier | Notes |
|---|---|---|---|
| Header | Back/forward, workspace switcher, search (Ctrl+K), command palette (Ctrl+Shift+P), notifications, settings, avatar | A (search, palette, settings) / C (workspaces, avatar, invites) | One local workspace: a folder on disk |
| Left sidebar | Collections, Environments, History, plus Specs, Flows and Mock servers; search; **+** new; per-item context menu | A / B | Collections, Environments, History first |
| Workbench tabs | New tab, drag to reorder, italic preview tabs, unsaved-changes dot, tab search, duplicate, close others | A (F07) | |
| Environment selector and variables pane | Top right of the workbench | A (F08) | |
| Request builder | Method, URL, Send; tabs **Params, Authorization, Headers, Body, Scripts, Settings** | A / B (Scripts) | |
| Response pane | Status, time (hover breakdown), size (hover breakdown), network info; tabs **Body, Cookies, Headers, Test Results** | A / B (Test Results) | |
| Right sidebar | Code snippets, Info, Variables in request, Comments, AI agent | B (code snippets) / C (comments, AI) | |
| Footer | Console, Cookies, Runner, Find and replace, Vault, Globals, Trash, layout toggles | A / B | |

## 2. Requests

| Feature | Tier | Notes |
|---|---|---|
| Methods, URL, path and query params table, bulk edit | A (F01) | |
| Headers table, auto-generated headers, presets | A (F02) | |
| Body: none, form-data (with files), x-www-form-urlencoded, raw (Text/JSON/XML/HTML/JS), binary | A (F03) | |
| Body: GraphQL (query, variables, schema introspection, autocomplete) | B | Anti-goal: GraphQL |
| Auth: No Auth, Inherit, API Key, Bearer, Basic | A (F04) | |
| Auth: Digest, JWT Bearer, AWS Signature, Hawk, NTLM, Akamai EdgeGrid, ASAP | B | Local signing, no server needed |
| Auth: OAuth 1.0, OAuth 2.0 (all grants, token management, PKCE) | B | Anti-goal: OAuth2 flows. Needs a loopback redirect and the system browser |
| Settings: HTTP version, SSL verification, follow redirects, max redirects, encode URL automatically, disable cookie jar, TLS versions and ciphers | A (F05) / B | |
| Client certificates, custom CA, proxy (system, custom, per-host bypass) | B | |
| Cookie jar and cookie manager (per domain) | B | |
| Variables: global, collection, environment, local, data; `{{var}}` highlighting and hover | A (F08) / B (global, local, data) | |
| Dynamic variables (`{{$guid}}`, `{{$timestamp}}`, `{{$randomInt}}` and so on) | B | |
| Vault (secrets kept out of exports) | B | Uses the OS keychain |
| Saved responses ("examples") | B | |

## 3. Responses

| Feature | Tier | Notes |
|---|---|---|
| Status, time, size, body, headers | A (done in Phase 1) | |
| Time breakdown (DNS, TCP, TLS, first byte, download) | A (F06-AC2) | |
| Size breakdown, network info (IPs, HTTP version, certificate) | B | |
| Body as JSON, XML, HTML, YAML, JS, Markdown, Raw, Hex, Base64; auto-detected | A / B | |
| Search in body (Ctrl+F) | A (F06) | |
| JSONPath / XPath filter | B | |
| Cookies tab | B | |
| Test Results tab (pass, skip and fail filters, rerun) | B | Needs scripts |
| Preview (rendered HTML, images, audio, video) | B | Images are cheap. HTML, audio and video need an embedded web view: tens of MB more memory |
| Visualize (HTML templates from scripts) | B | Needs scripts and a web view |
| Server-sent events stream | B | |
| Save response to file, clear | A | |

## 4. Organization and workflow

| Feature | Tier | Notes |
|---|---|---|
| Collections with folders, descriptions, collection-level auth, variables and scripts | A (F09) / B (scripts) | |
| History | A (F10) | |
| Import: Postman collection v2.1, Postman environment, cURL | A (F11) | |
| Import: OpenAPI 3 / Swagger 2, HAR, RAML, WSDL | B | OpenAPI and HAR are worth it; RAML and WSDL are rare |
| Export: Postman v2.1, cURL | A (F11) | |
| Code snippets (cURL, Python, JS fetch, Go, Rust, Java and others) | B | Generated from the request model, pure functions in `ferry-core` |
| Pre-request and post-response scripts with the `pm` API (`pm.test`, `pm.expect`, `pm.environment`, `pm.response` and more) | B | Anti-goal: scripts. Needs an embedded JS engine; QuickJS via `rquickjs` adds about 1–2 MB to the binary and a few MB per run |
| Collection Runner: order, iterations, delay, CSV/JSON data files, results export | B | |
| CLI runner (like Newman / Postman CLI) for CI | B | New `ferry-cli` binary sharing `ferry-app` |
| Console (raw request/response log, script logs) | B | |
| Find and replace across collections | B | |
| Local mock server from a collection's examples | B | Anti-goal: mock servers. Runs on localhost |
| API documentation generated from a collection (HTML/Markdown export) | B | |
| Spec editor (OpenAPI) with linting and collection sync | B | Large |
| Flows (visual low-code canvas) | B | Very large; last |
| Monitors (scheduled runs with alerts) | B / C | Locally, the CLI runner on a schedule. Hosted monitors are cloud-only |
| Performance tests (virtual users) | B | Local load generation is feasible later |

## 5. Protocols

| Protocol | Tier | Notes |
|---|---|---|
| HTTP/1.1, HTTP/2 | A | Done |
| GraphQL | B | Anti-goal |
| WebSocket | B | Anti-goal. `tokio-tungstenite` |
| Socket.IO | B | Built on WebSocket |
| Server-sent events | B | |
| gRPC (proto import, server reflection, streaming) | B | Anti-goal. `tonic` plus dynamic messages; large |
| MQTT | B | `rumqttc` |
| SOAP | A / B | A POST with an XML body; WSDL import is B |

## 6. Cloud only (tier C)

These can't be built without running and hosting a service: accounts and sign-in, team workspaces and invites, cloud sync, comments, forks and pull requests on collections, the Public API Network, the API Catalog, hosted mock servers and monitors, Insights, Agent Mode (AI), and the Interceptor browser extension.

Local alternatives that cover most of the value:
- **Sharing:** collections are plain files (F09), so a git repo is the workspace.
- **Monitors:** the CLI runner under cron or CI.

## 7. Cost to the project's goals

- **Memory budgets** (idle RSS within 2x an empty window, 10 tabs) stay achievable for tiers A and B, with two exceptions: HTML Preview and Visualize need a web view. It is created only while one of those tabs is open (ADR-0028).
- **Binary size** grows from scripts (QuickJS), gRPC (`tonic`, `prost`) and WebSocket. Each gets its own re-baseline ADR, like ADR-0021.
- **Effort:** tier A is the existing plan (Phases 2–4). Tier B roughly triples it.

## 8. Decision

The user chose local parity (tiers A and B) with the web view included by default (ADR-0027, ADR-0028). Feature IDs F14–F35 and the phase plan are in [`SPEC.md`](SPEC.md#phases).
