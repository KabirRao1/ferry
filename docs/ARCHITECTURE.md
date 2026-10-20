# Architecture

Ferry is a Cargo workspace of five library and binary crates plus `xtask` tooling. It follows the **Elm architecture**: all state lives in one place, changes only through messages, and the screen is redrawn from state.

## Crates

```
                ┌────────────┐
                │  ferry-ui  │  iced views, the `ferry` binary
                └─────┬──────┘
                      │
                ┌─────▼──────┐
                │ ferry-app  │  state, Message, update(), effects
                └─┬────┬───┬─┘
         ┌────────┘    │   └────────┐
   ┌─────▼─────┐ ┌─────▼──────┐ ┌───▼─────────┐
   │ferry-http │ │ ferry-core │ │ ferry-store │
   │ reqwest,  │ │ pure logic │ │ files, watch│
   │ rustls    │ │ no I/O     │ │ session     │
   └─────┬─────┘ └─────▲──────┘ └───┬─────────┘
         └─────────────┴────────────┘   http and store may use core
```

| Crate | Owns | Rules |
|---|---|---|
| `ferry-core` | Domain types: `RequestDraft` (what the user edits) and `build()` into a sendable `Request`; params/URL sync, headers, bodies, auth, JSON validation, pretty-printing and the `JSONPath` filter, `Response`, error messages, the body line index, hex view and search; `variables` (resolution, dynamic variables), `collection`, `environment`, `workspace` (the tree and the file `Op`s that change it), `history`, `cookies` (an RFC 6265 jar), `Session` | No I/O. Pure functions; time and randomness are passed in. Clippy pedantic. |
| `ferry-http` | Sending requests (headers, bodies, files, multipart), per-request redirect, TLS and cookie options, the cookie jar on every redirect hop, timing breakdown, cancellation, error classification | Tested against local servers only. DNS goes through an injectable `Resolver`. |
| `ferry-store` | Atomic writes; the session; the workspace folder (load, apply `Op`s, watch for outside changes); history and its bodies; the cookie jar | Filesystem behind the `Fs` trait (`MemFs` in tests) so error cases are testable. Unreadable files are moved to `.bak`. |
| `ferry-script` | Pre-request and post-response scripts: a fresh QuickJS runtime per script with a 64 MB memory limit and a 5 s time limit, and the `pm` API as a JavaScript prelude over a small native bridge (ADR-0044) | Depends on `ferry-core` only; sending and `console` go through the `Host` trait, which `ferry-app` implements with `ferry-http`. |
| `ferry-app` | The workbench: `App` (tabs, layout, workspace, history, cookies, console), `Tab` (draft or document, response, editing state), `Message`, `update()`, `Effect`, `run()`; search and filter jobs; what to preview; scripted sends (`scripting`), the Collection Runner's loop (`runner`) and its tab, Visualize pages | No rendering, no clock or randomness of its own (`Ambient`). Every user flow is testable headless. |
| `ferry-ui` | iced views (`chrome`, `sidebar`, `request`, `response`, `body_view`, `docs`, `dialogs`, `console`), the `Shell` that runs effects, the Preview web view, and the binary | Views are functions of state plus iced widget state in `editors` (ADR-0031). No business logic. |
| `ferry-cli` | `ferry-cli run`: loads a workspace with `ferry-store` and drives the runner from `ferry-app` headless | No UI dependency; saves nothing but an optional report (ADR-0047). |
| `xtask` | Gate, budgets, coverage, mutation and architecture checks | |

`cargo xtask arch` fails the build if any crate depends on one it shouldn't.

## Life of a request

```
 user presses Send
        │
        ▼
 ferry-ui     Message::SendPressed ─────────────────────┐
 ferry-app    update(): RequestDraft::build() validates │
              URL and headers, applies body and auth,   │
              Phase::Sending, returns                   │
              Effect::Send { tab, id, request, cancel } │
        │                                               │
        ▼                                               │
 ferry-ui     Task::perform(run(effect))  ← UI thread stays free
 ferry-app    run(): Client::send(), then body views
              (line index, pretty JSON, hex) off the UI thread
 ferry-http   reqwest + rustls, timing marks, classify errors
        │
        ▼
 ferry-app    Message::Finished { tab, id, result } → update():
              routed to its tab; ignored if the tab closed or
              `id` is stale, else Received / Failed / Cancelled
        │
        ▼
 ferry-ui     screen::view(&app) redraws
```

Key points:

- **`update()` never does I/O.** It returns an `Effect` describing the work. That keeps every state transition a plain function call in tests.
- **Every send has an id.** Results for a cancelled or superseded send are ignored.
- **Cancellation** uses a `CancellationToken` stored in `Phase::Sending`. Cancel fires it, and the in-flight `send` returns immediately, dropping the connection. Closing a tab cancels its send.
- **Other effects** are file and folder dialogs, workspace changes (`Effect::Workspace`), history and cookie saves, searches and filters, Open in Browser, and session saves (`Effect::SaveSession`, from an autosave tick every 2 s when something changed, plus a synchronous save of the session and the cookie jar on window close). `Effect::Batch` runs several.
- **Before a send**, `update()` fills in inherited auth and resolves `{{variables}}` (environment, then collection, then globals), and hands `run()` a snapshot of the cookie jar. The response brings back the cookies set on every hop, and `update()` stores them in the app's jar, records history and completes the console entry.

## The workspace

Collections, environments and globals are plain TOML files in a workspace folder that a team can keep in git (ADR-0037). `ferry-app` decides every change as a list of `Op`s (create, write, rename, copy, remove); `ferry-store` applies them and loads the folder again, and the app reconciles open tabs with the new tree. A file watcher reloads the folder when something else changes it. Secret variable values live in `*.local.toml` files that the workspace's `.gitignore` excludes (ADR-0040).

## Searching and filtering

Search in the response body and the `JSONPath` filter run as effects on a worker thread. Each request carries a job id, so a late answer for an older query or expression is dropped. Matches are byte offsets; the viewer splits only the visible lines around them.

## Preview

Preview shows HTML, image, audio, video and PDF bodies in the system web view (WebView2 on Windows, WKWebView on macOS), as a native child window laid over the body area (ADR-0041). `App::preview()` says what should be shown; after every update the `Shell` creates, replaces or destroys the web view to match, and moves it when the body area's bounds change. The body is served from memory through a custom protocol; scripts, navigation away, new windows and downloads are refused, and there is no IPC handler. Linux, or a web view that fails to start, offers Open in Browser instead.

## Large responses

Bodies are kept as `bytes::Bytes` (shared, never copied). `ferry_core::text::LineIndex` records where each display line starts (4 bytes per line). The viewer builds widgets only for the lines that fit the window, so frame cost does not depend on body size. A 50 MB body scrolls at about 3 ms per frame (see ADR-0007).

## Rendering

The UI renders on the CPU with tiny-skia by default. Compared with GPU rendering through wgpu, that uses about 10× less memory and opens about 10× faster (ADR-0018). GPU rendering is available with `--features gpu`.

## Testing layers

| Layer | Where | Tool |
|---|---|---|
| Unit and property | `crates/*/tests` | nextest, proptest |
| Error-message snapshots | `ferry-core/tests/snapshots` | insta |
| HTTP integration | `ferry-http/tests` | wiremock and raw local sockets |
| App flows | `ferry-app/tests` | message sequences, including real local servers; workspaces in `MemFs` |
| Headless UI and pixel snapshots | `ferry-ui/tests` | iced_test |
| Fuzzing | `fuzz/` | cargo-fuzz |
| Mutation | core, app, store | cargo-mutants (nightly in 8 shards) |
| Web view | `ferry-ui/examples/preview_probe.rs` | a real window, checked by eye |
| Performance | `cargo xtask budgets` | real windows, `budgets.toml` |
