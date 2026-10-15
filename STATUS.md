# Status

**Phase:** 0 (foundations) complete. Waiting for user review before Phase 1.
**Next step:** Phase 1 walking skeleton: F01-AC1/AC5, F05-AC1/AC6/AC7, F06-AC1 to AC5. Type a URL, send a GET, see the status and body through every crate.

## Phase 0 results
- 48 tests (core 17, app 6, ui 7 headless and snapshot, xtask 18).
- Coverage: core 95%, app 100%.
- Mutation testing: 92 mutants, 0 missed (71 caught, 12 timeouts, 8 unviable) after the fixes in FINDINGS.md.
- Budgets on Windows (tiny-skia, ADR-0018): idle RSS 22 MB (1.05x the empty window), cold start 68 ms, 50 MB scroll p95 3.1 ms, binary 4.0 MB.

## Blockers
- None. The Linux and macOS budget baselines are recorded only after the first CI run (`cargo xtask budgets --record` on each OS).

## Notes for the next session
- Work alone (ADR-0001): write the contract and failing tests, confirm they fail on `todo!()`, implement, then run `cargo xtask gate`.
- Commit new files before running the gate: the mutants step only sees tracked changes (ADR-0010).
- `cargo xtask budgets` opens real windows for about 30 s. Don't use the machine while it runs.
- Run `cargo mutants` with `-j 1` on this machine: `-j 4` ran out of memory.
- Commits carry no Claude attribution (user preference).
