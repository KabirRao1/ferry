# Changelog

All notable changes to Ferry are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and Ferry uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Import: Postman collections (v2.0 and v2.1) and environments, from a file or pasted text, with a summary of anything not carried over; pasting a cURL command into the URL field or the Import dialog turns it into a request.
- Export: collections and environments as Postman v2.1 files, without secret values.
- Code panel: the current request as code in 12 languages (cURL, HTTPie, Python, JavaScript, Node.js, Go, Rust, Java, C#, PHP, Ruby, Swift), with Copy.
- Scripts: pre-request and post-response JavaScript on requests, folders and collections, with Postman's `pm` API (`pm.test`, `pm.expect`, `pm.response`, `pm.request`, `pm.environment`, `pm.variables`, `pm.sendRequest` and more), run in a QuickJS sandbox with time and memory limits.
- Test Results tab with passed, failed and skipped tests and filters; script `console` output in the Console.
- Collection Runner: run a collection or folder with iterations, a delay and CSV or JSON data files, follow results live, stop, and export a JSON report.
- `ferry-cli run` for running collections in CI, with exit codes and a JSON report.
- Visualize: render a Handlebars template from `pm.visualizer.set` in the response pane, with scripts allowed and the network blocked.

- Workspace folder: collections, folders, requests, environments and globals saved as plain TOML files you can keep in git, with outside changes picked up live and `Open Workspace…` to switch folders.
- Collections sidebar: create, rename, duplicate, delete and drag to reorder or move; collection and folder settings (description, auth, variables); search across saved requests.
- Variables: `{{name}}` in the URL, params, headers, body and auth, from the selected environment, the collection and globals, with nesting, dynamic variables such as `{{$guid}}` and `{{$timestamp}}`, and a strip showing where each one comes from. Secret values stay in `*.local.toml` files that git ignores.
- Auth inherited from the collection or folder.
- Tabs remember the saved request they came from; Ctrl+S saves.
- History: the last 100 sends with their responses, grouped by day, searchable, one click to reopen.
- Cookie jar: cookies kept between requests (including redirects) as RFC 6265 says, a Cookies tab on each response, a cookie manager, and a per-request setting to turn them off.
- Console: every request as sent, with its headers and body, and how it ended; filter, errors only, expandable entries.
- Search in the response body (Ctrl+F), with highlights, `<i> of <n>` and Enter / Shift+Enter to step.
- JSONPath filter (RFC 9535) on the Pretty view of JSON responses up to 10 MB.
- Preview of HTML, image, audio, video and PDF responses in an embedded web view on Windows and macOS (scripts off, no navigation away), and Open in Browser everywhere.
- Saved examples: save a response as an example of its request, and edit or rename examples from the sidebar.
- Postman-style workbench: header, sidebar (Collections, Environments, History), tab strip, request builder, response pane and footer, with a resizable request/response split.
- Tabs: open, close (with a confirmation for unsaved changes), drag to reorder, dirty dot, and restore on restart.
- Query params table kept in sync with the URL, with descriptions and enable/disable per row.
- Custom HTTP methods typed into the method field.
- Headers table with the auto-generated headers listed, and bulk edit for every table.
- Request bodies: form-data with files, x-www-form-urlencoded, raw (Text, JSON, XML, HTML, JavaScript) and binary; JSON validation and Beautify.
- Authorization: Basic Auth, Bearer Token and API Key (header or query).
- Per-request settings: follow redirects, verify SSL certificates.
- Response time breakdown (DNS, connect, waiting, download), Pretty JSON, Raw and Hex views.
- Request bar: method picker (GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS), URL field, Send and Cancel.
- Send with the Send button, Enter in the URL field, or Ctrl+Enter (⌘+Enter on macOS).
- Response view: status (colored by class), total time, body size, and **Body** and **Headers** tabs.
- Virtualized body viewer that scrolls 50 MB responses smoothly.
- Specific error messages for invalid URLs, DNS failures, refused connections, TLS handshake failures and timeouts.
- gzip, brotli, deflate and zstd response decoding.
- Release workflow that builds binaries for Windows, macOS and Linux from version tags.

### Infrastructure

- Cargo workspace with six crates and enforced dependency direction.
- Merge gate: fmt, clippy, tests, coverage floors, cargo-deny, performance budgets, architecture check and mutation testing.
- CI on Linux, macOS and Windows, plus nightly fuzzing and full mutation runs.
- The binary-size budget measures `ferry` as it is released, without features that only the tests use.
