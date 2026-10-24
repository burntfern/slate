# Contributing

Thanks for helping with Slate. This project is Windows-only and built with the
Rust MSVC toolchain.

## Prerequisites

- The toolchain pinned in `rust-toolchain.toml` (rustup installs it automatically).
- The Windows SDK (for `rc.exe`, used to embed the icon and version resource).

## Building

```powershell
cargo build
```

> [!IMPORTANT]
> The full dependency tree is memory hungry to compile. If cargo fails with `os error 1455` (a paging or commit-limit failure), rerun it with `-j 1` to build one crate at a time.

## Before opening a pull request

Run all of these and make sure they pass:

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked
cargo test --locked
```

These mirror `.github/workflows/ci.yml` exactly, so a locally clean run should be
a green PR. `cargo clippy --locked --all-targets` must be warning-free. The test
suite covers the pure modules (`core`, `config`); the UI and Win32 layers are
verified by running the app.

## Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

```text
type(scope): lowercase imperative summary
```

Common types: `feat`, `fix`, `refactor`, `perf`, `docs`, `build`, `ci`, `chore`,
and `init` (reserved for the first release commit). Keep the subject on one line.
Explain *why* in the body only when it is not obvious.

## Branch naming

`<type>/<short-description>`, for example `fix/tab-underflow` or
`refactor/split-controller`.

## Which branch to target

`main` is the release branch: it moves only through release-please and the
occasional `development` -> `main` PR, and it is protected. Open your PR
against **`development`**, and branch from the tip of `development`, not from
`main`.

Fork PRs are welcome. The same CI runs on them (no workflow step needs a
secret), they restore the cache seeded from `main`, and GitHub does not let
fork PRs write caches - that is expected, not a failure.

## Architecture

Data flows one way: **UI -> command -> controller -> Win32 -> core state**. The UI
never calls Win32 directly; it dispatches a `Cmd` and re-renders from the
snapshot the controller publishes. See `src/controller/` for the command
handling and `src/win/` for the platform layer.

## File layout

- `src/core/` - pure state (groups, tabs, saved sets, install/update planning)
- `src/config/` - load/save/schema and path abstraction
- `src/controller/` - `Cmd` dispatch and snapshot; owns Win32 integration
- `src/win/` - Win32 enumeration, hosting, hooks, tray, icons, capture, updates, janitor
- `src/ui/` - gpui views (host window, tab bar, overlays, panels)
