# Try Ferry with real API keys

This walk-through uses [`docs/demo/API Keys Tour.postman_collection.json`](demo/API%20Keys%20Tour.postman_collection.json), a Postman collection of seven requests against real APIs. It goes from APIs that need no account up to one that bills you, and covers each way an API can take a credential:

| Folder | API | Credential | How it is sent | What you need |
|---|---|---|---|---|
| 1 · No signup | httpbin.org | made-up token and password | Bearer header, Basic auth, `X-Api-Key` header | nothing |
| 2 · Free key | NASA APOD | `nasaKey` | `?api_key=` query parameter | nothing (`DEMO_KEY`), or a free key from [api.nasa.gov](https://api.nasa.gov) (arrives by email in a minute) |
| 3 · Your account | GitHub REST API | `githubToken` | `Authorization: Bearer` | a [fine-grained personal access token](https://github.com/settings/personal-access-tokens) (read-only access is enough) |
| 4 · Paid LLM API | Anthropic Messages API | `anthropicKey` | `x-api-key` header | an API key from [console.anthropic.com](https://console.anthropic.com); one request costs well under a cent |

A request whose key is empty still runs, but its tests are **skipped** rather than failed, so the run passes with whatever keys you have. Fill in more keys and more tests run.

> NASA's shared `DEMO_KEY` is limited per IP address and is often busy (HTTP 429). The tour skips that test when it is; a free personal key fixes it.

## In the app

1. Press **Import** at the top of the window and choose `docs/demo/API Keys Tour.postman_collection.json`. Import `docs/demo/API Keys.postman_environment.json` the same way.
2. Pick **API Keys** in the environment menu at the top right, open it in the sidebar's **Environments** section, and paste your keys into `githubToken`, `anthropicKey` and (optionally) `nasaKey`. They are marked **Secret**, so they are saved in `API Keys.local.toml`, which the workspace's `.gitignore` keeps out of git.
3. Open any request and press **Ctrl+Enter**. The **Test Results** tab shows what passed; the **Console** (footer) shows the request exactly as sent, so you can check that the key went where it should. *Ask Claude* prints Claude's answer there.
4. To run them all, open the collection's `…` menu and choose **Run**.

## From the command line

`ferry-cli` runs the Postman files directly; there is nothing to import. Keys go in with `--env-var name=value`, which overrides the environment file, so they never have to be written to disk.

**macOS / Linux**

```sh
cd docs/demo
ferry-cli run "API Keys Tour.postman_collection.json" \
  --environment "API Keys.postman_environment.json" \
  --env-var "githubToken=$GITHUB_TOKEN" \
  --env-var "anthropicKey=$ANTHROPIC_API_KEY" \
  --env-var "nasaKey=$NASA_API_KEY"
```

**Windows (PowerShell)**

```powershell
cd docs\demo
ferry-cli run "API Keys Tour.postman_collection.json" `
  --environment "API Keys.postman_environment.json" `
  --env-var "githubToken=$env:GITHUB_TOKEN" `
  --env-var "anthropicKey=$env:ANTHROPIC_API_KEY"
```

If you use the GitHub CLI, `--env-var "githubToken=$(gh auth token)"` uses the token you are already logged in with.

With no keys at all, the output looks like this:

```
✓ GET Bearer token 200 · 175 ms
✓ GET Basic auth 200 · 29 ms
✓ GET API key in a header 200 · 31 ms
✓ GET Astronomy Picture of the Day 200 · 412 ms
✓ GET Who am I 401 · 184 ms
✓ GET My latest repositories 401 · 102 ms
✓ POST Ask Claude 401 · 131 ms

7 requests in 1 iteration: 0 failed. Tests: 8 passed, 0 failed, 3 skipped. 1.08 s
```

The 401s are the APIs refusing an empty key; their tests are skipped. With a GitHub token the two GitHub requests return 200 and their tests pass. Add `--report report.json` to save a JSON report; exit code 0 means everything passed, 1 means a request or test failed.

To see a failure, give a wrong key: `--env-var githubToken=nope` makes the GitHub tests fail with `expected response to have status code 200 but got 401`, and the exit code is 1.

## In CI

Store the keys as CI secrets and pass them with `--env-var`. A GitHub Actions job:

```yaml
- name: API tests
  run: |
    ferry-cli run "tests/My API.postman_collection.json" \
      --environment tests/staging.postman_environment.json \
      --env-var "apiKey=${{ secrets.API_KEY }}" \
      --report ferry-report.json
```

## Keeping keys safe

- Secret variables are written only to `*.local.toml` files, which Ferry adds to the workspace's `.gitignore`, and are left out of **Export…**.
- `--env-var` values are never written to disk.
- A key sent as a **query parameter** (like NASA's `api_key`) is part of the URL, so it appears in the request's URL in History, the Console and `--report` files. Prefer header-based keys for anything sensitive, and use read-only, short-lived tokens for testing.
