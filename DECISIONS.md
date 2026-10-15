# Decisions

Architecture decision records. Newest last. Format: context, decision, consequences.

---

## ADR-0001: Single-agent workflow (2026-10-14, user decision)
**Context.** The kickoff (`docs/kickoff.md` section 3) defines five subagents (spec-qa, developer, adversarial-qa, perf-qa, ux-qa) coordinated by an overseer.
**Decision.** The user asked for no subagents. One session plays every role. `.claude/agents/` is not created.
**Consequences.** The tests-first discipline still holds: for each behavior, write the `// CONTRACT` signatures with `todo!()` bodies and the failing tests, run them and confirm they fail on `todo!()`, then implement. Adversarial, perf and UX passes become explicit steps of the per-feature checklist. The "developer never edits tests" rule becomes: test changes after implementation starts need a recorded reason (a commit message or an entry here).

## ADR-0002: License MIT OR Apache-2.0 (2026-10-14, user accepted recommendation)
Dual license, the Rust ecosystem default. `cargo deny` allows only permissive licenses (see `deny.toml`).

## ADR-0003: Toolchain (2026-10-14)
Rust 1.99.0 pinned in `rust-toolchain.toml`, edition 2024, MSVC on Windows (VS 2026 C++ tools + Windows SDK 10.0.26100). Nightly is installed only for `cargo-fuzz`. Tools: cargo-nextest 0.9.148, cargo-llvm-cov 0.9.1, cargo-deny 0.20.2, cargo-mutants 27.1.0.

## ADR-0004: cargo-deny policy (2026-10-14)
**Decision.** `multiple-versions = "warn"`: iced (winit, wgpu) alone pulls in 3 versions of `windows-sys` and several duplicate `windows-*` crates, so "deny" would fail on day one. Two *unmaintained* (not vulnerable) advisories reached only through iced are ignored, each with a reason: `paste` (RUSTSEC-2024-0436, compile-time proc-macro via wgpu-hal → metal) and `ttf-parser` (RUSTSEC-2026-0192, via cosmic-text).
**Consequences.** Revisit both on every iced upgrade. Vulnerability advisories still fail the gate.

## ADR-0005: Budget enforcement and measurement (2026-10-14)
**Context.** Timing on shared CI runners (3 OSes, often no GPU) is noisy, so hard timing gates there would fail at random.
**Decision.**
- Locally, every budget is a hard gate (`cargo xtask budgets`).
- In CI (`--ci` or `CI` set), RSS ratio and binary size are hard gates. Cold start and scroll frame time are reported as advisory.
- Baselines are per OS (`[baseline.<os>]` in `budgets.toml`), since binary size and RSS differ by platform.
- Cold start = process spawn to first `RedrawRequested`, signalled by the `FERRY_PROBE` stdout marker. It is a median of 5.
- Idle RSS = OS-reported resident set (the working set on Windows) 1.5 s after the first frame, median of 5. It is compared with the `empty_window` example built with identical iced features.
- Scroll frame time = the real windowed app (`scroll_probe` example) with `ICED_PRESENT_MODE=immediate`, so vsync does not cap the frame interval. 300 frames after 30 warm-up frames, p95 of the frame-to-frame interval.
- The 10-tab idle RSS budget applies once F07 exists. Until then, `ferry` idles in its empty state.

## ADR-0006: Headless UI testing with iced_test (2026-10-14)
**Context.** The kickoff asked Phase 0 to confirm iced's headless testing support.
**Finding.** iced 0.14 ships `iced_test`. Its `Simulator` builds any `Element` with no window. It can find widgets by text, click, type, simulate raw events, collect emitted messages, and render snapshots (PNG or hash) through a headless renderer.
**Decision.** UI tests use `iced_test::Simulator` directly; no xtask screenshot driver is needed. `.cargo/config.toml` pins `ICED_TEST_BACKEND=tiny-skia`, so snapshots are CPU-rendered and do not depend on the GPU. Pixel snapshots (`tests/snapshots/*-tiny-skia.png`) are compared byte-exact on Windows only for now (`#[cfg(windows)]`). They may also match on Linux and macOS, since tiny-skia and the bundled fonts are pure Rust; widen this once CI confirms. Behavior tests (find, click, messages) run on every OS.

## ADR-0007: Large-body viewer architecture (2026-10-14, Phase 0 spike)
**Decision.**
- `ferry_core::text::LineIndex` stores only display-line *starts* as `u32` (4 bytes per line; ends are derived from the body), which caps bodies at 4 GiB. Lines longer than the wrap width (4096 bytes in the viewer) are split at UTF-8 boundaries, so a 50 MB minified JSON body does not become one unrenderable line.
- `ferry_app::body_view::BodyView` holds `Arc<[u8]>`, the index and the top line. Scrolling is O(1). `visible(rows)` decodes only the visible lines (lossy UTF-8).
- `ferry_ui::body_view::view` uses `responsive` to compute visible rows, then builds a column of only those lines, a `mouse_area` for the wheel, and an inverted `vertical_slider` as the scrollbar. No f32 content-height precision issues at millions of lines.

**Measurements** (Windows 11, release build, 50 MB body, 546,135 lines):
| Metric | Result |
|---|---|
| Index build | 4.3 to 4.7 ms. Index memory ≈ 2.1 MB. |
| view() + layout, headless | p95 4 to 6 ms |
| Real window (wgpu, vsync off) | frame p50 0.6 to 0.7 ms, p95 1.6 to 1.9 ms, max under 3.5 ms |
| Real window (tiny-skia, the default since ADR-0018) | frame p95 3.1 to 3.7 ms |
| Headless tiny-skia draw at 2x | about 22 ms |

The headless draw figure is not representative: `Simulator` creates a fresh renderer per frame (cold glyph cache) and reads back pixels, so it is not used for the budget.

**Open for F06.** Pretty-printing a 50 MB JSON body doubles memory or more. Options to choose between when F06-AC6 starts: (a) format into a second buffer off-thread; (b) a lazily generated pretty view using formatter checkpoints every N lines (the depth and in-string state), so only visible lines are produced.

## ADR-0008: Dependency direction details (2026-10-14)
`ui -> app -> {core, http, store}` as in the kickoff. In addition, `http` and `store` may depend on `core`, so they can use domain types; the kickoff only fixes that `core` depends on nothing. `ui` depends only on `app`, and `app` re-exports any core types the UI needs. `xtask` depends on no workspace crate. The rules apply to every dependency kind, dev-dependencies included. They are enforced by `cargo xtask arch`.

## ADR-0009: Fuzzing on Linux (2026-10-14)
`cargo-fuzz` (libFuzzer) has limited Windows support. Fuzz targets live in `fuzz/`, which is excluded from the workspace. CI runs them on Linux: a short smoke run per push and long runs nightly. Locally on Windows, the proptest properties cover the same parsers.

## ADR-0010: Mutation testing scope (2026-10-14)
`cargo xtask mutants` runs `cargo mutants --in-diff` against `origin/main` (or `HEAD` when there is no remote) for `ferry-core`, `ferry-app` and `ferry-store`, using nextest and the whole workspace's tests. Untracked files are not in a git diff, so new files must be committed (or `--all` used) before the gate's mutation step sees them. The nightly CI job runs `--all`. Any missed mutant fails the gate. A *timeout* (the mutation made the tests hang, for example an infinite wrap loop) counts as caught. An equivalent mutant (one that cannot change behavior) is fixed by rewriting the code to remove it, not by excluding it.

## ADR-0011: Secrets layer for environments (2026-10-14, user accepted recommendation)
Environment values may come from a sibling `*.local.toml` file that overrides the committed values. Ferry writes a `.gitignore` entry for `*.local.toml` when it creates a collection folder. Specified in F08.

## ADR-0012: Location of non-collection state (2026-10-14, user accepted recommendation)
Open tabs, history and settings live in the OS app-data directory, never in a collection folder. History keeps the last 100 requests by default, storing metadata plus response bodies truncated to 1 MB.

## ADR-0013: Item order inside collection folders (2026-10-14, user accepted recommendation)
Items sort by name. An optional `_order.toml` in a folder overrides the order. Ferry writes it only when the user reorders.

## ADR-0014: Windows filesystem rules for collections (2026-10-14, user accepted recommendation)
Request names map to filenames through a sanitizer. It replaces `<>:"/\|?*` and control characters, avoids reserved names (`CON`, `NUL`, `COM1`, and so on), trims trailing dots and spaces, and resolves case-insensitive collisions with a suffix. The file watcher ignores events caused by Ferry's own atomic writes (each write records the file's content hash or mtime).

## ADR-0015: F01 params table moves to Phase 2 (2026-10-14)
The kickoff lists "minimal F01" for Phase 1 and does not schedule the rest of F01. The params table (F01-AC2 to AC4 and AC6) ships in Phase 2 alongside F02 headers, since both are editable key/value tables built on the same widget.

## ADR-0016: Renderer features (2026-10-14). Superseded by ADR-0018
`ferry-ui` uses iced's default rendering stack: wgpu, with tiny-skia as a fallback when no GPU is available. The `empty_window` baseline uses the same features, so the RSS ratio compares like with like. Phase 4 performance hardening will measure tiny-skia-only builds as a lower-memory alternative.

## ADR-0017: Postman baseline deferred (2026-10-14)
Postman is not installed on the development machine. Budgets are calibrated against the empty iced window only. The Postman comparison table in the README is produced in Phase 4.

## ADR-0018: CPU rendering (tiny-skia) by default (2026-10-15)
**Context.** The first budget calibration with iced's default wgpu renderer showed the framework floor alone breaking the budgets. An empty iced window needed 805 ms to reach its first frame (budget: 400 ms) and held 211 MB RSS, mostly GPU driver memory. Ferry added only 4% on top of that floor.
**Measurements** (Windows 11, release builds, same machine and method):

| | wgpu | tiny-skia |
|---|---|---|
| Empty window: cold start / RSS | 805 ms / 211 MB | 50 ms / 21 MB |
| Ferry: cold start / RSS | 835 ms / 220 MB | 68 ms / 22 MB |
| Scrolling a 50 MB body, frame p95 | 1.55 ms | 3.1 to 3.7 ms |
| Release binary | 9.96 MB | 3.99 MB |

**Decision.** `ferry-ui` builds iced with `tiny-skia` only. wgpu becomes the opt-in `gpu` Cargo feature. Budgets are calibrated against an empty window built with the same (tiny-skia) features.
**Consequences.** This fits the "low-memory" pitch (a 10x RSS and cold-start improvement), and the binary is 60% smaller. The CPU cost of drawing grows with window pixels, so frame time on 4K or high-DPI displays must be measured in Phase 4. If it exceeds the budget there, options are partial redraw, or defaulting to `gpu` only above a pixel threshold. This is reversible by flipping a feature. First CI evidence (run 38792056359): the 50 MB scroll p95 is 1.8 ms on Linux and 4.6 ms on Windows, but 14.5 ms on macOS at Retina 2x (FND-008). The `paste` advisory (ADR-0004) now applies only to `--features gpu` builds on macOS.

---

## v2 candidates
(none yet)
