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
| `ferry-core` | Domain types: `RequestDraft` (what the user edits) and `build()` into a sendable `Request`; params/URL sync, headers, bodies, auth, JSON validation and pretty-printing, `Response`, error messages, the body line index and hex view, `Session` | No I/O. Pure functions. Clippy pedantic. |
| `ferry-http` | Sending requests (headers, bodies, files, multipart), per-request redirect and TLS options, timing breakdown, cancellation, error classification | Tested against local servers only. DNS goes through an injectable `Resolver`. |
| `ferry-store` | Atomic writes and the saved session; collections arrive in Phase 3 | Filesystem behind the `Fs` trait so error cases are testable. |
| `ferry-app` | The workbench: `App` (tabs, layout), `Tab` (draft, response, editing state), `Message`, `update()`, `Effect`, `run()` | No rendering. Every user flow is testable headless. |
| `ferry-ui` | iced views (`chrome`, `request`, `response`, `body_view`), the `Shell` that runs effects, and the binary | Views are functions of state plus iced widget state in `editors` (ADR-0031). No business logic. |
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
- **Other effects** are file dialogs (`Effect::PickFile`) and session saves (`Effect::SaveSession`, from an autosave tick every 2 s when something changed, plus a synchronous save on window close).

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
| App flows | `ferry-app/tests/flow.rs` | message sequences, including real local servers |
| Headless UI and pixel snapshots | `ferry-ui/tests` | iced_test |
| Fuzzing | `fuzz/` | cargo-fuzz |
| Mutation | core, app, store | cargo-mutants |
| Performance | `cargo xtask budgets` | real windows, `budgets.toml` |
