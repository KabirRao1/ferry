# Ferry

A native, low-memory, offline HTTP API client written in Rust: a lean alternative to Postman.

- **Native UI** with [iced](https://iced.rs). No webview, no Electron.
- **Collections are plain TOML files** in a folder: git-friendly, diffable, reviewable.
- **Offline.** No account, no telemetry.

> Status: Phase 0 (foundations). See [`STATUS.md`](STATUS.md) and [`FEATURES.md`](FEATURES.md).

## Building

Requires the Rust toolchain pinned in `rust-toolchain.toml` (rustup installs it automatically). On Windows you also need the MSVC C++ build tools.
On Linux you need `libxkbcommon-dev` and `libwayland-dev`.

```sh
cargo run --release          # launches `ferry`
```

## Development

| Command | What it does |
|---|---|
| `cargo nextest run --workspace` | All tests (unit, property, headless UI) |
| `cargo xtask gate` | Full merge gate: fmt, clippy, tests, coverage, deny, budgets, arch, mutants |
| `cargo xtask budgets [--record]` | Measure RSS, cold start, 50 MB scroll frame time and binary size against `budgets.toml` |
| `cargo xtask arch` | Enforce crate dependency direction |
| `cargo xtask coverage` | Per-crate line coverage floors |
| `cargo xtask mutants [--all]` | Mutation testing of changed code in core, app, store |
| `cargo +nightly fuzz run line_index` (in `fuzz/`) | Fuzz a parser (Linux) |

Workspace layout:

| Crate | Responsibility |
|---|---|
| `ferry-core` | Domain types and pure logic. No I/O. |
| `ferry-http` | HTTP execution, timing, cancellation |
| `ferry-store` | Filesystem persistence, atomic writes, file watching |
| `ferry-app` | Application state and `update()`. Headless-testable. |
| `ferry-ui` | iced views (the `ferry` binary) |
| `xtask` | Gate, budgets and CI tooling |

Specification: [`SPEC.md`](SPEC.md). Decisions: [`DECISIONS.md`](DECISIONS.md). QA findings: [`FINDINGS.md`](FINDINGS.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option.
