# Ferry

[![CI](https://github.com/KabirRao1/ferry/actions/workflows/ci.yml/badge.svg)](https://github.com/KabirRao1/ferry/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

**A native, low-memory, offline HTTP API client.** A lean alternative to Postman, written in Rust.

![Ferry showing a JSON response](docs/images/screenshot.png)

- **Native UI.** No Electron; a web view is only created for HTML previews. Idles at about 24 MB of RAM and opens in under 100 ms.
- **Handles huge responses.** A 50 MB body scrolls smoothly; only the visible lines are ever rendered.
- **Offline and private.** No account, no sign-in, no telemetry.
- **Plain-file collections** *(coming in Phase 3)*: requests saved as TOML files in a folder, so you can keep them in git and review them like code.

> **Status: early development.** Ferry can send requests and show responses today.
> Headers, bodies, auth, tabs, collections, scripts and more protocols are on the [roadmap](#roadmap).

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

1. Pick a method (GET, POST, …).
2. Type a URL, for example `https://httpbin.org/json`.
3. Press **Send**, **Enter** in the URL field, or **Ctrl+Enter** (**⌘+Enter** on macOS).
4. Read the status, time and size. Switch between **Body** and **Headers**.

While a request is in flight, **Send** becomes **Cancel**.

## Roadmap

Ferry is heading for Postman's layout and every Postman feature that runs on your machine: no account, no cloud ([parity map](docs/POSTMAN_PARITY.md)).

| Phase | Features | Status |
|---|---|---|
| 0 | Foundations: CI, test harness, performance budgets | ✅ done |
| 1 | Send a request, see status, headers and body | ✅ done |
| 2 | Postman-style workbench, query params, headers, bodies, auth, tabs, redirect/TLS options, timing breakdown | in progress |
| 3 | Variables and environments, collections, history, cookies, console, saved examples, HTML preview | planned |
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
