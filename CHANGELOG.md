# Changelog

All notable changes to Ferry are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and Ferry uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
