# Findings

QA findings: adversarial, performance and UX. Each one becomes a failing regression test before it is fixed.

Severity: **critical** (data loss, crash, security) · **high** (feature broken) · **medium** (wrong in edge cases, budget at risk) · **low** (polish).

| ID | Severity | Found in | Summary | Regression test | Status |
|---|---|---|---|---|---|
| FND-001 | low | Phase 0, xtask unit test | Binary-size budget rejected growth of exactly +10% because of float rounding (`1100/1000 - 1 = 0.10000000000000009`). | `xtask budgets::tests::f00_ac3_binary_growth_over_10_percent_fails` | fixed |
| FND-002 | medium | Phase 0, `cargo mutants --all` | 6 surviving mutants in `push_wrapped`'s CRLF detection (text.rs:89–90). No test covered CRLF or a lone `\r` at a wrap boundary. | `ferry-core text_index f06_ac9_crlf_at_wrap_boundary_adds_no_line`, `f06_ac9_lone_carriage_return_at_end_wraps_as_content` | fixed |
| FND-003 | low | Phase 0, `cargo mutants --all` | Equivalent mutant `delta < 0` → `<=` in `BodyView::update`. Fixed by changing `ScrolledBy` to `isize` and using `saturating_add_signed`, which removes the comparison. | `ferry-app body_view f06_ac9_scroll_clamps_both_ends` | fixed |
| FND-004 | medium | Phase 0, `cargo mutants` (sequential re-run) | Surviving mutant `>` → `>=` at text.rs:77 in `LineIndex::line`. No example test had a body starting with an empty line, where the mutant reads `body[0 - 1]`; the property test only hit that case by chance. | `ferry-core text_index f06_ac9_leading_empty_lines` | fixed |
| FND-005 | high | Phase 0, gate tooling | `cargo mutants` exits 3 (timeouts) even when some mutants were missed. `xtask` treated 3 as a pass, so the gate would have accepted missed mutants. It now reads `mutants.out/missed.txt`. | `xtask gate::tests::f00_ac5_missed_mutants_fail_even_when_exit_code_reports_timeouts` | fixed |
| FND-006 | medium | First CI run (Linux) | `empty_window` drew no first frame under xvfb, and the budget probe discarded the child's stderr, so the cause was invisible. The probe now reports the tail of stderr; CI installs the X11 runtime libraries winit loads at runtime. | `xtask budgets::tests::f00_ac3_stderr_tail_keeps_last_lines` | fixed (CI run 38792056359) |
| FND-007 | medium | First CI run (fuzz smoke) | The prebuilt cargo-fuzz defaults to a musl target, which is incompatible with AddressSanitizer. Workflows now pass `--target x86_64-unknown-linux-gnu`. | CI fuzz-smoke job | fixed (CI run 38792056359) |
| FND-008 | medium | CI run 38792056359 (macOS) | Scrolling a 50 MB body on macOS reaches 14.5 ms p95 (budget 16 ms), and 21.7 ms in run 38966606471: CPU rendering at Retina 2x on a GPU-less virtualized runner. This is the high-DPI risk from ADR-0018. Advisory in CI. | `cargo xtask budgets` (macOS) | open (Phase 4: measure on real hardware; consider partial redraw or `gpu` above a pixel threshold) |
| FND-009 | low | Phase 1, `cargo xtask mutants` | Equivalent mutant `&&` → `\|\|` in `parse_url`: once the literal-prefix check (ADR-0024) passes, the scheme and host checks can't fail. Removed the dead checks; the guarantee is now a property test. | `ferry-core request f01_ac5_prop_accepted_urls_are_http_with_host`, `f01_ac5_prefix_with_empty_host_rejected` | fixed |
