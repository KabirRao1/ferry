# Status

**Phase:** 1 (walking skeleton) complete. Waiting for user review before Phase 2.
**Next step:** Phase 2: F02 headers, F03 body, F04 auth, F07 tabs, plus the F01 params table (ADR-0015), custom-method entry, F05-AC2..AC5 options and the F06-AC2 timing breakdown.

## Phase 1 results
- Type a URL, pick a method, Send (button, Enter, Ctrl/Cmd+Enter), Cancel. The response shows status, time, size, body and headers. It runs end to end through core → http → app → ui.
- 114 tests: f00 21, f01 16, f05 34, f06 43. That covers unit, property, insta snapshot, local-server integration, app flows and headless UI with pixel snapshots.
- Coverage: core 97%, app 98%, http 91%.
- Budgets on Windows: idle RSS 23.5 MB (1.08x the empty window), cold start 69 ms, 50 MB scroll p95 3.5 ms, binary 8.15 MB (re-baselined for the HTTP stack, ADR-0021).

## CI
- Linux and macOS budget baselines were cleared for the HTTP stack (ADR-0021). Record them from the first green CI run after this push.
- FND-008 is still open: scrolling a 50 MB body on macOS is at 14.5 ms p95 (budget 16 ms).

## Blockers
- None.

## Notes for the next session
- Work alone (ADR-0001): write the contract and failing tests, confirm they fail on `todo!()`, implement, then run `cargo xtask gate`.
- `git add` new files before running the gate: the mutants step only sees tracked changes (ADR-0010).
- `cargo xtask budgets` opens real windows for about 30 s. Don't use the machine while it runs.
- Run `cargo mutants` with `-j 1` on this machine: `-j 4` ran out of memory.
- Edit files with Python only using `encoding='utf-8'`: Windows defaults to cp1252, and a failed write truncates the file.
- Commits carry no Claude attribution (user preference).
