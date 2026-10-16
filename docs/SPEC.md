# Ferry specification

Ferry is a native, low-memory, offline desktop API client with Postman's layout
and its local feature set (ADR-0027). This file is the source of truth for behavior. Every acceptance criterion (AC) has an ID, and every
test that covers it includes the ID in its name (`f06_ac9_...`).

Acceptance criteria are written in full before a feature's phase starts. Features
not yet started list their scope only.

Conventions:
- **Given / When / Then** describe observable behavior through public APIs
  (`ferry-core` functions, `ferry-app` messages and state, `ferry-ui` views).
- Error messages quoted in an AC are exact and covered by snapshot tests.

## Phases

From Phase 2 on, this plan replaces section 7 of `docs/kickoff.md` (ADR-0027). Each phase stops for user review.

| Phase | Features |
|---|---|
| 2 | F14 workbench layout; F01 params and custom methods; F02; F03; F04; F05 redirect and TLS options; F06-AC2 timing breakdown; F07 |
| 3 | F08 variables; F09 collections; F10 history; F17 cookies; F18 console; F32 saved examples; F22 Preview and Visualize |
| 4 | F15 scripts and tests; F16 Collection Runner and CLI |
| 5 | F11 import/export; F19 code snippets; F20 advanced auth and OAuth; F21 proxy and certificates; F31 vault; F30 OpenAPI and HAR import |
| 6 | F23 GraphQL; F24 WebSocket and Socket.IO; F25 SSE; F26 gRPC; F27 MQTT |
| 7 | F28 mock server; F29 docs generation; F33 find and replace; F12; F13; performance hardening; releases |
| Later | F30 spec editor; F34 Flows; F35 performance tests |

---

## F00 Foundations (Phase 0)

| AC | Criterion |
|---|---|
| F00-AC1 | **Given** any view function, **when** it is rendered in `iced_test::Simulator` with no window, **then** tests can find text, simulate input and collect the emitted messages. Pixel snapshots compare byte-exact against committed PNGs. |
| F00-AC2 | **Given** the workspace, **when** `cargo xtask arch` runs, **then** it fails if any crate depends on a workspace crate outside `ui -> app -> {core, http, store}`, `{http, store} -> core`, including dev-dependencies. |
| F00-AC3 | **Given** `budgets.toml`, **when** `cargo xtask budgets` runs, **then** it measures idle RSS against the empty iced window, cold start, 50 MB scroll frame time and binary size, and exits non-zero on any regression. In CI, the timing budgets are advisory (ADR-0005). |
| F00-AC4 | **Given** test coverage, **when** `cargo xtask coverage` runs, **then** it fails if line coverage is below 90% for `ferry-core`, `ferry-app` or `ferry-store`, or below 80% for `ferry-http`. |
| F00-AC5 | **Given** a push or pull request, **when** CI runs, **then** the full gate runs on Linux, macOS and Windows. A nightly job runs long fuzzing and full mutation testing. |

## F01 Request builder

Scope: method, URL, query params table in two-way sync with the URL.

Phase 1 delivered AC1 and AC5. Standard method names match case-insensitively. The Phase 1 UI offers only the standard methods; typing a custom method arrives in Phase 2. AC2 to AC4 and AC6 move to Phase 2 with F02 (ADR-0015). URLs must literally start with `http://` or `https://` followed by a host, which is stricter than the WHATWG parser (ADR-0024).

| AC | Criterion |
|---|---|
| F01-AC1 | **Given** a new request, **then** the method is `GET`. **When** the user picks a method, **then** it is one of GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS, or a custom token matching RFC 9110 `token`. |
| F01-AC2 | **Given** a URL with a query string, **then** the params table lists every pair in order, percent-decoded, with duplicate keys kept. |
| F01-AC3 | **When** a param row is added, edited, removed or disabled, **then** the URL's query updates to match. Disabled rows are left out of the URL. |
| F01-AC4 | **Given** any sequence of URL and table edits, **then** the URL and table stay consistent. Untouched params keep their original order and encoding (property test). |
| F01-AC5 | **Given** a URL without an `http://` or `https://` scheme or without a host, **when** the user sends, **then** nothing is sent and the request shows `Enter a URL starting with http:// or https://`. |
| F01-AC6 | **Given** `{{var}}` placeholders in the URL, **then** URL/table sync preserves them verbatim (F08 resolves them at send time). |
| F01-AC7 | **When** the user types a method name into the method field, **then** a standard name selects that method and any other RFC 9110 token becomes a custom method. An invalid name shows the `MethodError` text and blocks sending. |
| F01-AC8 | **Given** the params table, **then** each row has a key, a value, a description (never sent) and an enabled checkbox. A trailing blank row is always present; typing into it adds a row. |

Phase 2 sync rules: when the URL is edited, the enabled rows are replaced by the URL's pairs while disabled rows keep their positions. When the table is edited, the query is rebuilt from enabled rows: an untouched row keeps its original encoding, an edited row is percent-encoded (keeping `{{...}}` verbatim), and a row with an empty value is written as just its key. The fragment is preserved. An empty query drops the `?`.

## F02 Headers
Scope: editable rows, enable/disable per row, duplicate keys allowed, auto-generated headers, bulk edit.

| AC | Criterion |
|---|---|
| F02-AC1 | **Given** header rows (key, value, description, enabled), **when** the request is sent, **then** every enabled row with a non-blank key is sent in table order, duplicates included. Disabled and blank-key rows are not sent. |
| F02-AC2 | **Given** an enabled header whose name is not an RFC 9110 token, **then** sending is blocked with `Header name "<name>" contains invalid character '<c>'`. **Given** a value containing CR or LF, **then** sending is blocked with `Header "<name>" value cannot contain line breaks`. |
| F02-AC3 | **Given** a request, **then** the Headers tab can show the auto-generated headers: `User-Agent: ferry/<version>`, `Accept: */*`, `Accept-Encoding: gzip, deflate, br, zstd`, `Content-Type` from the body (F03), `Authorization` or the API key header from auth (F04), and `Host` and `Content-Length` as "calculated when sent". They are hidden by default behind a "<n> hidden" toggle. An enabled user header with the same name (case-insensitive) replaces an auto header, and the auto row shows as overridden. `Host` and `Content-Length` cannot be overridden. |
| F02-AC4 | **When** the user switches to bulk edit, **then** rows show as `key: value` lines, with disabled rows prefixed by `//`. Editing the text and switching back rebuilds the rows. Descriptions survive when the key on that line is unchanged. |

## F03 Body
Scope: none, form-data (text and file parts), x-www-form-urlencoded, raw (Text, JSON, XML, HTML, JavaScript), binary. JSON live validation and Beautify. GraphQL bodies are F23.

| AC | Criterion |
|---|---|
| F03-AC1 | **Given** the body modes none, form-data, x-www-form-urlencoded, raw and binary, **when** the user switches modes, **then** each mode keeps its own content and only the selected mode is sent. |
| F03-AC2 | **Given** raw mode, **then** the text is sent exactly as UTF-8. Unless overridden (F02-AC3), `Content-Type` is `text/plain` (Text), `application/json` (JSON), `application/xml` (XML), `text/html` (HTML) or `application/javascript` (JavaScript). |
| F03-AC3 | **Given** raw JSON, **then** invalid JSON shows `Invalid JSON at line <l>, column <c>: <reason>` under the editor, and sending is still allowed. **When** the user presses Beautify, **then** valid JSON is re-indented with 2 spaces, keeping key order and number spelling. |
| F03-AC4 | **Given** x-www-form-urlencoded rows, **then** enabled rows with a non-blank key are sent in order as `application/x-www-form-urlencoded`, encoded as HTML forms do (space as `+`). |
| F03-AC5 | **Given** form-data rows, **then** enabled text and file fields are sent in order as `multipart/form-data`. A file field sends the file's bytes with its file name and a content type guessed from its extension (`application/octet-stream` otherwise). A file that cannot be read fails the send with `Could not read file <path>: <reason>`. |
| F03-AC6 | **Given** binary mode with a file, **then** the file's bytes are the body, with a content type guessed from its extension. Without a file, no body is sent. |

## F04 Auth
Scope: No Auth, Basic, Bearer, API key (header or query). Inherit from parent arrives with F09. Other types are F20.

| AC | Criterion |
|---|---|
| F04-AC1 | **Given** the types No Auth, Basic Auth, Bearer Token and API Key, **when** the user switches type, **then** each type keeps its fields, and only the selected type affects the request. |
| F04-AC2 | **Given** Basic Auth, **then** `Authorization: Basic <base64(username:password)>` is sent, with UTF-8 bytes. |
| F04-AC3 | **Given** Bearer Token with a non-empty token, **then** `Authorization: Bearer <token>` is sent. An empty token sends nothing. |
| F04-AC4 | **Given** API Key with a non-empty key, **then** `<key>: <value>` is sent as a header, or `key=value` is appended to the query (percent-encoded), as chosen. An empty key sends nothing. |
| F04-AC5 | **Given** an enabled user header with the same name as the auth header, **then** the user header wins (F02-AC3). |

## F05 Send

Scope: send, cancel in flight, timeout, follow-redirects toggle, TLS-verify toggle.

Phase 1 delivered AC1, AC6 and AC7. It also delivered parts of AC2 and AC3: Cancel works, and a fixed 30 s timeout applies with its exact error message. Making the timeout configurable belongs to F12. In AC1, Enter in the URL field also sends. Phase 2 adds AC4 and AC5 as per-request options in the Settings tab, labeled "Automatically follow redirects" and "Enable SSL certificate verification".

| AC | Criterion |
|---|---|
| F05-AC1 | **Given** a valid request, **when** the user presses Send or Ctrl/Cmd+Enter, **then** the request goes out with its method and URL, and the response's status, headers, body and total time are captured. |
| F05-AC2 | **Given** a request in flight, **when** the user cancels, **then** the request is aborted, the tab shows `Request cancelled`, and no response is shown. |
| F05-AC3 | **Given** a timeout of N seconds (default 30, F12), **when** no complete response arrives in time, **then** the request fails with `Request timed out after N s`. |
| F05-AC4 | **Given** follow-redirects on (the default, max 10 hops), **then** redirects are followed and the final response is shown. **Given** it is off, **then** the 3xx response is shown as-is. |
| F05-AC5 | **Given** TLS verification on (the default) and an untrusted certificate, **then** the request fails with an error naming the certificate problem. **Given** it is off, **then** the request succeeds. |
| F05-AC6 | **When** a connection fails, **then** the error names the cause: DNS lookup failed for `<host>`, connection refused by `<host>:<port>`, or TLS handshake failed. |
| F05-AC7 | **While** a request is in flight, **then** the UI stays responsive and Send becomes Cancel. Sending never blocks the UI thread. |

## F06 Response viewer

Scope: status, timing breakdown, size, headers, body as pretty JSON / raw / hex for binary, search in body, virtualized rendering for large bodies.

Phase 1 delivered AC1 to AC5. AC9 to AC11 were built in the Phase 0 spike. Phase 2 adds AC2's breakdown, AC6 and AC7. AC8 (search) moves to Phase 3. The HTTP stack has no hook between the TCP and TLS handshakes, so the breakdown shows them together as "Connect" (ADR-0029).

| AC | Criterion |
|---|---|
| F06-AC1 | **Given** a response, **then** the status code and reason phrase are shown (for example `200 OK`). |
| F06-AC2 | **Given** a response, **then** the total time is shown. Hovering it shows the breakdown: DNS lookup, Connect (TCP and TLS), Waiting (time to first byte after connecting) and Download. A reused connection shows no DNS or Connect time. |
| F06-AC3 | **Given** a response, **then** its body size is shown in human units (B, KB, MB). |
| F06-AC4 | **Given** a response, **then** all response headers are listed, including duplicates. Names are shown lowercase, since HTTP/2 and the HTTP stack normalize them. Repeated headers keep their received order and are grouped under the name's first appearance (ADR-0023). |
| F06-AC5 | **Given** a text body, **then** it is shown as raw text. |
| F06-AC6 | **Given** a JSON body (by `Content-Type` or by content), **then** the Pretty view is the default and shows it re-indented with 2 spaces, keeping key order and number spelling. Raw shows the bytes as received. Pretty-printing runs off the UI thread and needs memory proportional to the output only. |
| F06-AC7 | **Given** a non-text (binary) body, **then** a hex view is the default: offset, 16 bytes in hex, and their printable ASCII. |
| F06-AC8 | **Given** a body, **when** the user searches, **then** matches are highlighted and the user can step through them. |
| F06-AC9 | **Given** a body, **then** it is shown as display lines: split on `\n`, with the `\r` of CRLF removed, and lines longer than the wrap width split without breaking a UTF-8 character. Invalid UTF-8 shows as U+FFFD. |
| F06-AC10 | **Given** a 50 MB body, **when** the user scrolls (wheel or jump), **then** frame time stays under 16 ms (p95), and only visible lines are built as widgets. Indexing runs off the UI thread. |
| F06-AC11 | **Given** a body larger than the viewer supports (4 GiB), **then** the viewer shows `body is <n> bytes; the viewer supports bodies up to 4294967295 bytes`. |

## F07 Tabs
Scope: open, close, reorder, dirty indicator, restore on restart.

| AC | Criterion |
|---|---|
| F07-AC1 | **Given** a fresh start, **then** one tab with a new request is open. **When** the user presses + or Ctrl/Cmd+T, **then** a new tab opens and becomes active. Each tab has its own request, response and send. A tab's title is its URL, or `Untitled Request` when the URL is empty. |
| F07-AC2 | **When** the user closes a tab (its x, middle-click, or Ctrl/Cmd+W), **then** it closes and the nearest tab becomes active. Closing the last tab opens a new one. Closing a tab with unsaved changes first asks `Discard unsaved changes?` with Discard and Cancel. Closing a tab cancels its send. |
| F07-AC3 | **When** the user drags a tab onto another, **then** it moves to that position. Ctrl+Tab and Ctrl+Shift+Tab activate the next and previous tab. |
| F07-AC4 | **Given** a tab whose request differs from its saved state (until F09, a new request), **then** it shows a dot. |
| F07-AC5 | **Given** open tabs, **then** their requests, order and active tab are saved to the OS app-data directory within 2 s of a change and on exit, with an atomic write. **On** the next start, they are restored, without responses. An unreadable session file is renamed to `session.toml.bak` and a fresh tab opens. |
| F07-AC6 | **Given** a send in flight in one tab, **when** the user switches tabs, **then** the response lands in the tab that sent it. |

## F08 Environments
Scope: `{{var}}` interpolation in URL, headers, body and auth. Global and environment scopes. Unresolved variables are highlighted and block send with a clear error. A gitignored `*.local.toml` layer overrides values for secrets (ADR-0011).

## F09 Collections
Scope: folders; save, rename, duplicate, delete; one TOML file per request; atomic writes; external edits picked up via file watching. Folder order: by name, with an optional `_order.toml` (ADR-0013).

## F10 History
Scope: last N requests (default 100), searchable, one-click replay. Stored in the OS app-data directory (ADR-0012).

## F11 Import/export
Scope: curl import and export; Postman Collection v2.1 import (documented subset).

## F12 Settings
Scope: theme, font size, default timeout.

## F13 Keyboard
Scope: Send (Ctrl/Cmd+Enter), new tab, close tab, command palette. Every action is reachable without a mouse.

## F14 Workbench layout
Scope: Postman's layout. Header with search and settings; left sidebar with Collections, Environments and History; workbench tabs; environment selector; request builder with Params, Authorization, Headers, Body, Scripts and Settings tabs; response pane with Body, Cookies, Headers and Test Results tabs; footer with Console, Cookies and Runner. Sidebars collapse. Ferry's own name, icon and colors.

| AC | Criterion |
|---|---|
| F14-AC1 | **Given** the main window, **then** it shows, top to bottom: a header (Ferry name, search field, settings); a body with the left sidebar (Collections, Environments, History) and the workbench (tab strip with + and an environment selector, request builder, response pane); and a footer (sidebar toggle, Console, Cookies, Runner, version). |
| F14-AC2 | **Given** the request builder, **then** it has the tabs Params, Authorization, Headers, Body, Scripts and Settings. Params and Headers show the count of enabled rows (Headers including auto headers); Authorization and Body show a dot when set. |
| F14-AC3 | **Given** the response pane, **then** it has the tabs Body, Cookies, Headers (with count) and Test Results, and on the right the status, time and size. |
| F14-AC4 | **When** the user toggles the sidebar (footer button or Ctrl/Cmd+B), **then** it collapses or expands. **When** the user drags the divider between request and response, **then** the split changes, between 20% and 80% of the height. Both are saved with the session (F07-AC5). |
| F14-AC5 | **Given** a feature from a later phase (collections, environments, history, scripts, cookies, console, runner, test results), **then** its place in the layout says what it will do and that it is not available yet. It never looks clickable without an effect. |

## F15 Scripts and tests
Scope: pre-request and post-response JavaScript per request, folder and collection, run in an embedded engine with the `pm` API (`pm.test`, `pm.expect`, `pm.environment`, `pm.variables`, `pm.request`, `pm.response`, `pm.sendRequest`, `console`). Test Results tab with pass, fail and skip filters. Scripts have no file system or process access, and they time out.

## F16 Collection Runner and CLI
Scope: run a collection or folder in order with iterations, delay and CSV/JSON data files; live results; export a report. `ferry-cli run` does the same headless for CI, with a non-zero exit code on test failures.

## F17 Cookies
Scope: a per-workspace cookie jar used on send, a Cookies tab in the response pane, and a cookie manager to add, edit and delete cookies by domain. The jar can be turned off per request.

## F18 Console
Scope: a log of every request as sent (after variables and scripts) and every response, plus script `console` output. Filter and clear.

## F19 Code snippets
Scope: generate code for the current request: cURL, HTTPie, Python requests, JavaScript fetch, Node axios, Go, Rust reqwest, Java, C#, PHP, Ruby, Swift. Pure functions in `ferry-core`.

## F20 Advanced auth
Scope: Digest, JWT Bearer, AWS Signature v4, Hawk, NTLM, Akamai EdgeGrid, ASAP, OAuth 1.0, OAuth 2.0 (authorization code with PKCE, implicit, password, client credentials; token storage and refresh; system-browser loopback redirect).

## F21 Network settings
Scope: HTTP version choice, proxy (system, custom, bypass list), client certificates per host, custom CA certificates, TLS version limits.

## F22 Response Preview and Visualize
Scope: Preview renders HTML, images, audio and video; Visualize renders HTML templates set by scripts (`pm.visualizer.set`). Both use an embedded system web view created on demand (ADR-0028). Also a JSONPath filter for JSON and XPath for XML and HTML, and body views for XML, HTML, YAML, JavaScript, Markdown and Base64.

## F23 GraphQL
Scope: query and variables editors, schema introspection, autocomplete and validation against the schema.

## F24 WebSocket and Socket.IO
Scope: connect, send and receive messages, message log with search, saved messages, Socket.IO events and acknowledgements.

## F25 Server-sent events
Scope: stream events into the response pane, search and clear.

## F26 gRPC
Scope: import .proto files or use server reflection, unary and streaming calls, metadata, TLS.

## F27 MQTT
Scope: connect, subscribe, publish, message log.

## F28 Mock server
Scope: serve a collection's saved examples on localhost, matched by method and path.

## F29 Documentation
Scope: generate HTML and Markdown documentation from a collection.

## F30 OpenAPI and HAR
Scope: import OpenAPI 3, Swagger 2 and HAR into collections. Later: an OpenAPI editor with linting.

## F31 Vault
Scope: secrets stored in the OS keychain, referenced as `{{vault:name}}`, never written to collection files or exports.

## F32 Saved examples
Scope: save a response as a named example of its request; view and edit examples.

## F33 Find and replace
Scope: search and replace across requests, collections and environments.

## F34 Flows (later)
Scope: a visual canvas that chains requests and logic blocks.

## F35 Performance tests (later)
Scope: run a collection with many virtual users and report latency percentiles and error rates.
