# Contributing to Ferry

Thanks for helping. This guide covers the development setup, how work is structured, and what has to pass before a change merges.

## Development setup

1. Follow the [Quick start](README.md#quick-start) to install Rust and your platform's build tools.
2. Install the test tools (prebuilt binaries via [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) are fastest):

   ```sh
   cargo install cargo-binstall
   cargo binstall cargo-nextest cargo-llvm-cov cargo-deny cargo-mutants
   ```

3. Check everything works:

   ```sh
   cargo nextest run --workspace
   ```

## Repository layout

```
crates/
  ferry-core/    domain types and pure logic (no I/O)
  ferry-http/    sending requests: reqwest + rustls, timing, cancellation
  ferry-store/   files on disk: atomic writes, file watching
  ferry-app/     application state and update() (headless, fully testable)
  ferry-ui/      iced views and the `ferry` binary
xtask/           project tooling: `cargo xtask <command>`
fuzz/            cargo-fuzz targets (Linux, nightly)
docs/            spec, architecture, decisions, findings, status
```

Dependencies only point one way: `ui → app → {core, http, store}`, and `http` and `store` may use `core`. `cargo xtask arch` enforces this. See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## How work is done

Ferry is built **tests first** against a written spec:

1. Every behavior has an acceptance criterion in [`docs/SPEC.md`](docs/SPEC.md) with an ID such as `F05-AC6`.
2. Write the public signatures (marked `// CONTRACT`, bodies `todo!()`) and failing tests. Test names include the AC ID: `f05_ac6_dns_failure_names_host`.
3. Run the tests and confirm they fail **for the right reason** (they hit `todo!()`, not a compile error or a broken fixture).
4. Implement until they pass, then refactor while green.
5. Run the full gate.

Rules that keep the codebase healthy:

- No `unwrap()`, `expect()` or `panic!` outside tests (clippy enforces this).
- No `#[ignore]`d tests and no weakened assertions to get green.
- Tests use local servers only, never the real network.
- No sleeps for synchronization. Inject clocks and IDs.
- Performance budgets are tests: memory, startup, frame time and binary size are checked by `cargo xtask budgets`.

## The merge gate

```sh
cargo xtask gate
```

| Step | Command |
|---|---|
| Formatting | `cargo fmt --all --check` |
| Lints | `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| Tests | `cargo nextest run --workspace` and `cargo test --doc --workspace` |
| Coverage floors | `cargo xtask coverage` (90% core/app/store, 80% http) |
| Licenses and advisories | `cargo deny check` |
| Performance budgets | `cargo xtask budgets` (opens windows for ~30 s) |
| Architecture | `cargo xtask arch` |
| Mutation testing | `cargo xtask mutants` (changed code in core/app/store) |

CI runs the same gate on Linux, macOS and Windows for every pull request.

Mutation testing only sees files git tracks: `git add` new files before running the gate.

## UI snapshots

Headless UI tests render screens to PNG under `crates/ferry-ui/tests/snapshots/` and compare them byte for byte (Windows only; see ADR-0006). If you change the UI on purpose:

1. Delete the affected `*.png` files.
2. Run `cargo nextest run -p ferry-ui` to record new ones.
3. **Look at the new images** before committing them.

## Commits and pull requests

- Keep commits focused, and mention the AC IDs they cover.
- Update `CHANGELOG.md` under **Unreleased** for user-visible changes.
- Record significant design choices as an ADR in [`docs/DECISIONS.md`](docs/DECISIONS.md).
- Bugs found by testing go in [`docs/FINDINGS.md`](docs/FINDINGS.md) with the regression test that covers them.

## Licensing

By contributing, you agree that your contributions are licensed under the Apache License, Version 2.0, as described in the [README](README.md#license).
