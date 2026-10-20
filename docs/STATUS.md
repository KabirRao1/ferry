# Status

**Phase:** 3 in progress (user approved 2026-10-17): every feature built; quality pass under way.

## Phase 3 progress (resume here)
- Commits 18ef86d (F08, F09), a38a041 (F10, F17, F18, F06-AC8, F22-AC4, F32-AC1) and 8e2319a (Preview, quality pass) pushed 2026-10-19. CI run 39303834027 failed on three things, fixed in the next commit: the `set_cookie` fuzz target's assertion (FND-020), cargo-deny rejecting MPL-2.0 crates (ADR-0043), and ferry-app coverage at 89.2% (now 95.8% locally, with F32-AC3 example-tab tests that found FND-019).
- User asked (2026-10-19) to build the remaining phases (4-7) for a public release; go phase by phase, each pushed green, with a short summary at each boundary.
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
