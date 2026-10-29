# Ferry: project overview

One-page summary of what Ferry is, how it is built and how it is verified. Details live in the linked docs; numbers here are as of Phase 5 in progress (2026-10-29) and are updated at each phase.

## What it is

Ferry is a native desktop API client with Postman's layout and Postman's local feature set, written in Rust. It needs no account and no cloud: collections, environments and settings are plain TOML files in a folder you can keep in git. It runs on Windows, macOS and Linux and is licensed Apache-2.0.

## Features built

| Area | What works |
|---|---|
| Requests | Methods (including custom), URL with a params table kept in sync both ways, headers with auto-generated ones shown, bodies (form-data with files, urlencoded, raw with JSON validation and Beautify, binary), Basic / Bearer / API key auth, redirect and TLS options, cancel, 30 s timeout |
| Responses | Status, time with DNS / connect / waiting / download breakdown, size; Pretty JSON, Raw and Hex views over bodies up to hundreds of MB; search with highlights; JSONPath filter (RFC 9535); HTML / image / PDF Preview in an embedded web view; Open in Browser |
| Workspace | Collections, folders and requests as files; rename, duplicate, delete, drag to reorder or move; outside edits picked up live; collection and folder settings; saved examples |
| Variables | `{{name}}` everywhere, environments, collection variables and globals in precedence order, nesting with cycle and depth limits, dynamic variables (`{{$guid}}`, `{{$timestamp}}`, …), secret values kept in git-ignored local files |
| Activity | History of the last 100 sends; an RFC 6265 cookie jar with a manager; a console logging every request as sent |
| Scripts | Pre-request and post-response JavaScript on requests, folders and collections with Postman's `pm` API (`pm.test`, Chai-style `pm.expect`, `pm.response`, `pm.request`, variables, `pm.sendRequest`, `pm.visualizer`); Test Results tab; Visualize view rendering Handlebars templates |
| Automation | Collection Runner (iterations, delay, CSV/JSON data files, `setNextRequest`, live results, JSON report) and `ferry-cli run` for CI with exit codes |
| Import, export, code | Postman collections (v2.0 and v2.1) and environments imported with a list of anything not carried over, and exported as Postman v2.1 without secrets; cURL commands pasted into the URL bar; the current request as code in 12 languages (cURL, HTTPie, Python, JavaScript, Node.js, Go, Rust, Java, C#, PHP, Ruby, Swift) |
| Next (Phase 5) | OpenAPI / Swagger / HAR import, advanced auth (OAuth 2.0 with PKCE, AWS SigV4, Digest, NTLM, JWT, …), proxy and certificates, an OS-keychain vault |

The full plan is in [SPEC.md](SPEC.md) (every acceptance criterion has an ID) and [FEATURES.md](FEATURES.md).

## Architecture

Eight Rust crates with a dependency direction enforced by a build check (`cargo xtask arch`):

| Crate | Role |
|---|---|
| `ferry-core` | Pure domain logic, no I/O: request drafts and building, params/URL sync, variables, collections and file formats, cookies, history, JSON pretty-printing and JSONPath, body line index, code generators, cURL parsing |
| `ferry-http` | Sending with reqwest/rustls: timing breakdown, redirects with the cookie jar on every hop, cancellation, error classification |
| `ferry-store` | Files: atomic writes, the session, the workspace folder (load, apply operations, watch), history, cookie jar; behind an `Fs` trait with an in-memory version for tests |
| `ferry-script` | Scripts: a fresh QuickJS runtime per script with 64 MB and 5 s limits and no file, process or network access; the `pm` API as a JavaScript prelude over a small native bridge |
| `ferry-app` | The application as an Elm-style state machine: `Message` → `update()` → `Effect`, with no UI dependency, so every user flow is tested headless; scripted sends, the runner loop, Visualize |
| `ferry-ui` | iced views (GPU-free rendering with tiny-skia), the shell that runs effects, the embedded web view (WebView2 / WKWebView through wry) created only on demand |
| `ferry-cli` | Headless runner for CI on top of `ferry-app` |
| `xtask` | The merge gate: fmt, clippy, tests, coverage floors, cargo-deny, performance budgets, architecture check, mutation testing |

Key techniques, each recorded as a decision in [DECISIONS.md](DECISIONS.md) (53 ADRs):

- **Elm architecture with injected effects**: `update()` never does I/O or reads the clock; time and randomness are injected (`Ambient`), so behavior is deterministic in tests.
- **Large bodies without large memory**: a line index over the raw bytes and a virtualized view render only visible lines; filtering and search run off the UI thread with job ids that drop stale results.
- **Round-trip testing for formats**: the cURL generator is checked by parsing its output back, and Postman export by importing it again, with property tests over random requests and over Postman-shaped JSON with wrong-typed fields.
- **Git-friendly storage**: one TOML file per request; secret and script-set values go to `*.local.toml` files the workspace's `.gitignore` excludes; all writes are atomic.
- **Sandboxed scripting**: QuickJS (`rquickjs`) with memory, time and stack limits, an interrupt handler for cancel and timeouts that excludes time spent waiting on `pm.sendRequest`, and a host trait so the script crate never touches the network itself.
- **Web view isolation**: content served from memory through a custom protocol; navigation, new windows and downloads blocked; scripts off for Preview, and for Visualize scripts on behind a Content Security Policy that blocks all network access.
- **Security bounds found by testing**: limits on JSON nesting depth and on variable expansion growth after fuzzing found memory blow-ups.

[ARCHITECTURE.md](ARCHITECTURE.md) has the full picture.

## How it is verified

- **691 automated tests**, each named after the acceptance criterion it covers (`f15_ac9_…`): unit, property-based (proptest), headless UI tests with iced's simulator, byte-exact UI snapshot images, and end-to-end tests against local mock servers (wiremock), including the CLI binary.
- **Coverage floors** enforced in CI: core, app and store at 90 % or more (currently 92–99 %).
- **Mutation testing** with cargo-mutants on every change and nightly over everything (8 parallel shards); surviving mutants become new tests.
- **Fuzzing** with cargo-fuzz: 8 targets (JSON, body index, params sync, cookies, variables, file formats, scripts, imports), smoke-run in CI.
- **Performance budgets** checked in CI on Windows, macOS and Linux: idle memory within 2× an empty window with 10 tabs open, cold start under 400 ms, 16 ms frame time while scrolling a 50 MB body, and binary size growth limited to 10 % per phase.
- **Supply chain**: cargo-deny for licenses, advisories and sources.
- **Findings log**: 23 issues found by these checks and fixed, each with its regression test ([FINDINGS.md](FINDINGS.md)).

## Numbers

| | |
|---|---|
| Rust source | about 42,000 lines (crates and xtask), 17,000 of them tests |
| Crates | 8 |
| Tests | 691 |
| Decisions (ADRs) | 53 |
| Findings fixed | 23 |
| Release binary | 15.2 MB on Windows; idle with 10 tabs about 26 MB |
| CI | Linux, macOS and Windows on every push |
