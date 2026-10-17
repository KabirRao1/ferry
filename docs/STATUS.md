# Status

**Phase:** 3 in progress (user approved 2026-10-17).
**Next step:** Phase 3: F08 variables and environments, F09 collections, F10 history, F17 cookies, F18 console, F32 saved examples, F22 Preview and Visualize (see `docs/SPEC.md#phases`).

## Phase 2 results
- Scope changed to local Postman parity (ADR-0027) with an on-demand web view for previews (ADR-0028). Research and tiers: `docs/POSTMAN_PARITY.md`.
- Postman-style workbench: header, sidebar, tab strip, request builder (Params, Authorization, Headers, Body, Scripts, Settings), response pane (Body, Cookies, Headers, Test Results), footer, resizable split.
- Requests: params table synced with the URL, custom methods, headers with auto-generated list and bulk edit, bodies (form-data with files, urlencoded, raw with JSON validation and Beautify, binary), Basic / Bearer / API key auth, follow-redirect and TLS-verify options.
- Responses: time breakdown (DNS, connect, waiting, download), Pretty JSON, Raw and Hex views.
- Tabs: new, close with confirmation, drag to reorder, dirty dot, restored on restart from an atomically written `session.toml`.
- 246 tests: f00 19, f01 37, f02 19, f03 19, f04 8, f05 42, f06 64, f07 27, f14 11.
- Coverage: core 95%, app 97%, store 95%, http 94%.
- Budgets on Windows: idle RSS with 10 tabs 26.2 MB (1.21x the empty window), cold start 87–105 ms, 50 MB scroll p95 3.5–4.0 ms, binary 9.37 MB (re-baselined, ADR-0036).
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
