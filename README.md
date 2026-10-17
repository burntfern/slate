<img alt="banner" src="./assets/readme/banner.png" />

<p align="center">
<code>Slate</code> brings browser-style tabs to every Windows app. Your open windows become tabs in a <code>single</code> window, and you switch, add, or detach them with <code>Ctrl</code>+<code>Tab</code> and a <code>drag</code>.
</p>

<div align="center">

[![CI](https://img.shields.io/github/actions/workflow/status/burntfern/slate/ci.yml?label=ci&style=flat-square)](https://github.com/burntfern/slate/actions/workflows/ci.yml)
[![Deny](https://img.shields.io/github/actions/workflow/status/burntfern/slate/deny.yml?label=deny&style=flat-square)](https://github.com/burntfern/slate/actions/workflows/deny.yml)
[![Release](https://img.shields.io/github/v/release/burntfern/slate?sort=semver&style=flat-square)](https://github.com/burntfern/slate/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/burntfern/slate/total?style=flat-square)](https://github.com/burntfern/slate/releases)
![Platform](https://img.shields.io/badge/platform-Windows-0078D6?style=flat-square)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg?style=flat-square)](LICENSE)

</div>
<br>

> [!IMPORTANT]
> **Slate is free, local, and open source.** It only rearranges windows that are already on your desktop - no app is ever launched for you, nothing is reparented into another process, and your settings stay in `%USERPROFILE%\.burntfern`.

> [!WARNING]
> **Slate is under active development.** Expect bugs and rough edges, and please [open an issue](https://github.com/burntfern/slate/issues) if something breaks.

> Built with **Rust** on **[gpui](https://github.com/zed-industries/zed)**.

## Table of Contents

- [Features](#features)
- [Gallery](#gallery)
- [Install](#install)
- [Build from source](#build-from-source)
- [Usage](#usage)
- [Files and data](#files-and-data)
- [Limitations](#limitations)
- [License](#license)

## Features

- **`Any window as a tab`** - add one from the `+` picker, Ctrl-drag a window onto the host, or hover it in pick mode and press `TAB`
- **`Drag out to detach`** - drag a tab off the strip to send its window back to the desktop at the drop point, across monitors
- **`Per-tab accents`** - color-code tabs by hue from the tab menu (session only)
- **`Saved sets`** - snapshot the open tabs as a named set and restore it later; restoring only re-hosts windows that are already running
- **`Hover preview`** - rest on a tab for a live snapshot of its window
- **`Global shortcuts`** - `Ctrl`+`Tab`, `Ctrl`+`Shift`+`Tab`, `Ctrl`+`1..9` and `Ctrl`+`W` keep working while a hosted window has focus
- **`Tray icon`** - restore the host or exit from the notification area
- **`Light and dark themes`** - following the system by default

## Gallery

<table>
  <tr>
    <td colspan="2" align="center"><code><b>all your windows in one window</b></code></td>
  </tr>
  <tr>
    <td colspan="2">
      <img width="100%" src="assets/readme/theme-split.png" alt="Slate hosting three windows as tabs, dark theme on the left and light theme on the right" />
    </td>
  </tr>
  <tr>
    <td width="50%">
      <img width="100%" src="assets/readme/desktop-dark.png" alt="The full desktop with Slate in the dark theme" />
    </td>
    <td width="50%">
      <img width="100%" src="assets/readme/desktop-light.png" alt="The full desktop with Slate in the light theme" />
    </td>
  </tr>
  <tr>
    <td colspan="2" align="center"><code><b>tab strip</b></code></td>
  </tr>
  <tr>
    <td colspan="2">
      <img width="100%" src="assets/readme/tab-strip-dark.png" alt="The Slate tab strip in the dark theme" />
    </td>
  </tr>
  <tr>
    <td colspan="2">
      <img width="100%" src="assets/readme/tab-strip-light.png" alt="The Slate tab strip in the light theme" />
    </td>
  </tr>
  <tr>
    <td colspan="2" align="center"><code><b>overflow</b></code></td>
  </tr>
  <tr>
    <td colspan="2">
      <img width="100%" src="assets/readme/overflow-dark.png" alt="The overflow row in the dark theme" />
    </td>
  </tr>
  <tr>
    <td colspan="2">
      <img width="100%" src="assets/readme/overflow-light.png" alt="The overflow row in the light theme" />
    </td>
  </tr>
  <tr>
    <td colspan="2" align="center"><code><b>hosted apps</b></code></td>
  </tr>
  <tr>
    <td width="50%">
      <img width="100%" src="assets/readme/hosted-apps-dark.png" alt="Windows hosted as tabs in the dark theme, each keeping its own title bar" />
    </td>
    <td width="50%">
      <img width="100%" src="assets/readme/hosted-apps-light.png" alt="Windows hosted as tabs in the light theme" />
    </td>
  </tr>
  <tr>
    <td colspan="2" align="center"><code><b>pick mode</b></code></td>
  </tr>
  <tr>
    <td width="50%">
      <img width="100%" src="assets/readme/pick-mode-dark.png" alt="Pick mode highlighting candidate windows in the dark theme" />
    </td>
    <td width="50%">
      <img width="100%" src="assets/readme/pick-mode-light.png" alt="Pick mode highlighting candidate windows in the light theme" />
    </td>
  </tr>
  <tr>
    <td colspan="2" align="center"><code><b>saved sets</b></code></td>
  </tr>
  <tr>
    <td width="50%">
      <img width="100%" src="assets/readme/saved-sets-dark.png" alt="The saved sets menu in the dark theme" />
    </td>
    <td width="50%">
      <img width="100%" src="assets/readme/saved-sets-light.png" alt="The saved sets menu in the light theme" />
    </td>
  </tr>
  <tr>
    <td colspan="2" align="center"><code><b>game as a tab (without title bar)</b></code></td>
  </tr>
  <tr>
    <td colspan="2">
      <img width="100%" src="assets/readme/game-tab.png" alt="A game running as a Slate tab" />
    </td>
  </tr>
</table>

## Install

Pick the asset that matches your CPU from the
[latest release](https://github.com/burntfern/slate/releases/latest) and run it.
Swap `X.Y.Z` for the version shown there. There is no installer and nothing
else to install.

| Asset                                       | For                                              |
| ------------------------------------------- | ------------------------------------------------ |
| `slate-vX.Y.Z-x64.exe`                      | Intel / AMD 64-bit PCs (most machines)           |
| `slate-vX.Y.Z-arm64.exe`                    | ARM64 Windows (Surface, Snapdragon laptops)      |

Releases are unsigned, so SmartScreen may warn on the first launch. To check a
download, compare it against the published `SHA256SUMS`:

```powershell
Get-FileHash .\slate-vX.Y.Z-x64.exe -Algorithm SHA256  # match the file you downloaded
```

## Build from source

You need Rust with the MSVC target and the Windows SDK. The exact toolchain
and target list are pinned in `rust-toolchain.toml`, which rustup applies
automatically (the `aarch64` target there is only for the ARM64 release build
in CI; local `cargo build --release` produces the x64 exe).

```powershell
cargo build --release
```

> [!IMPORTANT]
> The full dependency tree is memory hungry to compile. If cargo fails with `os error 1455` (a paging or commit-limit failure), rerun it with `-j 1` to build one crate at a time.

The binary lands in `target\release\slate.exe`. For the development workflow,
see [CONTRIBUTING.md](CONTRIBUTING.md).

## Usage

| Shortcut                         | Action                                        |
| -------------------------------- | --------------------------------------------- |
| `Ctrl+Tab` / `Ctrl+Shift+Tab`    | Next / previous tab                           |
| `Ctrl+1` to `Ctrl+9`             | Jump to the Nth tab                           |
| `Ctrl+W`                         | Return the active tab's window to the desktop |
| Ctrl-drag a window onto the host | Add it as a tab                               |
| Hover a window, then `TAB`       | Add it as a tab (pick mode)                   |
| `Esc`                            | Cancel pick mode or dismiss a menu            |

## Files and data

Slate stores everything under `%USERPROFILE%\.burntfern`:

- `config.json` - theme and accent
- `slate\settings.json` - tab, preview, and shortcut settings
- `slate\sets.json` - saved tab sets
- `slate\backups\` - timestamped config backups

On first run each file is seeded from the `configs\` folder that ships next to
the executable (or the working directory in a dev checkout). Deleting these
files resets Slate to defaults.

## Limitations

**`1`** **Windows only.** Slate relies on Win32 window ownership, DWM frame
control, and GDI icon extraction.

**`2`** **Owned windows draw above the host.** While a Slate menu or dialog is
open, it covers the active member. Tooltips and toasts that overlap a hosted
window have the same problem.

**`3`** **No launching, no elevation.** Slate hosts windows that are already
running. It skips elevated windows unless Slate itself is elevated.

## License

GNU General Public License v3.0. See [LICENSE](LICENSE).

Copyright (C) 2026 burntfern
