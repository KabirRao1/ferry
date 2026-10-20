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
| 3 | F08 variables; F09 collections and the workspace; F10 history; F17 cookies; F18 console; F32 saved examples; F22 Preview and JSONPath filter; F06-AC8 search in body; F04-AC6 inherited auth; F07-AC7 linked tabs |
| 4 | F15 scripts and tests; F16 Collection Runner and CLI; F22 Visualize (needs `pm.visualizer`) |
| 5 | F11 import/export; F19 code snippets; F20 advanced auth and OAuth; F21 proxy and certificates; F31 vault; F30 OpenAPI and HAR import |
| 6 | F23 GraphQL; F24 WebSocket and Socket.IO; F25 SSE; F26 gRPC; F27 MQTT |
| 7 | F28 mock server; F29 docs generation; F33 find and replace; F12; F13; F22 XPath and extra body views; performance hardening; releases |
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
| F01-AC2 | **Given** a URL with a query string, **then** the params table lists every pair in order, as written in the URL (not percent-decoded, as in Postman), with duplicate keys kept. |
| F01-AC3 | **When** a param row is added, edited, removed or disabled, **then** the URL's query updates to match. Disabled rows are left out of the URL. |
| F01-AC4 | **Given** any sequence of URL and table edits, **then** the URL and table stay consistent. Untouched params keep their original order and encoding (property test). |
| F01-AC5 | **Given** a URL without an `http://` or `https://` scheme or without a host, **when** the user sends, **then** nothing is sent and the request shows `Enter a URL starting with http:// or https://`. |
| F01-AC6 | **Given** `{{var}}` placeholders in the URL, **then** URL/table sync preserves them verbatim (F08 resolves them at send time). |
| F01-AC7 | **When** the user types a method name into the method field, **then** a standard name selects that method and any other RFC 9110 token becomes a custom method. An invalid name shows the `MethodError` text and blocks sending. |
| F01-AC8 | **Given** the params table, **then** each row has a key, a value, a description (never sent) and an enabled checkbox. A trailing blank row is always present; typing into it adds a row. |

Phase 2 sync rules (ADR-0030): when the URL is edited, the enabled rows are replaced by the URL's pairs while disabled rows keep their positions. When the table is edited, the query is rebuilt from enabled rows with a key: an untouched row keeps its original text, an edited row is written as typed with only `&`, `#` (and `=` in keys) escaped, and a row with an empty value is written as just its key. `{{...}}` stays verbatim. The fragment is preserved. An empty query drops the `?`. Percent-encoding of everything else happens when the request is built.

## F02 Headers
Scope: editable rows, enable/disable per row, duplicate keys allowed, auto-generated headers, bulk edit.

| AC | Criterion |
|---|---|
| F02-AC1 | **Given** header rows (key, value, description, enabled), **when** the request is sent, **then** every enabled row with a non-blank key is sent in table order, duplicates included. Disabled and blank-key rows are not sent. |
| F02-AC2 | **Given** an enabled header whose name is not an RFC 9110 token, **then** sending is blocked with `Header name "<name>" contains invalid character '<c>'`. **Given** a value containing CR or LF, **then** sending is blocked with `Header "<name>" value cannot contain line breaks`; any other control character except tab blocks it with `Header "<name>" value contains invalid character <escaped char>`. |
| F02-AC3 | **Given** a request, **then** the Headers tab can show the auto-generated headers: `User-Agent: ferry/<version>`, `Accept: */*`, `Accept-Encoding: gzip, deflate, br, zstd`, `Content-Type` from the body (F03), `Authorization` or the API key header from auth (F04), and `Host` and `Content-Length` as "calculated when sent". They are hidden by default behind a "<n> hidden" toggle. An enabled user header with the same name (case-insensitive) replaces an auto header, and the auto row shows as overridden. |
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
Scope: No Auth, Basic, Bearer, API key (header or query), and Inherit from parent (Phase 3, with F09). Other types are F20.

| AC | Criterion |
|---|---|
| F04-AC1 | **Given** the types No Auth, Basic Auth, Bearer Token and API Key, **when** the user switches type, **then** each type keeps its fields, and only the selected type affects the request. |
| F04-AC2 | **Given** Basic Auth, **then** `Authorization: Basic <base64(username:password)>` is sent, with UTF-8 bytes. |
| F04-AC3 | **Given** Bearer Token with a non-empty token, **then** `Authorization: Bearer <token>` is sent. An empty token sends nothing. |
| F04-AC4 | **Given** API Key with a non-empty key, **then** `<key>: <value>` is sent as a header, or `key=value` is appended to the query (percent-encoded), as chosen. An empty key sends nothing. |
| F04-AC5 | **Given** an enabled user header with the same name as the auth header, **then** the user header wins (F02-AC3). |
| F04-AC6 | **Given** the type `Inherit auth from parent`, **then** the request uses the auth of its nearest parent folder or collection whose type is not Inherit (a collection's Inherit means No Auth). An unsaved request has no parent, so Inherit sends nothing. The Authorization tab names the source: `This request is using <type> from <parent kind> "<parent name>".` Requests created inside a collection start as Inherit; other new requests start as No Auth. |

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

Phase 1 delivered AC1 to AC5. AC9 to AC11 were built in the Phase 0 spike. Phase 2 adds AC2's breakdown, AC6 and AC7. Phase 3 adds AC8 (search) and AC12. The HTTP stack has no hook between the TCP and TLS handshakes, so the breakdown shows them together as "Connect" (ADR-0029).

| AC | Criterion |
|---|---|
| F06-AC1 | **Given** a response, **then** the status code and reason phrase are shown (for example `200 OK`). |
| F06-AC2 | **Given** a response, **then** the total time is shown. Hovering it shows the breakdown: DNS lookup, Connect (TCP and TLS), Waiting (time to first byte after connecting) and Download. A reused connection shows no DNS or Connect time. |
| F06-AC3 | **Given** a response, **then** its body size is shown in human units (B, KB, MB). |
| F06-AC4 | **Given** a response, **then** all response headers are listed, including duplicates. Names are shown lowercase, since HTTP/2 and the HTTP stack normalize them. Repeated headers keep their received order and are grouped under the name's first appearance (ADR-0023). |
| F06-AC5 | **Given** a text body, **then** it is shown as raw text. |
| F06-AC6 | **Given** a JSON body (by `Content-Type` or by content), **then** the Pretty view is the default and shows it re-indented with 2 spaces, keeping key order and number spelling. Raw shows the bytes as received. Pretty-printing runs off the UI thread and needs memory proportional to the output only. |
| F06-AC7 | **Given** a non-text (binary) body, **then** a hex view is the default: offset, 16 bytes in hex, and their printable ASCII. |
| F06-AC8 | **Given** a Pretty or Raw body, **when** the user opens the find bar (Ctrl/Cmd+F in the response pane, or its search button) and types, **then** the shown text is searched without regard to ASCII case, matches on visible lines are highlighted, the current one more strongly, and the bar shows `<i> of <n>` or `No results`. Enter or Down moves to the next match and Shift+Enter or Up to the previous one, wrapping around, and scrolls it into view. Counting stops at 10,000 matches (`10000+`). The search runs off the UI thread. Escape closes the bar and clears the highlights. |
| F06-AC9 | **Given** a body, **then** it is shown as display lines: split on `\n`, with the `\r` of CRLF removed, and lines longer than the wrap width split without breaking a UTF-8 character. Invalid UTF-8 shows as U+FFFD. |
| F06-AC10 | **Given** a 50 MB body, **when** the user scrolls (wheel or jump), **then** frame time stays under 16 ms (p95), and only visible lines are built as widgets. Indexing runs off the UI thread. |
| F06-AC11 | **Given** a body larger than the viewer supports (4 GiB), **then** the viewer shows `body is <n> bytes; the viewer supports bodies up to 4294967295 bytes`. |
| F06-AC12 | **Given** a response, **when** another response, a history entry or an example is shown in the tab, **then** the find bar, filter and preview state of the previous body are discarded. |

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
| F07-AC7 | **Given** a tab linked to a saved request (F09), **then** its title is the request's name, its dot compares the draft with the file's contents, and the session saves the link with any unsaved edits. **On** restart, a linked tab whose file still exists restores its edits and its link; one whose file is gone restores as an unsaved request. Environment, collection and example tabs are not restored. |

## F08 Variables and environments
Scope: `{{var}}` interpolation in URL, params, headers, bodies and auth. Global, collection and environment scopes, with secrets kept out of committed files (ADR-0011, ADR-0040). Dynamic variables. Data and local scopes arrive with scripts and the runner (Phase 4).

| AC | Criterion |
|---|---|
| F08-AC1 | **Given** text containing `{{name}}`, where `name` is one or more characters other than `{`, `}`, CR and LF, **then** it is a variable reference. Anything else, such as `{{}}`, `{x}` or an unclosed `{{a`, is literal text. Substituted values are not scanned again for references, except as AC4 describes. |
| F08-AC2 | **Given** variables in several scopes, **then** a name resolves to the first enabled definition in this order: the active environment, the request's collection, then globals. Disabled variables are ignored. |
| F08-AC3 | **When** a request is sent, **then** references are replaced in the URL (so also its params), header names and values, the raw body, urlencoded and form-data names and text values, and all auth fields, before any validation (so `{{base}}/users` with `base = https://api.test` is a valid URL). The draft keeps its placeholders. |
| F08-AC4 | **Given** a value that itself contains references, **then** they are resolved in turn, up to 10 levels. A reference that leads back to itself blocks sending with `Variable {{<name>}} refers to itself`. Deeper nesting blocks sending with `Variables are nested more than 10 levels deep at {{<name>}}`. Values that would make a text more than 10 MB longer block sending with `Variables expand to more than 10 MB at {{<name>}}`, and resolving stops there (FND-017). |
| F08-AC5 | **Given** a reference that no scope defines and that is not a dynamic variable, **when** the user sends, **then** nothing is sent and the request shows `Unresolved variable {{<name>}}`, or `Unresolved variables {{<a>}}, {{<b>}}` for several, each name once in order of first appearance. |
| F08-AC6 | **Given** a request, **then** under the URL bar a strip lists each variable it references, once, in order: resolved ones with their value and scope (secret values shown as `••••••`), unresolved ones highlighted as errors with `not defined`. The strip is hidden when the request references none. |
| F08-AC7 | **Given** `{{$guid}}` or `{{$randomUUID}}` (a random version 4 UUID), `{{$timestamp}}` (Unix seconds), `{{$isoTimestamp}}` (UTC, `2026-10-17T19:07:20.123Z`) or `{{$randomInt}}` (0 to 1000), **then** each occurrence gets its own value when the request is sent. A defined variable with the same name takes precedence. The clock and randomness are injected. |
| F08-AC8 | **Given** the Environments sidebar, **then** it lists `Globals` and every environment by name. **When** the user creates (`New Environment`), renames, duplicates (`<name> Copy`) or deletes one (after `Delete environment "<name>"?`), **then** the workspace files change to match. Clicking an environment or Globals opens it in a workbench tab with a table of variables (enabled, variable, value, secret), a dot while it has unsaved changes, and Save (Ctrl/Cmd+S). |
| F08-AC9 | **Given** the tab strip's environment selector, **then** it lists `No environment` and every environment. The choice applies to every tab, is saved with the session by file name, and falls back to `No environment` if that file is gone. |
| F08-AC10 | **Given** an environment saved by Ferry, **then** it is `environments/<file>.toml` with `name` and `[[variables]]` (`key`, `value`, `enabled`, `secret`), and globals are `globals.toml` with `[[variables]]`. A secret variable's value is written only to the sibling `<file>.local.toml`, never to the main file. **When** loading, values in `<file>.local.toml` replace the main file's value for the same key, and keys only in the local file are added. The workspace `.gitignore` lists `*.local.toml` (F09-AC1). |
| F08-AC11 | **Given** an environment file that cannot be read or parsed, **then** it is listed with `Could not read <path>: <reason>`, cannot be selected or edited, and is never overwritten. Other environments are unaffected. |

## F09 Collections and the workspace
Scope: a workspace folder of plain TOML files; collections with folders; save, rename, duplicate, delete and move; one TOML file per request; atomic writes; external edits picked up via file watching; collection and folder settings (description, auth, collection variables). Order by name with an optional `_order.toml` (ADR-0013, ADR-0037).

| AC | Criterion |
|---|---|
| F09-AC1 | **Given** startup, **then** the workspace folder is `FERRY_WORKSPACE` if set, else the folder last chosen with `Open Workspace…` (saved in the session), else `<data dir>/workspace`. It holds `collections/`, `environments/` and `globals.toml`. **When** Ferry creates a workspace or writes into one, **then** its `.gitignore` contains a `*.local.toml` line (added if missing, other lines kept). |
| F09-AC2 | **Given** a collection, **then** it is a folder `collections/<name>/` with `collection.toml` (`name`, `description`, `auth`, `[[variables]]`). A folder inside it is a directory with an optional `folder.toml` (`name`, `description`, `auth`). A request is `<name>.toml` holding `name`, the request fields of F01 to F05, and its examples (F32). Fields at their default values are omitted. Loading what Ferry saved gives back the same collection (property test), and the format is fixed by snapshot tests. A missing `name` falls back to the file or folder name. |
| F09-AC3 | **Given** the Collections sidebar, **then** collections are listed by name, each expandable to its folders and requests. Inside a folder, items sort by name, case-insensitively, unless the folder has `_order.toml` (`order = ["<file or folder name>", …]`): listed items come first in that order, the rest follow by name. Requests show their method and name. |
| F09-AC4 | **When** the user clicks a request in the sidebar, **then** the tab already linked to it becomes active, or else a new tab opens linked to it. |
| F09-AC5 | **When** the user saves (Save button or Ctrl/Cmd+S) a linked request tab, **then** its file is rewritten atomically and the dot clears. **When** the tab is not linked, **then** a Save dialog asks for a name (default: the tab title) and a collection or folder, offers `New Collection` when there are none, and on Save writes the file and links the tab. An empty name shows `Enter a name`. |
| F09-AC6 | **When** the user creates a collection (`New Collection` in the sidebar), a folder or a request (a collection's or folder's `…` menu), **then** the directory or file is created with the name `New Collection`, `New Folder` or `New Request` (made unique as in AC8), and a new request opens in a tab linked to its file. |
| F09-AC7 | **When** the user renames an item, **then** its file or directory and its `name` change together, and linked tabs follow it. **When** the user duplicates an item, **then** a copy named `<name> Copy` is created next to it (folders recursively). **When** the user deletes an item and confirms `Delete "<name>"?`, **then** its file or directory is removed, and tabs linked to it stay open as unsaved requests. |
| F09-AC8 | **Given** a name, **then** its file or directory name replaces `<>:"/\|?*` and control characters with `_`, trims trailing dots and spaces, adds `_` after Windows reserved names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`, with or without an extension), uses `Untitled` when nothing is left, and keeps at most 100 characters. A name that collides case-insensitively with a sibling gets ` (2)`, ` (3)`, and so on. |
| F09-AC9 | **When** the user drags an item onto a request, **then** it moves just before that request, in that request's folder; dropped onto a folder or collection, it moves to the end of it. The affected folders' `_order.toml` is rewritten. A folder cannot move into itself. |
| F09-AC10 | **Given** a file in the workspace changed by another program, **then** within 1 s the sidebar shows the change. A linked tab without unsaved changes reloads the new contents; one with unsaved changes keeps them and its dot. A linked tab whose file was deleted becomes an unsaved request. Ferry's own writes do not disturb tabs. |
| F09-AC11 | **When** the user opens a collection or folder (its name in the sidebar, or `…` > Settings), **then** a tab shows its Overview (name and description), Authorization (F04 types, including Inherit for folders) and, for collections, Variables, with a dot while unsaved and Save. Collection variables take part in F08-AC2 for requests inside the collection. |
| F09-AC12 | **Given** a request, folder or collection file that cannot be read or parsed, **then** the sidebar shows it with `Could not read <path>: <reason>`, opening it does nothing, and Ferry never overwrites or deletes it except on an explicit Delete. |
| F09-AC13 | **Given** the header's search field, **when** the user types, **then** the sidebar shows the saved requests whose name or URL contains the text, ignoring case, with their collection and folder; clicking one opens it as in AC4. Clearing the field restores the sidebar. |
| F09-AC14 | **When** the user chooses `Open Workspace…` and picks a folder, **then** Ferry switches to that workspace: the sidebar reloads, linked tabs become unsaved requests, and the folder is remembered (AC1). |
| F09-AC15 | **Given** a workspace operation that fails (permission denied, disk full, a name taken by another program), **then** the files are left as they were where possible, and the footer shows `Could not <action>: <reason>`. |

## F10 History
Scope: last N requests (default 100), searchable, one-click replay. Stored in the OS app-data directory (ADR-0012, ADR-0039).

| AC | Criterion |
|---|---|
| F10-AC1 | **When** a send gets a response or fails, **then** a history entry records the time, the method, the URL as sent (variables resolved), the request as edited (placeholders kept), and the status, time and size, or the error text. Cancelled sends are not recorded. |
| F10-AC2 | **Given** more than 100 entries, **then** the oldest are dropped. Response bodies are kept up to 1 MB; longer ones are truncated and marked. |
| F10-AC3 | **Given** the History sidebar, **then** entries are listed newest first, grouped under `Today`, `Yesterday` or the date (`October 15`, or `October 15, 2025` for other years) in local time, each showing the method and URL. A search field filters them by URL or method, ignoring case. |
| F10-AC4 | **When** the user clicks an entry, **then** a new unsaved tab opens with its request and its recorded response (status, headers, time, size and body). A truncated body shows `Body truncated to 1 MB in history`. |
| F10-AC5 | **When** the user deletes an entry, **then** it disappears. **When** the user clears all history and confirms `Clear all history?`, **then** every entry is removed. |
| F10-AC6 | **Given** history, **then** it is stored in the data directory as `history/history.toml` (entries) and `history/bodies/<id>` (body bytes), written atomically, and survives restarts. Bodies of dropped entries are deleted. An unreadable `history.toml` is renamed to `history.toml.bak` and history starts empty. |

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
| F14-AC5 | **Given** a feature from a later phase (scripts, runner, test results and, before Phase 3, collections, environments, history, cookies and console), **then** its place in the layout says what it will do and that it is not available yet. It never looks clickable without an effect. |
| F14-AC6 | **Given** a request tab, **then** above the URL bar it shows the request's location (`<collection> / <folder> / <name>` for a linked request, `Untitled Request` or the URL otherwise) and a Save button. Workbench tabs for environments, collections, folders and examples show their kind in the tab strip. |

## F15 Scripts and tests
Scope: pre-request and post-response JavaScript per request, folder and collection, run in an embedded engine with the `pm` API (`pm.test`, `pm.expect`, `pm.environment`, `pm.variables`, `pm.request`, `pm.response`, `pm.sendRequest`, `console`). Test Results tab with pass, fail and skip filters. Scripts have no file system or process access, and they time out.

Engine: QuickJS through `rquickjs` (ADR-0044). Script writes to environments, globals and collection variables go to local files only (ADR-0045).

| AC | Criterion |
|---|---|
| F15-AC1 | **Given** a request, **then** its Scripts tab has `Pre-request` and `Post-response` editors. A collection's or folder's settings tab (F09-AC11) has a Scripts section with the same two editors. Scripts are saved in the item's file as `[scripts]` with `pre_request` and `post_response` strings, left out when empty, and a request with scripts round-trips through its file unchanged. |
| F15-AC2 | **When** a request is sent, **then** its pre-request scripts run in order collection, folders from the outermost in, then the request; then variables are resolved and the request is built and sent (F08-AC3); then the post-response scripts run in the same order. An unsaved request runs only its own scripts. Empty scripts are skipped. |
| F15-AC3 | **Given** a script that throws, **then** the remaining scripts of that phase do not run. In the pre-request phase the request is not sent and the tab shows `Pre-request script failed in <location>: <message>`, where location is `the collection`, `folder <name>` or `the request`. In the post-response phase the response is still shown and Test Results shows `Post-response script failed in <location>: <message>` above the tests. A syntax error is reported the same way, with its line. |
| F15-AC4 | **Given** a script, **then** it runs in a fresh context with no file system, process, network or module access: `require` throws `require is not available in Ferry scripts`. A script that runs longer than 5 s (time spent waiting for `pm.sendRequest` excluded) stops with `Script timed out after 5 s`, and one that allocates more than 64 MB stops with `Script ran out of memory (64 MB)`. Scripts run off the UI thread, and Cancel stops them. |
| F15-AC5 | **Given** `pm.environment`, `pm.globals` and `pm.collectionVariables`, **then** each has `get(key)`, `has(key)`, `set(key, value)`, `unset(key)` and `toObject()`. `set` stores strings as they are and other values as `JSON.stringify` gives them. Changes are visible to later scripts and to the request being prepared at once, and are saved to the scope's local file (`<environment>.local.toml`, `globals.local.toml`, `collection.local.toml`) as F08-AC10 describes, never to the shared file. `unset` removes the local value, so a value in the shared file shows again. `pm.environment.set` with no environment selected throws `No environment is selected`, and `pm.collectionVariables.set` on an unsaved request throws `This request is not in a collection`. |
| F15-AC6 | **Given** `pm.variables`, **then** `get(key)` resolves as F08-AC2 does, with values set through `pm.variables.set` first and then the runner's data row (F16-AC3). `set` values last until the request finishes, or for a whole run in the runner, and are never saved. `has`, `unset`, `toObject` and `replaceIn(text)` (which replaces `{{name}}` references as F08 does) are also available. |
| F15-AC7 | **Given** a pre-request script, **then** `pm.request` has `method`, `url` (with `toString()`, and settable as text), `headers` (`get`, `has`, `add({key, value})`, `upsert({key, value})`, `remove(key)`, `toObject()`) and `body` (`mode` and `raw`). Changes apply to the request that is sent, not to the saved draft. In a post-response script `pm.request` describes the request as sent, and changes have no effect. |
| F15-AC8 | **Given** a post-response script, **then** `pm.response` has `code`, `status` (the reason phrase), `headers` (`get`, `has`, `toObject`), `responseTime` (ms), `responseSize` (bytes), `text()` and `json()`, which throws `Response body is not valid JSON: <reason>`. `pm.response.to.have.status(code or reason)`, `to.have.header(name[, value])`, `to.have.body(text)`, `to.have.jsonBody()`, `to.be.ok` (200), `to.be.success` (2xx), `to.be.redirection` (3xx), `to.be.clientError` (4xx), `to.be.serverError` (5xx), `to.be.error` (4xx or 5xx), `to.be.notFound` (404) and `.not` versions of each are assertions. In a pre-request script `pm.response` is undefined. |
| F15-AC9 | **Given** `pm.test(name, fn)`, **then** a test named `name` passes if `fn` returns without throwing and fails with the thrown message otherwise; `pm.test.skip(name, fn)` records a skipped test without running `fn`. `pm.expect(value)` supports Chai's BDD chains `to`, `be`, `been`, `is`, `that`, `which`, `and`, `has`, `have`, `with`, `at`, `of`, `same`, `not`, `deep` and the assertions `equal`/`eq`, `eql`, `above`/`gt`, `below`/`lt`, `least`/`gte`, `most`/`lte`, `within`, `a`/`an`, `include`/`contain`, `ok`, `true`, `false`, `null`, `undefined`, `exist`, `empty`, `lengthOf`/`length`, `property`, `match`, `oneOf` and `keys`, with Chai's messages (for example `expected 404 to equal 200`). |
| F15-AC10 | **Given** a response, **then** Test Results is labelled `Test Results (<passed>/<total>)` and lists each test with `PASS`, `FAIL` or `SKIP`, the test name and, for failures, the message. Filters `All`, `Passed`, `Skipped` and `Failed` narrow the list. Without tests it shows `No tests for this request`. |
| F15-AC11 | **Given** `console.log`, `info`, `warn` and `error` in a script, **then** each call adds a line to the Console (F18) under the request, with its level. Arguments are joined by spaces; strings appear as they are and other values as JSON. |
| F15-AC12 | **Given** `pm.sendRequest(urlOrRequest, callback)`, **then** Ferry sends the request (a URL string, or `{url, method, header, body: {mode: "raw", raw}}`) with the same client, cookie jar and 30 s timeout as a normal send, and calls `callback(error, response)`, where `response` has the `pm.response` fields and `error` is null or has `message`. A script may send at most 10 requests. |
| F15-AC13 | **Given** a script, **then** `pm.info` has `requestName`, `eventName` (`prerequest` or `test`), `iteration` (from 0) and `iterationCount`, and `pm.iterationData` has `get`, `has` and `toObject` over the runner's data row (empty outside the runner). `JSON`, `atob`, `btoa` and `encodeURIComponent` are available. |

## F16 Collection Runner and CLI
Scope: run a collection or folder in order with iterations, delay and CSV/JSON data files; live results; export a report. `ferry-cli run` does the same headless for CI, with a non-zero exit code on test failures.

| AC | Criterion |
|---|---|
| F16-AC1 | **When** the user chooses `Run` on a collection or folder (`…` menu or its settings tab), **then** a Runner tab opens listing the requests inside it in sidebar order, each with a checkbox (all checked), and fields `Iterations` (1 to 1000, default 1), `Delay` (0 to 60000 ms between requests, default 0), `Data file` (optional) and `Stop on first failure`. Invalid numbers show `Enter a number from 1 to 1000` or `Enter a number from 0 to 60000` and disable Run. |
| F16-AC2 | **When** Run is pressed, **then** for each iteration the checked requests run in order as F15-AC2 describes, waiting `Delay` between requests, using the selected environment and the cookie jar. Variable changes made by scripts are seen by later requests; `pm.variables` values last for the whole run. Requests that fail to send, and failed tests, count as failures; with `Stop on first failure` the run ends at the first one. |
| F16-AC3 | **Given** a data file, **then** it is CSV with a header row (RFC 4180 quoting) or a JSON array of objects, and each row is one iteration: the number of rows replaces `Iterations`. Each row's values are `pm.iterationData` and also resolve as variables, after `pm.variables` values and before the environment. A file that is neither shows `Data file must be CSV with a header row or a JSON array of objects`. |
| F16-AC4 | **Given** a run in progress, **then** each finished request appears at once with its iteration, method, name, status, time, and passed and failed test counts, and the summary shows iterations done, requests, tests passed and failed, and total time. Selecting a result shows its tests and any error. `Stop` cancels the request in flight and ends the run with `Run stopped`. The run continues while other tabs are used. |
| F16-AC5 | **Given** `pm.execution.setNextRequest(name)` in a script during a run, **then** the request with that name in the run runs next instead of the following one; `setNextRequest(null)` ends the iteration. A name not in the run shows `No request named <name> in this run` and ends the iteration. An iteration that runs more than 10,000 requests stops with `Stopped after 10000 requests in one iteration`. Outside the runner the call does nothing. |
| F16-AC6 | **When** the user presses `Export Results` after a run, **then** a file dialog saves a JSON report: collection or folder name, environment, start time (RFC 3339), duration, iterations, totals, and per request its iteration, name, method, URL, status, time, error and tests (`name`, `status` of `passed`, `failed` or `skipped`, and `error`). |
| F16-AC7 | **Given** `ferry-cli run <path> [--workspace <dir>] [--environment <name>] [--iterations <n>] [--data <file>] [--delay <ms>] [--bail] [--report <file>]`, where `path` is a collection or folder path relative to the workspace (the current directory by default), **then** it runs as F16-AC2 to AC5 describe and prints one line per request (`✓`/`✗`, method, name, status, time) followed by its failed tests, then a summary. It exits 0 when every request and test passed, 1 when any failed, and 2 for invalid arguments or an unreadable workspace, printing the reason. `--report` writes the F16-AC6 report. Script variable changes are not saved to any file. |

## F17 Cookies
Scope: a cookie jar used on send, a Cookies tab in the response pane, and a cookie manager to add, edit and delete cookies by domain. The jar can be turned off per request (ADR-0038).

| AC | Criterion |
|---|---|
| F17-AC1 | **Given** a response, including each redirect hop, **then** its `Set-Cookie` headers are stored as RFC 6265 describes: host-only without a `Domain` attribute; the default path from the request path without `Path`; `Max-Age` over `Expires`; an expiry in the past deletes the cookie; `Secure` and `HttpOnly` kept. A `Domain` that does not domain-match the request host, or that is a single label such as `com` (unless it equals the host), is ignored, as is a header without `name=`. |
| F17-AC2 | **When** a request is sent with the jar on, **then** every unexpired cookie whose domain and path match the URL is sent in one `Cookie` header, `Secure` ones only over https, longer paths first and then older ones first. A user `Cookie` header is sent with the jar's cookies for the request's own URL appended to it. |
| F17-AC3 | **Given** a request's Settings tab, **then** `Send and store cookies` (on by default) controls the jar: when off, the jar is neither read nor written for that request. |
| F17-AC4 | **Given** a response, **then** the Cookies tab, labelled `Cookies (<n>)`, lists the cookies the final response set: name, value, domain, path, expires (`Session` if none), HttpOnly and Secure. |
| F17-AC5 | **When** the user opens the cookie manager (footer Cookies, or `Cookies` under Send), **then** a dialog lists the jar's domains and each domain's cookies. The user can add a domain, add or edit a cookie as `Set-Cookie` text (`name=value; Path=/; Expires=…`), delete a cookie, and delete all of a domain's cookies. Text without `name=` shows `Enter a cookie as name=value; attributes`. |
| F17-AC6 | **Given** the jar, **then** it is saved to `cookies.toml` in the data directory, atomically, within 2 s of a change and on exit, and restored on start without the cookies that have expired. An unreadable file is renamed to `cookies.toml.bak` and the jar starts empty. |

## F18 Console
Scope: a log of every request as sent (after variables and scripts) and every response, plus script `console` output (F15-AC11). Filter and clear.

| AC | Criterion |
|---|---|
| F18-AC1 | **When** a request is sent, **then** the console logs it as sent: method, URL (variables resolved), headers including the auto-generated ones and the jar's `Cookie`, and the body (up to 10 KB of text, otherwise a description such as `multipart/form-data, 2 fields` or `<n> bytes`). **When** it ends, **then** the entry adds the status, time, size and response headers and body (up to 10 KB of text), or the error text, or `Request cancelled`. |
| F18-AC2 | **When** the user presses Console in the footer, **then** a console panel opens or closes under the workbench. Entries show one line each (`GET https://… 200 · 85 ms`); clicking one expands its details. |
| F18-AC3 | **Given** the console, **then** a text filter keeps the entries whose method or URL contain the text (ignoring case), an `Errors only` switch keeps failed sends and responses with status 400 or above, and Clear empties it. |
| F18-AC4 | **Given** more than 500 entries, **then** the oldest are dropped. The console is not saved across restarts. |

## F19 Code snippets
Scope: generate code for the current request: cURL, HTTPie, Python requests, JavaScript fetch, Node axios, Go, Rust reqwest, Java, C#, PHP, Ruby, Swift. Pure functions in `ferry-core`.

## F20 Advanced auth
Scope: Digest, JWT Bearer, AWS Signature v4, Hawk, NTLM, Akamai EdgeGrid, ASAP, OAuth 1.0, OAuth 2.0 (authorization code with PKCE, implicit, password, client credentials; token storage and refresh; system-browser loopback redirect).

## F21 Network settings
Scope: HTTP version choice, proxy (system, custom, bypass list), client certificates per host, custom CA certificates, TLS version limits.

## F22 Response Preview and Visualize
Scope: Preview renders HTML, images, audio and video; Visualize renders HTML templates set by scripts (`pm.visualizer.set`). Both use an embedded system web view created on demand (ADR-0028). Also a JSONPath filter for JSON and XPath for XML and HTML, and body views for XML, HTML, YAML, JavaScript, Markdown and Base64.

Phase 3 delivers Preview and the JSONPath filter. Visualize needs scripts and moves to Phase 4; XPath and the extra body views move to Phase 7 (ADR-0041).

| AC | Criterion |
|---|---|
| F22-AC1 | **Given** a response whose `Content-Type` is HTML, an image, audio, video or PDF, **then** the body formats include `Preview`. On Windows and macOS, Preview shows the body rendered by an embedded system web view that fills the body area. The web view is created when Preview is shown and destroyed when the user leaves it (another format, response tab, workbench tab or response). |
| F22-AC2 | **Given** a preview, **then** the body is served from memory with its `Content-Type`, the page cannot open windows or navigate away, and it has no bridge to Ferry. |
| F22-AC3 | **Given** Linux, or a web view that fails to start, **then** Preview shows `Preview is not available here` with `Open in Browser`, which writes the body to a temporary file with a matching extension and opens it with the system's default application. `Open in Browser` is also offered on every platform. |
| F22-AC4 | **Given** a JSON body up to 10 MB, **then** the Pretty view has a filter field. A JSONPath expression (RFC 9535) shows its results as a pretty-printed JSON array; an invalid one shows `Invalid JSONPath: <reason>`; clearing it shows the body again. Filtering runs off the UI thread. Larger bodies show `Filtering is available for bodies up to 10 MB`. |
| F22-AC5 | **Given** a post-response script that calls `pm.visualizer.set(template, data)`, **then** the body formats include `Visualize`, which renders the Handlebars template with `data` and shows the HTML in the web view (Open in Browser on Linux, F22-AC3). The page may run its own inline scripts, but a Content Security Policy keeps it from loading or sending anything outside the page; `pm.getData(callback)` in the page passes it `data`. A template that does not render shows `Visualizer template error: <reason>`. Without a `pm.visualizer.set` call there is no `Visualize` format. |

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

| AC | Criterion |
|---|---|
| F32-AC1 | **Given** a response in a tab linked to a saved request, **when** the user chooses `Save as Example`, **then** an example holding the request as edited and the response's status, headers and body is added to the request's file, named after the request (with ` (2)`, ` (3)` for repeats). In an unlinked tab the action is disabled with `Save the request to a collection first`. |
| F32-AC2 | **Given** an example, **then** a UTF-8 body is stored as text and any other body as base64 (`body_encoding = "base64"`), and loading gives back the same bytes. |
| F32-AC3 | **Given** a request with examples, **then** the sidebar lists them under it. Clicking one opens an example tab showing its request and response, where the name, status code, response headers and body text are editable, with a dot while unsaved and Save. |
| F32-AC4 | **When** the user renames, duplicates or deletes an example from the sidebar, **then** the request's file changes to match. |

## F33 Find and replace
Scope: search and replace across requests, collections and environments.

## F34 Flows (later)
Scope: a visual canvas that chains requests and logic blocks.

## F35 Performance tests (later)
Scope: run a collection with many virtual users and report latency percentiles and error rates.
