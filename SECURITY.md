# Security Policy

## Supported versions

Slate ships as a portable binary from the
[Releases](https://github.com/divyanshudhruv/slate/releases) page. Only the latest
release is supported. If you build from source, use the tip of `main`.

## Reporting a vulnerability

Please report security issues privately. Do not open a public issue.

Use GitHub's private vulnerability reporting: open the **Security** tab of the
repository and click **Report a vulnerability**.

Include as much of this as you can:

- What the issue is and what an attacker could do with it.
- Steps to reproduce, or a proof of concept.
- The Slate version (the release tag, or the `version` in `Cargo.toml`).
- Your Windows version.

## Scope

Slate is a Win32 desktop app that hosts other windows. Areas of interest:

- Window ownership and message handling (`src/win/`).
- The single-instance janitor and its shared state (`src/win/janitor/`).
- Config and saved-set parsing (`src/config/`).
- The self-update flow and its registered-exe record (`src/win/update.rs`,
  `install.json` in the data directory).
- The tray icon, global shortcuts, and IPC.

Out of scope: anything that needs an already-compromised machine, and the
limitations already listed in the README.

## What to expect

This is a small project maintained in spare time. I aim to acknowledge reports
within a few days, and I will credit reporters in the release notes unless asked
otherwise.
