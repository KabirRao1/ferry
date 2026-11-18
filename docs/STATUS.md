# Status

**Phase:** 5 in progress (user asked 2026-10-19 to build the remaining phases). Phase 4 pushed in 150afdf.

## v0.1.0 release prep (2026-11-18, resume here)
- User asked to finish the project for public use: verify everything, show testing with API keys, release, and write a launch write-up. Decision: ship v0.1.0 with Phases 0-4 plus the Phase 5 parts done (import/export, snippets); Phases 5 (rest), 6 and 7 stay on the roadmap.
- Added F16-AC8: `ferry-cli run <file>.postman_collection.json [-e <file>.postman_environment.json] [--env-var k=v]...` (in-memory workspace via `ferry_app::file_workspace`).
- Fixed: scripts now run as function bodies (top-level `return` works, as in Postman); found by running the demo tour against live APIs.
- New `docs/TRY_WITH_API_KEYS.md` + `docs/demo/API Keys Tour` collection/environment (httpbin, NASA DEMO_KEY, GitHub, Anthropic); verified live, GitHub path with a real token.
- Nightly: fuzz 10 min/target, mutants 24 shards (8 shards all hit the time limit; shard 0 alone had 16 surviving mutants to close once the run completes).
- Release workflow: changelog notes, build provenance attestations, Windows (Azure Artifact Signing) and macOS (Developer ID + notarization) signing gated on secrets; see `docs/CODE_SIGNING.md`. The release workflow has never run; dispatch it once before tagging.
- RUSTSEC-2026-0543 (chrono unmaintained, new since the last CI run) failed cargo-deny; chrono only read the local UTC offset, so it is replaced by jiff 1 (`ZonedDateTime::now().offset()`), checked against the OS offset on Windows.
- Open Dependabot PR #2 (sysinfo 0.40 breaks xtask, base64 0.23) left for after the release.

## Phase 3 progress (resume here)
- Phase 3 complete: CI run 39364317363 (c2b6ed3) green on Linux, macOS and Windows, fuzz smoke included. Linux and macOS budget baselines recorded from it (binary 15.09 MB and 10.10 MB; idle RSS 1.64x and 1.06x; macOS scroll p95 15.47 ms, FND-008).
- User asked (2026-10-19) to build the remaining phases (4-7) for a public release; go phase by phase, each pushed green, with a short summary at each boundary.
- Phase 4 done: F15 scripts (ferry-script, QuickJS), F16 Collection Runner and ferry-cli, F22-AC5 Visualize (checked by eye with `examples/preview_probe.rs visualize`: inline script runs, fetch is blocked; Preview still runs no scripts). Fuzz target `script`; data files fuzzed in `workspace_formats`. Windows budgets re-recorded (ADR-0048, 15.23 MB); Linux and macOS baselines recorded from CI run 39377003830 (green on all OSes). Pushes were held after d71fd70 (its CI failed only the binary-size budget) until the re-baseline.
- 555 tests pass on Windows; clippy clean.
- Mutation testing: the local Phase 3 run was stopped by the system for low memory after 132 of 523 core mutants (113 caught, 1 timeout, 14 unviable, 4 missed); store and app were not reached. The 4 missed (collection.rs `toml_error` line number and `restore_originals`; cookies.rs empty `SameSite` and `parse_date` minute/second check) and 8 hand-applied mutants of the FND-017 limit are all caught by new tests. The nightly covers the rest; pushes to main mutation-test nothing (their diff against origin/main is empty).
- Budgets re-recorded on Windows (ADR-0042): binary 12.73 MB, idle RSS with 10 tabs 28.0 MB (1.24x), cold start 95 ms, 50 MB scroll p95 3.21 ms. Linux and macOS baselines are cleared; record them from the CI run's logged values. FND-018: the budget used to measure `ferry` built with the examples, which added a test-only font.
- Open: FND-016 (binary-size investigation). Known limits: tooltips cannot draw over the Preview web view; keyboard shortcuts do not reach Ferry while the web view has focus.
**Next step:** finish the Phase 3 quality pass, then Phase 4 (scripts, Collection Runner, CLI, Visualize).

## Phase 2 results
- Scope changed to local Postman parity (ADR-0027) with an on-demand web view for previews (ADR-0028). Research and tiers: `docs/POSTMAN_PARITY.md`.
- Postman-style workbench: header, sidebar, tab strip, request builder (Params, Authorization, Headers, Body, Scripts, Settings), response pane (Body, Cookies, Headers, Test Results), footer, resizable split.
- Requests: params table synced with the URL, custom methods, headers with auto-generated list and bulk edit, bodies (form-data with files, urlencoded, raw with JSON validation and Beautify, binary), Basic / Bearer / API key auth, follow-redirect and TLS-verify options.
- Responses: time breakdown (DNS, connect, waiting, download), Pretty JSON, Raw and Hex views.
- Tabs: new, close with confirmation, drag to reorder, dirty dot, restored on restart from an atomically written `session.toml`.
- 246 tests: f00 19, f01 37, f02 19, f03 19, f04 8, f05 42, f06 64, f07 27, f14 11.
- Coverage: core 95%, app 97%, store 95%, http 94%.
- Budgets on Windows: idle RSS with 10 tabs 26.2 MB (1.21x the empty window), cold start 87–105 ms, 50 MB scroll p95 3.5–4.0 ms, binary 9.37 MB as then measured (8.95 MB as released; FND-018).
- Mutation testing (Phase 2 diff, core/app/store): 389 mutants, 26 survivors on the first pass, all closed with new tests; re-runs show 0 missed.
- Findings: FND-010 (critical, fixed: deeply nested JSON could exhaust memory), FND-011 (low, fixed).

## CI
- CI run 38977119070 green on Linux, macOS and Windows. Linux and macOS baselines recorded from it: binary 12.41 MB and 7.87 MB; idle RSS with 10 tabs 1.52x and 1.05x.
- FND-008 is still open: scrolling a 50 MB body on macOS CI is close to the 16 ms p95 budget (15.7 ms in run 38977119070; advisory).

## Blockers
- None.

## Notes for the next session
- Work alone (ADR-0001): write the contract and failing tests, implement, then run `cargo xtask gate`.
- `git add` new files before running the gate: the mutants step only sees tracked changes (ADR-0010).
- `cargo xtask budgets` opens real windows for about 30 s and rebuilds `target/release/ferry.exe`; close any running Ferry first on Windows.
- Run `cargo mutants` with `-j 1` for workspace-wide test runs on this machine (`-j 4` ran out of memory); per-crate runs (`--test-workspace false`) manage `-j 3`.
- Edit files with Python only using `encoding='utf-8'`, and put scripts with backslashes in files: heredocs mangle them.
- Commits carry no Claude attribution (user preference).
