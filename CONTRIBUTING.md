# Contributing

Thanks for helping with Slate. This project is Windows-only and built with the
Rust MSVC toolchain.

## Prerequisites

- Rust stable with the `x86_64-pc-windows-msvc` target.
- The Windows SDK (for `rc.exe`, used to embed the icon and version resource).

## Building

```powershell
cargo build -j 1
```

Use `-j 1`. Parallel builds on Windows can transiently fail with
`os error 1455` ("paging file is too small") or `E0786`. These are not code
errors — retry the command. If a failure persists, read the output: it is
usually a real compile error.

## Before opening a pull request

Run all of these and make sure they pass:

```powershell
cargo fmt --all
cargo clippy -j 1 --all-targets -- -D warnings
cargo build -j 1
cargo test -j 1
```

`cargo clippy --all-targets` must be warning-free. The test suite covers the
pure modules (`core`, `config`); the UI and Win32 layers are verified by running
the app.

## Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

```text
type(scope): lowercase imperative summary
```

Common types: `feat`, `fix`, `refactor`, `perf`, `docs`, `ci`, `chore`. Keep the
subject on one line. Explain *why* in the body only when it is not obvious.

## Branch naming

`<type>/<short-description>`, for example `fix/tab-underflow` or
`refactor/split-controller`.

## Architecture

Data flows one way: **UI → command → controller → Win32 → core state**. The UI
never calls Win32 directly; it dispatches a `Cmd` and re-renders from the
snapshot the controller publishes. See `src/controller/` for the command
handling and `src/win/` for the platform layer.
