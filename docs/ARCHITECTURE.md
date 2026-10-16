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
   │ rustls    │ │ no I/O     │ │ (Phase 3)   │
   └─────┬─────┘ └─────▲──────┘ └───┬─────────┘
         └─────────────┴────────────┘   http and store may use core
```

| Crate | Owns | Rules |
|---|---|---|
| `ferry-core` | Domain types (`Method`, `Request`, `Response`), URL validation, error messages, formatting, the body line index | No I/O. Pure functions. Clippy pedantic. |
| `ferry-http` | Sending requests, timing, cancellation, error classification | Tested against local servers only. DNS goes through an injectable `Resolver`. |
| `ferry-store` | Collections on disk (Phase 3) | Filesystem behind a trait so error cases are testable. |
| `ferry-app` | `App` state, `Message`, `update()`, `Effect`, `run()` | No rendering. Every user flow is testable headless. |
| `ferry-ui` | iced views and the binary | Views are functions of state. No business logic. |
| `xtask` | Gate, budgets, coverage, mutation and architecture checks | |

`cargo xtask arch` fails the build if any crate depends on one it shouldn't.

## Life of a request

```
 user presses Send
        │
        ▼
 ferry-ui     Message::SendPressed ─────────────────────┐
 ferry-app    update(): validate URL, Phase::Sending,   │
              return Effect::Send { id, request, cancel }
        │                                               │
        ▼                                               │
 ferry-ui     Task::perform(run(effect))  ← UI thread stays free
 ferry-app    run(): Client::send() then BodyView::new()
              (indexing happens here, off the UI thread)
 ferry-http   reqwest + rustls, classify errors
        │
        ▼
 ferry-app    Message::Finished { id, result } → update():
              ignore if `id` is stale, else Phase::Received / Failed / Cancelled
        │
        ▼
 ferry-ui     screen::view(&app) redraws
```

Key points:

- **`update()` never does I/O.** It returns an `Effect` describing the work. That keeps every state transition a plain function call in tests.
- **Every send has an id.** Results for a cancelled or superseded send are ignored.
- **Cancellation** uses a `CancellationToken` stored in `Phase::Sending`. Cancel fires it, and the in-flight `send` returns immediately, dropping the connection.

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
