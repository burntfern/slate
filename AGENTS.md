# AGENTS.md

Windows-only tab host: hosts real Win32 windows as tabs inside one gpui window.
Rust + `gpui` + `ely_gpui_component`.

## Setup

- The toolchain pinned in `rust-toolchain.toml` (rustup installs it on
  demand) and the Windows SDK. That file also lists `aarch64-pc-windows-msvc`,
  which only `release.yml` builds; day-to-day work is `x86_64`.
- Build: `cargo build`
- If cargo fails with `os error 1455` (paging/commit limit), add `-j 1`.

## Checks to run before finishing

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
```

`cargo test` covers only `src/core` and `src/config`. The UI (`src/ui`) and Win32
(`src/win`) layers are verified by running `target\debug\slate.exe`. Kill any
running `slate.exe` before rebuilding - it holds the exe open.

## Layout

- `src/core/` - pure state (groups, tabs, saved sets) with its tests
- `src/config/` - load/save/schema for `%USERPROFILE%\.burntfern`, seeded from `configs/`
- `src/controller/` - `Cmd` dispatch and the snapshot the UI renders; owns the win layer
- `src/win/` - Win32: enumeration, hooks, host lifecycle, janitor, tray, icons, capture
- `src/ui/` - gpui views: host window, tab bar, overlays and panels

## Rules

- Data flows one way: UI -> `Cmd` -> controller -> Win32 -> core state. The UI
  never calls Win32 directly; it renders from the controller snapshot.
- No new dependencies without asking. Prefer `ely_gpui_component` primitives over
  hand-rolled UI.
- Never vendor or patch `ely_gpui_component`; it is a git dependency pinned by rev.
- Docs and comments use `-`, not em dashes, and `->`, not arrows.
- Conventional Commits. Only `feat`, `fix` or a breaking change cuts a release
  (release-please runs on `main`); the version lives in `Cargo.toml`.
- Do not commit unless asked.

## Platform notes

- Windows only: Win32 window ownership, DWM frame control, GDI icon extraction.
- Elevated windows are skipped unless Slate itself is elevated, and Slate never
  launches apps - it only hosts windows that are already running.
- Owned windows draw above the host, so Slate's own menus can be covered by the
  active member.
