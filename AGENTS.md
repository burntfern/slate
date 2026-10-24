Windows-only tab host: hosts real Win32 windows as tabs in one gpui window.
Built with Rust, `gpui`, and `ely_gpui_component`.

## Setup
- Toolchain is pinned in `rust-toolchain.toml` (rustup installs it). `aarch64-pc-windows-msvc` is only used for release builds; use `x86_64` day-to-day.
- Build: `cargo build`
- If you hit `os error 1455` (paging/commit limit), try `cargo build -j 1`.

## Checks before finishing
```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
```
- `cargo test` only runs `src/core` and `src/config`. Check `src/ui` and `src/win` by running `target\debug\slate.exe`. Kill any running `slate.exe` first (it locks the exe).

## Layout
- `src/core/` - pure state (groups, tabs, saved sets, install/update planning) + tests
- `src/config/` - settings/load/save/schema for `%LOCALAPPDATA%\Burntfern\Slate\`, seeded from `configs/`
- `src/controller/` - `Cmd` dispatch and UI snapshot; owns the win layer
- `src/win/` - Win32 (enumeration, hooks, host lifecycle, janitor, tray, icons, capture, self-update)
- `src/ui/` - gpui views (host window, tab bar, overlays, panels)

## Rules
- One-way flow: UI -> `Cmd` -> controller -> Win32 -> core state. UI never calls Win32 directly.
- Don't add deps without asking. Prefer `ely_gpui_component` primitives.
- Don't vendor/patch `ely_gpui_component` (git dep pinned by rev).
- Use `-` not em dashes, `->` not arrows in docs/comments.
- Conventional Commits. Only `feat`, `fix`, or a breaking change cut a release (via release-please on `main`). Version lives in `Cargo.toml`.
- Don't commit unless asked.

## Notes
- Windows only. Uses Win32 window ownership, DWM frame control, GDI icon extraction.
- Elevated windows only hosted if Slate is elevated. Slate never launches apps, only hosts running ones.
- Hosted windows draw above the host, so Slate menus can be covered by the active member.
