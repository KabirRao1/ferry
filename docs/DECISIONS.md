# Decisions

Architecture decision records. Newest last. Format: context, decision, consequences.

---

## ADR-0001: Single-agent workflow (2026-10-14, user decision)
**Context.** The kickoff (`docs/kickoff.md` section 3) defines five subagents (spec-qa, developer, adversarial-qa, perf-qa, ux-qa) coordinated by an overseer.
**Decision.** The user asked for no subagents. One session plays every role. `.claude/agents/` is not created.
**Consequences.** The tests-first discipline still holds: for each behavior, write the `// CONTRACT` signatures with `todo!()` bodies and the failing tests, run them and confirm they fail on `todo!()`, then implement. Adversarial, perf and UX passes become explicit steps of the per-feature checklist. The "developer never edits tests" rule becomes: test changes after implementation starts need a recorded reason (a commit message or an entry here).

## ADR-0002: License MIT OR Apache-2.0 (2026-10-14, user accepted recommendation; superseded by ADR-0026)
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
`cargo xtask mutants` runs `cargo mutants --in-diff` against `origin/main` (or `HEAD` when there is no remote) for `ferry-core`, `ferry-app` and `ferry-store`, using nextest and the whole workspace's tests. Untracked files are not in a git diff, so new files must be committed (or `--all` used) before the gate's mutation step sees them. The nightly CI job runs `--all`, split into 8 parallel shards (`--shard K/8`) since one job exceeded GitHub's 3-hour limit (run 39014749941). Any missed mutant fails the gate. A *timeout* (the mutation made the tests hang, for example an infinite wrap loop) counts as caught. An equivalent mutant (one that cannot change behavior) is fixed by rewriting the code to remove it, not by excluding it.

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

## ADR-0019: TLS with rustls and the ring provider (2026-10-15)
**Context.** reqwest 0.13 defaults to rustls with the aws-lc-rs crypto provider. Building aws-lc-rs can require CMake and NASM, especially on Windows, which makes the project harder for a newcomer to build.
**Decision.** Use `reqwest` with `rustls-no-provider`, and install `rustls::crypto::ring` as the process-wide provider in `ferry_http::Client::new`. ring builds with only a C compiler. Certificate checks use `rustls-platform-verifier`, so the OS trust store applies, including corporate CAs.
**Consequences.** One fewer build prerequisite. The verifier's root bundle (`webpki-root-certs`) is licensed CDLA-Permissive-2.0, which is now in `deny.toml`.

## ADR-0020: Effects returned from `update()` (2026-10-15)
**Decision.** `App::update(message) -> Option<Effect>` never does I/O. `Effect::Send { id, request, cancel }` describes the work, and `ferry_app::run(effect, client)` executes it on the tokio runtime and returns `Message::Finished { id, result }`. That includes indexing the response body, so the UI thread never indexes 50 MB. Each send gets an id from a counter in `App` (deterministic, no RNG), and results whose id isn't the current `Phase::Sending` are ignored. Cancel fires a `CancellationToken` held in `Phase::Sending`, and `Client::send` races it with `tokio::select!`.
**Consequences.** Every flow is a sequence of plain function calls in tests, including end-to-end runs against local servers (`ferry-app/tests/flow.rs`). `ferry-ui`'s only job is to turn `Effect`s into `iced::Task`s. iced runs on its `tokio` executor feature, because reqwest needs tokio.

## ADR-0021: Binary-size re-baseline for the HTTP stack (2026-10-15)
Adding reqwest, rustls, ring, h2 and the response decoders took the release binary from 3.99 MB to 8.15 MB on Windows. This is a feature cost, not a regression, so the Windows baseline was re-recorded. Dropping zstd (−0.14 MB) and brotli (−0.2 MB) was measured and rejected: an API client should decode whatever a server sends. The Linux and macOS baselines were cleared and are re-recorded from the next CI run.

## ADR-0022: Repository layout (2026-10-15, user request)
The user asked for an industry-standard layout that's easy to run. Root: README (quick start per OS), CHANGELOG (Keep a Changelog), CONTRIBUTING, SECURITY, the Apache-2.0 `LICENSE`, `.editorconfig`, toolchain and lint config. Tracking documents moved to `docs/` (SPEC, DECISIONS, FEATURES, FINDINGS, STATUS, plus a new ARCHITECTURE); the kickoff had them at the root. `.github/` has issue and PR templates, Dependabot (monthly, grouped), and a `release` workflow that builds binaries for four targets from `v*` tags. `llvm-tools-preview` left `rust-toolchain.toml` (`cargo xtask coverage` installs it on demand), so a first build downloads less.

## ADR-0023: Response header representation (2026-10-15)
Headers come from reqwest's `HeaderMap`. Names are lowercase (HTTP/2 requires it, and hyper normalizes HTTP/1). Values that aren't valid UTF-8 are decoded lossily for display. The map groups repeated names, so interleaved headers (`a`, `b`, `a`) are listed as `a`, `a`, `b`, with each name's values in received order. Exact wire order would need a raw HTTP/1 parser, which isn't worth it for v1.

## ADR-0024: Strict URL prefix (2026-10-15)
The WHATWG URL parser (the `url` crate, matching browsers) accepts `https:///path` and `http:/host`, treating them as `https://path` and `http://host`. Ferry requires the literal prefix `http://` or `https://`, followed by a character that isn't `/` or `\`, so a typo never silently sends to a host such as `path`. This matches the error text "Enter a URL starting with http:// or https://".

## ADR-0025: Headless UI test limits (2026-10-15)
`iced_test`'s text selector cannot see a `pick_list`'s selected label, because it isn't a text widget. The visible method label is covered by a pixel snapshot (`screen_method_patch`) instead, and method selection itself by `ferry-app` flow tests. UI tests for Phase 1 were written right after the views rather than before. The views have no logic of their own: every behavior they show is tested first in `ferry-app`.

## ADR-0026: License Apache-2.0 only (2026-10-16, user request)
Supersedes ADR-0002. Ferry is licensed under Apache-2.0 alone, with the text in `LICENSE`; `LICENSE-MIT` is removed. Apache-2.0 adds an explicit patent grant. Dependencies are unaffected: `cargo deny` still allows MIT and the other permissive licenses for third-party crates.

## ADR-0027: Postman parity, local only (2026-10-16, user decision)
The user asked for Ferry to look like Postman and have all of its features, not only API testing. After the research in `docs/POSTMAN_PARITY.md`, the user chose **local parity**. Ferry gets Postman's layout and every feature that runs on the user's machine: scripts and tests (`pm` API), the Collection Runner and a CLI, GraphQL, WebSocket, Socket.IO, SSE, gRPC, MQTT, OAuth 1.0/2.0 and the other auth types, cookies, the console, code snippets, a local mock server, OpenAPI/HAR import and docs generation. This reverses the v1 anti-goals in `docs/kickoff.md` section 1 except accounts, cloud sync and team features, which remain out of scope along with the rest of tier C. The layout follows Postman; the name, icon and colors stay Ferry's own. New features are F14–F35 in `docs/SPEC.md`, and the phase plan there replaces kickoff section 7 from Phase 2 on. Each phase still stops for user review.

## ADR-0028: Web view for Preview and Visualize (2026-10-16, user decision)
The user chose to include an embedded system web view (WebView2 on Windows, WKWebView on macOS, WebKitGTK on Linux) in the default build, for the response Preview and Visualize tabs (F22). This relaxes the kickoff differentiator "no webview": the UI itself stays native iced. To keep the memory budgets, the web view is created only when the user opens Preview or Visualize and is destroyed when they leave it. Idle RSS and cold start are therefore unaffected, and a separate scenario budget will cover an open preview.

## ADR-0029: Timing breakdown granularity (2026-10-16)
F06-AC2 asked for DNS, connect, TLS, time to first byte and download. reqwest exposes a hook around the whole connector (`connector_layer`), which covers TCP and TLS together, and Ferry's own resolver times DNS. There is no hook between the TCP and TLS handshakes short of replacing reqwest's connector. The breakdown is therefore DNS lookup, Connect (TCP and TLS), Waiting and Download. The marks travel with the request in a task-local, so concurrent sends never mix them; a connection reused from the pool has no DNS or Connect phase.

## ADR-0030: Params table shows the URL's text (2026-10-16)
F01-AC2 originally said the table shows pairs percent-decoded. Postman shows them as written and encodes at send time, which also makes the sync lossless: an untouched pair is never re-encoded, and `%20` typed into a value stays `%20`. Ferry follows Postman. Edited rows escape only the characters that would change the query's structure (`&`, `#`, and `=` in keys).

## ADR-0031: Widget state outside the app (2026-10-16)
iced keeps some state outside the view: the method combo box (`combo_box::State`) and multi-line editors (`text_editor::Content`, used for raw bodies and bulk edit). These are iced types, which `ferry-app` must not depend on. `ferry-ui::editors::Editors` holds them per tab and is re-synced after every update: created for new tabs, dropped for closed ones, and reloaded when the app rewrites a body itself (it bumps `Tab::raw_revision`, as Beautify does). Every edit still flows into `ferry-app` as a message, so the app remains the source of truth and its tests cover all behavior.

## ADR-0032: One reqwest client per option set (2026-10-16)
Redirect policy and certificate verification are client-level settings in reqwest, but per-request settings in Ferry (F05-AC4, F05-AC5). `ferry-http::Client` holds up to four reqwest clients, one per combination, built on first use. The default one is built eagerly so a broken TLS setup is reported at startup. Each client has its own connection pool, which is correct: a connection opened without verification must never be reused by a verifying request.

## ADR-0033: Session file and data directory (2026-10-16)
Open tabs are saved to `session.toml` in the per-user data directory (`%APPDATA%\Ferry`, `~/Library/Application Support/Ferry`, `$XDG_DATA_HOME/ferry`), written atomically through `ferry-store` within 2 s of a change and on window close. `FERRY_DATA_DIR` overrides the directory. Under `FERRY_PROBE` (performance budgets) nothing is read or written, so measuring never touches a developer's tabs; `FERRY_PROBE_TABS` opens that many tabs, and the idle RSS budget now measures 10 as the kickoff specified. File dialogs for form-data and binary bodies use `rfd`, which calls the native dialog on each OS.

## ADR-0034: Bounded JSON pretty-printing (2026-10-16)
Pretty-printing indents by depth, so deeply nested JSON grows quadratically: 200 KB of `[` and `]` would need about 10 GB (FND-010). `ferry_core::json::pretty` refuses output larger than 8 times the input plus 64 KiB, and the viewer falls back to Raw. Real responses grow 1.2 to 3 times.

## ADR-0035: Phase 2 test order (2026-10-16)
For Phase 2, contracts and their tests were written in the same pass for each module rather than confirming each test fails on a `todo!()` body first (ADR-0001). The evidence that the tests can fail comes from the mutation run over the Phase 2 diff (`cargo mutants --in-diff`), which the gate requires anyway, plus the defects the tests did catch while the modules were built (FND-010, FND-011).

## ADR-0036: Binary-size re-baseline for Phase 2 (2026-10-16)
The Windows release binary grew from 8.15 MB to 9.37 MB (+15%), past the 10% regression guard. Phase 2 adds the whole Postman-style workbench (combo boxes, text editors, tooltips, tab strip), native file dialogs (`rfd`), multipart bodies (`mime_guess`), and the TOML session (`serde`, `toml`). These are expected growth from the scope change in ADR-0027, not a regression, so the Windows baseline is re-recorded; Linux and macOS baselines are cleared and re-recorded from the next CI run, as in ADR-0021. Idle memory stays at 1.21x the empty window with 10 tabs open.

## ADR-0037: The workspace folder (2026-10-17)
**Context.** Collections, environments and globals must be plain files a team can keep in git (kickoff differentiator), while session, history, cookies and settings stay in the app-data directory (ADR-0012).
**Decision.** Ferry works on one *workspace* folder: `FERRY_WORKSPACE`, else the folder picked with `Open Workspace…` (remembered in the session), else `<data dir>/workspace`. Layout: `collections/<name>/` (with `collection.toml`, subfolders with optional `folder.toml`, one `<name>.toml` per request), `environments/<name>.toml`, `globals.toml`, and a `.gitignore` containing `*.local.toml`. Items are identified by their path relative to the workspace, with `/` separators. `ferry-app` decides every target path (sanitized and de-duplicated in `ferry-core`, F09-AC8) and `ferry-store` only executes the file operation, then rescans. Watcher events also trigger a rescan, and the app reconciles open tabs against the new tree, so Ferry's own writes are naturally no-ops for tabs (they match the file already). Files that fail to parse are shown with their error and never written.
**Consequences.** Every workspace change is a pure `update()` over a freshly loaded tree, testable with an in-memory filesystem. A rescan reads every file, which is fine for the hundreds of requests a real workspace has; if it ever is not, rescans can be scoped to the changed directory.

## ADR-0038: Cookie jar (2026-10-17)
**Decision.** The jar is a pure `ferry-core` type (RFC 6265 storage and matching, with the clock passed in). `ferry-app` owns it. Each send carries a snapshot; `ferry-http` gives reqwest a cookie provider that reads and writes the snapshot through a task-local (as the timing marks do, ADR-0029), so redirect hops store and send cookies correctly and concurrent sends never share state. The `Set-Cookie` headers of every hop come back with the response and `update()` applies them to the app's jar. Without a public suffix list, a `Domain` attribute that is a single label (`com`) is rejected; multi-label public suffixes (`co.uk`) are not detected. reqwest skips the jar when a request already has a `Cookie` header, so Ferry appends the jar's cookies for the first URL itself; later hops of such a request do not get cookies set along the way. Cookies are stored in the data directory, never the workspace, since they are often credentials.

## ADR-0039: History storage (2026-10-17)
**Decision.** `history/history.toml` holds entry metadata and the request as edited (a few KB each); bodies (truncated to 1 MB, ADR-0012) are separate files `history/bodies/<id>`, so adding an entry rewrites a small file and writes one body. Entry ids come from a counter stored in the file. Entries record the clock time from the app's injected clock; the sidebar groups by local date using the UTC offset the app supplies.

## ADR-0040: Secret variables live in local files (2026-10-17)
**Context.** ADR-0011 introduced `*.local.toml` override files for secrets. Postman marks variables as *secret* to mask them.
**Decision.** A variable marked secret is masked in the UI, and when Ferry saves it, its value goes only to the sibling `.local.toml` (git-ignored); the main file keeps the key with an empty value. Values in the local file override the main file's on load. A teammate who clones the repository sees the variable and fills in their own value.

## ADR-0041: Preview scope and platforms (2026-10-17)
**Context.** A Phase 3 spike (`wry` 0.57 with `build_as_child` through `iced::window::run`) showed a WebView2 child window rendering inside the iced window on Windows; the web view runs in its own Edge processes, so Ferry's process grows little. On Linux, `wry` needs GTK and WebKitGTK, which winit does not run, and building would need the WebKitGTK development packages, against ADR-0022's easy build.
**Decision.** Preview uses an embedded web view on Windows and macOS, created on demand (ADR-0028) and serving the body from a custom protocol so any size and content type works. On Linux, and wherever the web view fails to start, Preview offers `Open in Browser` instead (a temporary file opened with the default application). Visualize depends on `pm.visualizer.set`, so it moves to Phase 4 with scripts; XPath and the XML, YAML, Markdown and Base64 body views move to Phase 7. The JSONPath filter (RFC 9535, `serde_json_path`) ships in Phase 3.
**Consequences.** macOS embedding cannot be tried on the development machine; CI builds it, and FND entries will track anything found on real hardware. The web view adds to the binary; the re-baseline is recorded in ADR-0042.
**Implementation (2026-10-18).** `App::preview()` names the response to show; after every update the `Shell` creates, replaces or destroys the web view to match, and a sensor on the body area moves it when its bounds change. Leaving Preview, its response tab, its workbench tab or its response destroys the web view, and the web view also goes away while a dialog is open, since a native child window would cover it. The page is served from memory through a `ferry` custom protocol (other paths are 404). It cannot navigate away, open windows or download; it has no IPC handler; and scripts are disabled, which the spec does not require but which keeps untrusted response HTML from running at all. WebView2 keeps its profile in the temporary directory, never next to the executable. Checked with `examples/preview_probe.rs` on Windows: HTML, SVG and PDF render; a `<meta refresh>` to another site is refused; the script in the test page does not run; leaving Preview ends the WebView2 processes.

## ADR-0042: Binary-size re-baseline for Phase 3 (2026-10-18)
The Windows release binary grew from 8.95 MB to 12.73 MB (+42%), past the 10% regression guard. (The recorded Phase 2 baseline said 9.37 MB, but that build carried a test-only font; FND-018. All sizes here are `ferry` built on its own, as released.) Measured per step: variables, environments, collections and the workspace watcher +1.24 MB (10.19 MB at 18ef86d); history, cookies, console, search and the JSONPath filter +2.11 MB (12.30 MB at a38a041); the Preview web view (`wry`, WebView2) +0.43 MB. The growth is the Phase 3 scope from ADR-0027, so the Windows baseline is re-recorded (2026-10-19: 12.73 MB; idle RSS with 10 tabs 28.0 MB, 1.24x the empty window; cold start 95 ms; scroll p95 3.21 ms) and the Linux and macOS baselines are cleared and re-recorded from the next CI run, as in ADR-0021 and ADR-0036. The middle step is larger than its code suggests; FND-016 tracks finding out why (the cookie stack is the main suspect) before Phase 4 adds a script engine.

## ADR-0043: License checks cover the release targets; one MPL-2.0 exception (2026-10-19)
**Context.** With wry, `cargo deny` rejected MPL-2.0 crates. Most (`cssparser`, `selectors`, `dom_query`) are wry dependencies for Android only, which Ferry never builds: cargo-deny checked every platform. One, `option-ext` (via wry -> `dirs`), is in the Windows, macOS and Linux binaries.
**Decision.** `deny.toml` checks the four release targets from `release.yml`. `option-ext` is allowed MPL-2.0 as a named exception; MPL-2.0 as a whole stays off the allow list, so any new MPL-2.0 crate is a decision of its own. The unused `CDLA-Permissive-2.0` allowance is removed.
**Consequences.** MPL-2.0 is file-level copyleft: Ferry uses `option-ext` unmodified, and its source stays available on crates.io, so Ferry's own code stays Apache-2.0. A release target added to `release.yml` must be added to `deny.toml` too.

---

## v2 candidates
(none yet)
