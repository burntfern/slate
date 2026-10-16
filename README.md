<div align="center">

<img src="assets/slate.png" width="72" alt="Slate" />

# Slate

**Universal window tabs for Windows.** Host any app's window inside one tabbed window.

[![CI](https://github.com/burntfern/slate/actions/workflows/ci.yml/badge.svg)](https://github.com/burntfern/slate/actions/workflows/ci.yml)
[![Release](https://github.com/burntfern/slate/actions/workflows/release-please.yml/badge.svg)](https://github.com/burntfern/slate/actions/workflows/release-please.yml)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)

</div>

Slate takes ordinary top-level windows — browsers, editors, terminals, anything
— and hosts them as tabs in a single window. It does not reparent windows with
`SetParent`; it takes ownership and repositions them to the host's content rect,
so every hosted window keeps its own process, GPU surface, and input handling.
Only the active tab is visible; the rest are left running and hidden.

## Features

- **Host any window as a tab** — add via the `+` picker, Ctrl-drag a window onto
  the host, or hover a window in pick mode and press `TAB`.
- **Drag-out detach** — drag a tab out of the strip to return its window to the
  desktop at the drop position (works across monitors).
- **Per-tab accents** — color-code tabs by hue from the tab context menu
  (session-only).
- **Saved sets** — snapshot the open tabs as a named set and restore it later
  (attach-only: it re-hosts windows that are already running, never launches).
- **Hover preview** — rest on a tab to see a live snapshot of its window.
- **Global shortcuts** — `Ctrl+Tab`, `Ctrl+Shift+Tab`, `Ctrl+1..9`, `Ctrl+W`
  work while a hosted window has focus.
- **Tray icon** — restore the host or exit from the notification area.
- **Light and dark themes**, following the system by default.

## Install

Download `slate-vX.Y.Z-x86_64-pc-windows-msvc.zip` from
[Releases](https://github.com/burntfern/slate/releases), unzip it, and run
`slate.exe`. No installer and no runtime dependencies are required.

Releases are unsigned, so Windows SmartScreen may warn on first launch. Verify
the download against the published `SHA256SUMS` if you want to be sure:

```powershell
Get-FileHash .\slate-vX.Y.Z-x86_64-pc-windows-msvc.zip -Algorithm SHA256
```

## Build from source

Requires the Rust stable toolchain with the MSVC target and the Windows SDK.

```powershell
cargo build --release -j 1
```

The binary is written to `target\release\slate.exe`. See
[CONTRIBUTING.md](CONTRIBUTING.md) for the development workflow.

## Usage

| Shortcut | Action |
| --- | --- |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous tab |
| `Ctrl+1` … `Ctrl+9` | Jump to the Nth tab |
| `Ctrl+W` | Return the active tab's window to the desktop |
| Ctrl-drag a window onto the host | Add it as a tab |
| Hover a window, then `TAB` | Add it as a tab (pick mode) |
| `Esc` | Cancel pick mode or dismiss a menu |

## Limitations

- **Windows only.** Slate uses Win32 window ownership, DWM frame control, and
  GDI icon extraction.
- **Owned windows paint above the host**, so a menu or dialog drawn by Slate
  hides the active member while it is open. The same applies to tooltips and
  toasts that would otherwise overlap a hosted window.
- **No installing, no privilege elevation.** Slate hosts windows that are
  already running and skips elevated ones unless it is itself elevated.

## License

[GPL-3.0](LICENSE).
