# Security policy

## Supported versions

Ferry is in early development. Security fixes go into the latest commit on `main` and the most recent release.

## Reporting a vulnerability

**Please do not open a public issue for security problems.**

Report vulnerabilities privately through GitHub: open the repository's **Security** tab and choose **Report a vulnerability**. Include:

- what the problem is and its impact,
- steps to reproduce, or a proof of concept,
- the Ferry version or commit and your operating system.

You should get an acknowledgement within a few days. Once a fix is ready, a release and an advisory will credit you unless you'd rather stay anonymous.

## Scope

Ferry sends requests the user builds, to servers the user chooses. Examples of issues in scope:

- TLS certificate checks that pass when they should fail.
- Crashes or memory exhaustion caused by a malicious server response.
- Saved collection or environment files leaking secrets beyond what the user configured.
