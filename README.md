# Ferry

[![CI](https://github.com/KabirRao1/ferry/actions/workflows/ci.yml/badge.svg)](https://github.com/KabirRao1/ferry/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

**A native, low-memory, offline HTTP API client.** A lean alternative to Postman, written in Rust.

![Ferry showing a JSON response](docs/images/screenshot.png)

- **Native UI.** No Electron; a web view is only created for HTML previews. Idles at about 24 MB of RAM and opens in under 100 ms.
- **Handles huge responses.** A 50 MB body scrolls smoothly; only the visible lines are ever rendered.
- **Offline and private.** No account, no sign-in, no telemetry.
- **Plain-file collections** *(coming in Phase 3)*: requests saved as TOML files in a folder, so you can keep them in git and review them like code.

> **Status: early development.** Ferry sends requests with params, headers, auth and bodies, in tabs that survive restarts.
> Collections, environments, scripts and more protocols are on the [roadmap](#roadmap).

## Quick start

### 1. Install Rust

Install [rustup](https://rustup.rs). Ferry pins its Rust version in `rust-toolchain.toml`, and rustup downloads it automatically the first time you build.

### 2. Install platform build tools

| OS | What you need |
|---|---|
| **Windows** | [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the **Desktop development with C++** workload. rustup's installer offers to install this for you. |
| **macOS** | Xcode command line tools: `xcode-select --install` |
| **Linux (Debian/Ubuntu)** | `sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev` |
| **Linux (Fedora)** | `sudo dnf install gcc pkg-config libxkbcommon-devel wayland-devel` |

### 3. Build and run

```sh
git clone https://github.com/KabirRao1/ferry.git
cd ferry
cargo run --release
```

The first build takes a few minutes. Later builds are incremental.

To install the `ferry` command on your `PATH` instead:

```sh
cargo install --git https://github.com/KabirRao1/ferry ferry-ui
```

## Using Ferry

The window is laid out like Postman: a sidebar on the left, request tabs across the top, the request builder above the response.

1. Pick a method (GET, POST, …) or type your own, such as `PROPFIND`.
2. Type a URL, for example `https://httpbin.org/json`. Query parameters show up in the **Params** table, and editing the table updates the URL.
3. Add what the request needs:
   - **Authorization**: Basic Auth, Bearer Token or API Key.
   - **Headers**: your own rows, plus the headers Ferry adds (click "hidden" to see them).
   - **Body**: form-data (text and files), x-www-form-urlencoded, raw (Text, JSON with validation and Beautify, XML, HTML, JavaScript) or a binary file.
   - **Settings**: turn off following redirects or certificate verification.
4. Press **Send**, **Enter** in the URL field, or **Ctrl+Enter** (**⌘+Enter** on macOS).
5. Read the status, time (hover for the DNS / connect / waiting / download breakdown) and size. JSON is pretty-printed; switch to **Raw** or **Hex** at any time.

Tables have **Bulk Edit** for pasting `key: value` lines. Open tabs, including unsent requests, are restored when you start Ferry again.

| Shortcut | Action |
|---|---|
| Ctrl+Enter | Send |
| Ctrl+T / Ctrl+W | New tab / close tab |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab |
| Ctrl+B | Show or hide the sidebar |

On macOS, use ⌘ instead of Ctrl (Ctrl+Tab stays Ctrl).

## Roadmap

Ferry is heading for Postman's layout and every Postman feature that runs on your machine: no account, no cloud ([parity map](docs/POSTMAN_PARITY.md)).

| Phase | Features | Status |
|---|---|---|
| 0 | Foundations: CI, test harness, performance budgets | ✅ done |
| 1 | Send a request, see status, headers and body | ✅ done |
| 2 | Postman-style workbench, query params, headers, bodies, auth, tabs, redirect/TLS options, timing breakdown | ✅ done |
| 3 | Variables and environments, collections, history, cookies, console, saved examples, HTML preview | next |
| 4 | Pre-request and test scripts (`pm` API), Collection Runner, CLI | planned |
| 5 | Import/export (Postman, cURL, OpenAPI, HAR), code snippets, OAuth and other auth, proxy and certificates, vault | planned |
| 6 | GraphQL, WebSocket, Socket.IO, server-sent events, gRPC, MQTT | planned |
| 7 | Local mock server, docs generation, find and replace, settings, keyboard, releases | planned |

## Contributing

Contributions are welcome. See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the development setup, the test-first workflow and the merge gate.

| Document | Contents |
|---|---|
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | How the crates fit together |
| [`docs/SPEC.md`](docs/SPEC.md) | Features and acceptance criteria |
| [`docs/DECISIONS.md`](docs/DECISIONS.md) | Architecture decision records |
| [`docs/FEATURES.md`](docs/FEATURES.md) | Feature status and test counts |
| [`docs/FINDINGS.md`](docs/FINDINGS.md) | QA findings and their regression tests |
| [`CHANGELOG.md`](CHANGELOG.md) | Notable changes per release |

## Security

Please report vulnerabilities privately. See [`SECURITY.md`](SECURITY.md).

## License

Licensed under the Apache License, Version 2.0 ([`LICENSE`](LICENSE)).

Unless you explicitly state otherwise, any contribution you intentionally submit for inclusion in this project, as defined in the Apache-2.0 license, is licensed as above, without any additional terms or conditions.
