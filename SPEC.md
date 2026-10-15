# Ferry specification

Ferry is a native, low-memory, offline desktop HTTP API client. This file is the
source of truth for behavior. Every acceptance criterion (AC) has an ID, and every
test that covers it includes the ID in its name (`f06_ac9_...`).

Acceptance criteria are written in full before a feature's phase starts. Features
not yet started list their scope only.

Conventions:
- **Given / When / Then** describe observable behavior through public APIs
  (`ferry-core` functions, `ferry-app` messages and state, `ferry-ui` views).
- Error messages quoted in an AC are exact and covered by snapshot tests.

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

Phase 1 covers AC1 and AC5. AC2 to AC4 and AC6 move to Phase 2 with F02 (ADR-0015).

| AC | Criterion |
|---|---|
| F01-AC1 | **Given** a new request, **then** the method is `GET`. **When** the user picks a method, **then** it is one of GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS, or a custom token matching RFC 9110 `token`. |
| F01-AC2 | **Given** a URL with a query string, **then** the params table lists every pair in order, percent-decoded, with duplicate keys kept. |
| F01-AC3 | **When** a param row is added, edited, removed or disabled, **then** the URL's query updates to match. Disabled rows are left out of the URL. |
| F01-AC4 | **Given** any sequence of URL and table edits, **then** the URL and table stay consistent. Untouched params keep their original order and encoding (property test). |
| F01-AC5 | **Given** a URL without an `http://` or `https://` scheme or without a host, **when** the user sends, **then** nothing is sent and the request shows `Enter a URL starting with http:// or https://`. |
| F01-AC6 | **Given** `{{var}}` placeholders in the URL, **then** URL/table sync preserves them verbatim (F08 resolves them at send time). |

## F02 Headers
Scope: editable rows, enable/disable per row, duplicate keys allowed.

## F03 Body
Scope: none, raw text, JSON with live validation, x-www-form-urlencoded, multipart (text and file parts).

## F04 Auth
Scope: none, Basic, Bearer, API key (header or query).

## F05 Send

Scope: send, cancel in flight, timeout, follow-redirects toggle, TLS-verify toggle.

Phase 1 covers AC1, AC6 and AC7.

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

Phase 1 covers AC1 to AC5. AC9 to AC11 were built in the Phase 0 spike.

| AC | Criterion |
|---|---|
| F06-AC1 | **Given** a response, **then** the status code and reason phrase are shown (for example `200 OK`). |
| F06-AC2 | **Given** a response, **then** the total time is shown. From Phase 2, it is broken down into DNS, connect, TLS, time to first byte and download. |
| F06-AC3 | **Given** a response, **then** its body size is shown in human units (B, KB, MB). |
| F06-AC4 | **Given** a response, **then** all response headers are listed in received order, including duplicates. |
| F06-AC5 | **Given** a text body, **then** it is shown as raw text. |
| F06-AC6 | **Given** a JSON body, **then** a pretty-printed view is available and is the default. |
| F06-AC7 | **Given** a non-text (binary) body, **then** a hex view is shown. |
| F06-AC8 | **Given** a body, **when** the user searches, **then** matches are highlighted and the user can step through them. |
| F06-AC9 | **Given** a body, **then** it is shown as display lines: split on `\n`, with the `\r` of CRLF removed, and lines longer than the wrap width split without breaking a UTF-8 character. Invalid UTF-8 shows as U+FFFD. |
| F06-AC10 | **Given** a 50 MB body, **when** the user scrolls (wheel or jump), **then** frame time stays under 16 ms (p95), and only visible lines are built as widgets. Indexing runs off the UI thread. |
| F06-AC11 | **Given** a body larger than the viewer supports (4 GiB), **then** the viewer shows `body is <n> bytes; the viewer supports bodies up to 4294967295 bytes`. |

## F07 Tabs
Scope: open, close, reorder, dirty indicator, restore on restart.

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
