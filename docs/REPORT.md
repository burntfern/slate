# Slate Improvement Report

Generated: 2026-10-24  
Repo: `D:\github-desktop\burntfern\slate`  
Version (Cargo): 0.2.0

This report is a diagnosis + action plan. It documents every issue found by reviewing the whole codebase (source, configs, CI, docs), maps them to concrete fixes, and aligns with the Gyotaku-style documentation approach requested. Code changes are not implemented here - this is for review before planning/fixing.

User preferences (from questions):
- Updates: Auto-download + install in-app (Recommended)
- Storage: Split - exe in `%LOCALAPPDATA%\Programs\Burntfern\Slate\`, data in `%LOCALAPPDATA%\Burntfern\Slate\` (Recommended)  
- Migration: Auto-migrate on first run with backup (Recommended)
- Logging: File in data dir with rotation (Recommended)

## 1) File storage / paths (critical)

### Current behavior
- Root via `config::load::burntfern_root()` uses `%USERPROFILE%` (+ fallbacks) -> `.burntfern/` (`src/config/load.rs:31-55`)
- App dir `.burntfern/slate/` via `app_dir()` / `config::slate_dir()` (`src/config/load.rs:53-55`, `src/config/install.rs:7-18`)
- Exe lives in data dir as `slate/slate.exe` (canonical), updated in-place; `slate/slate.exe.old`, `slate/updates/`, `slate/backups/` all under same dir (`README.md:192-204`, `src/config/install.rs:7-46`)
- `install.json` stores absolute path to canonical exe + version + pid (`src/config/schema.rs:73-80`, `src/config/install.rs:19-46`)
- Seeding from `configs/` next to exe or CWD (`src/config/load.rs:73-157`)
- `Cmd::OpenSlateFolder` opens `config::slate_dir()` (`src/controller/dispatch.rs:113-116`)

### Issues
1. **Exe mixed with user data** - binaries next to settings/sets/install.json/backups is non-standard and messy for backup/roaming/uninstall.
2. **Non-standard location** - `.burntfern` under `%USERPROFILE%` is vague vs standard Windows paths.
3. **Hardcoded env assumptions** - only `USERPROFILE`/`HOMEDRIVE+HOMEPATH`, no `%LOCALAPPDATA%`, no portable mode.
4. **Tests assert old layout** - `src/config/install.rs:75-87` asserts `slate` under `.burntfern`.
5. **Migration impossible as-is** - paths are scattered, no migration logic.

### Fixes
- [ ] **Introduce path abstraction**: Create `src/config/paths.rs` with `data_dir()`, `install_dir()`, `cache_dir()`, `config_dir()`, `updates_dir()`, `backups_dir()`. All call sites use these.
- [ ] **Adopt split layout** (per user choice): `install_dir = %LOCALAPPDATA%\Programs\Burntfern\Slate\`, `data_dir = %LOCALAPPDATA%\Burntfern\Slate\`. Store exe in install dir, data (settings.json, sets.json, config.json, install.json, backups/, logs) in data dir.
- [ ] **Portable mode support**: If `SLATE_PORTABLE=1` or `slate.portable` file exists next to exe, use exe dir for both (opt-in).
- [ ] **Auto-migrate from `.burntfern/slate`** (per user choice): On first run with new paths missing but old exists, copy/move files to new locations with backup of old dir, update `install.json` paths, log migration. Never delete old files until verified.
- [ ] **Update all references**: `config/load.rs`, `config/install.rs`, `config/mod.rs`, `controller/dispatch.rs` (OpenSlateFolder), `README.md`, tests.
- [ ] **Keep backwards compat**: Fall back to old paths if new paths missing and migration not done yet.

### Touchpoints
`src/config/{load,install,save,mod,schema}.rs`, `src/controller/{dispatch,update,mod}.rs`, `README.md`, `src/config/install.rs` tests

## 2) Update flow (critical)

### Current behavior
- **Startup classification** (`src/core/install.rs:50-96`, `src/controller/update.rs:12-38`): Dev if in `target/*`; same as canonical + record alive -> `FocusRunning`; different exe is strict release filename (`slate-vX.Y.Z-{x64,arm64}.exe`) + no canonical -> `Bootstrap` (copy, checksum if SHA256SUMS exists, write install.json, relaunch); newer release asset -> `Update` (download via curl, spawn swap helper, update install.json, exit); else `Defer` (launch canonical).
- **In-app check** (`src/controller/update.rs:41-106`, `src/win/update.rs:67-141`, `src/core/update.rs:1-155`): fetches GitHub API, compares versions, stores in settings; if update available opens releases page. Does **not** auto-install from UI.
- **Download/verify/swap** (`src/win/update.rs:107-196`): `curl.exe` downloads to `updates/`, fetches `SHA256SUMS`, verifies with `certutil.exe` if present. Batch helper waits for PID, backs up canonical to `.old`, moves staged over, restores on failure, relaunches, self-deletes. Helper logic in `src/win/update.rs:182-240`.
- **Install record** tracks path/version/pid.

### Issues
1. **Two mechanisms, confusing UX** - manual download triggers classification; "Check for updates" only links to page.
2. **External tools required** - `curl.exe`, `certutil.exe` not guaranteed; harder to control.
3. **Checksum optional** - if SHA256SUMS missing/unfetchable, verification skipped (`src/win/update.rs:131-141,153-157`). ChatGPT explicitly said enforce integrity.
4. **Swap helper fragile** - batch file, long waits, limited error reporting, no post-swap verification.
5. **Exe in data dir** - compounds update complexity.
6. **Version parsing strict/inconsistent** - release filename parsing rejects prerelease (`src/core/install.rs:125-135`) while general parsing allows suffixes (`src/core/update.rs:36-51`).
7. **No in-app progress/retry** - failures show toasts but recovery is manual.

### Fixes (per user choice: auto-download + install in-app)
- [ ] **Implement in-app auto-update**: Add `UpdateAction::DownloadAndInstall` flow in controller/UI. Show progress (checking/downloading/verifying/swapping/restarting). Require user confirmation before install.
- [ ] **Make checksums mandatory**: Fetch SHA256SUMS, require match. If unavailable -> fail with clear error (no silent skip). Support both space+space and space formats.
- [ ] **Replace curl/certutil with Rust**: Use `ureq` (blocking) or `reqwest` for HTTP; implement SHA256 with `sha2`. Remove external process dependencies.
- [ ] **Harden swap mechanism**: Better PID wait (with timeout), verify move success, atomic replace where possible, improved rollback, logging at each step. Consider `MoveFileEx` with `MOVEFILE_REPLACE_EXISTING`/delay options as needed. Keep backup until confirmed running.
- [ ] **Improve classification**: Handle split install/data paths, better dev detection, clearer error messages.
- [ ] **Update docs/UX**: Explain auto-update flow in README, show status in UI.

### Touchpoints
`src/core/{install,update}.rs`, `src/win/update.rs`, `src/controller/update.rs`, `src/controller/dto.rs`, UI components, `Cargo.toml` (add deps)

## 3) Crash recovery / Janitor (complex/fragile)

### Current behavior
- `Janitor` spawned as separate process with `--janitor <state_path> <parent_pid>` (`src/main.rs:22-27`, `src/win/janitor/mod.rs:55-72`)
- State written by controller each sync via `Janitor::persist` encoding `(slate_hwnd, Vec<(hwnd, StylingState)>)` with magic (`src/controller/events.rs:106`, `src/win/janitor/mod.rs:33-49`, `src/win/janitor/wire.rs:1-139`)
- On parent death, janitor waits for PID, reads state, calls `restore()` which checks window validity/ownership and calls `unregister_member` (`src/win/janitor/mod.rs:127-174`)
- Sweep on startup removes stale files for dead PIDs (`src/win/janitor/mod.rs:73-96`)

### Issues
1. **Complexity** - custom binary format, temp files, separate process, cmd spawning.
2. **Observability** - no logging, hard to debug failures.
3. **Robustness** - state corruption not fully handled, ownership checks may miss cases, encoding assumes struct layout.
4. **Platform specifics** - Windows-only, relies on PID tracking.

### Fixes
- [ ] **Add logging** to janitor (file in data dir with rotation).
- [ ] **Harden validation** - stricter decode checks, versioning in wire format, better error handling.
- [ ] **Improve ownership checks** - more defensive, handle races.
- [ ] **Consider alternatives** - evaluate if simpler crash recovery possible, but keep if needed.
- [ ] **Tests** - add more janitor wire tests, document behavior.

### Touchpoints
`src/win/janitor/*`, `src/main.rs`, `src/controller/events.rs`

## 4) Window lifecycle / hosting (architectural gaps)

### Current behavior
- Implicit lifecycle via controller state + events + worker
- Hooks for destroy/namechange/movesize (`src/win/events.rs`)
- Register/unregister in `src/win/host/lifecycle.rs`, worker manages positioning/z-order in `src/win/host/worker.rs`
- Elevation checks block elevated windows unless Slate elevated

### Issues (from ChatGPT + review)
1. **No explicit state machine** - transitions not formalized/testable
2. **HWND reuse** - not explicitly guarded
3. **Event sync races** - hook queue + 50ms polling
4. **Win32 contract underdocumented** - focus/z-order/dialogs/DPI/owned windows
5. **Recovery guarantees vague** - detachment/rollback/shutdown behavior

### Fixes
- [ ] **Document lifecycle** in `docs/architecture.md` (Gyotaku-style) with states/transitions
- [ ] **Add state machine enum** for hosted windows where feasible
- [ ] **Harden event handling** - better reconciliation, stale handle detection
- [ ] **Improve DPI/multi-monitor handling** - test and fix edge cases
- [ ] **Document Win32 contract** comprehensively

### Touchpoints
`src/win/host/*`, `src/win/events.rs`, `src/controller/{events,mod,tabs}.rs`, `docs/`

## 5) Config/persistence (issues)

### Current behavior
- Schema versioning exists (`SCHEMA_VERSION=1`), atomic writes, backups on corrupt reads
- Settings merged from shared + app files with precedence rules
- Saved sets with ID assignment, matching logic

### Issues
1. **Migrations stubbed** - only stamps missing version
2. **Backup retention unbounded** - backups accumulate with timestamp+counter (`src/config/load.rs:323-336`)
3. **Path-dependent** - tied to old layout

### Fixes
- [ ] **Real migration framework** - versioned migrations for Settings/SavedSets
- [ ] **Backup rotation** - limit count/age
- [ ] **Update for new paths** after migration

### Touchpoints
`src/config/{load,save,schema}.rs`

## 6) Logging / observability (missing)

### Current behavior
- `eprintln!` in main only (`src/main.rs:32,51,61`)
- `run.cmd` redirects to `slate.log` in repo dir
- No structured logging

### Issues
1. **Hard to debug** - update failures, janitor issues, hosting problems invisible
2. **run.cmd broken path** - goes up dir, wrong location
3. **No rotation/levels**

### Fixes (per user choice: file in data dir with rotation)
- [ ] **Add logging framework** - e.g. `tracing` + `tracing-subscriber` with file appender, rotation (by size). Write to `data_dir/slate.log`
- [ ] **Log key events** - startup/classification, update steps, attach/detach, errors
- [ ] **Fix run.cmd** or remove/update it
- [ ] **Add log location to UI/docs**

### Touchpoints
`src/main.rs`, `Cargo.toml`, `run.cmd`, new logging module

## 7) Other issues found

### Code/structure
1. **Hardcoded repo** - `src/core/update.rs:1-8` has `REPO_SLUG = "divyanshudhruv/slate"`. Make configurable or derive.
2. **run.cmd broken** - `"%~dp0..\target\debug\slate.exe"` wrong relative path from repo root, pipes to repo-local log.
3. **No error context in places** - some errors swallowed with `let _ = ...`
4. **Controller complexity** - large struct (many fields), could benefit from decomposition but may not be necessary.
5. **Tests limited** - only core+config tested; win/ui need manual testing as noted.
6. **Dependency pinning** - `ely-gpui-component` pinned by git rev (good per AGENTS), `gpui-pre` pinned.

### Security/robustness
7. **Update integrity** - must enforce checksums (see section 2)
8. **Path traversal** - be careful with parsed paths in updates
9. **Janitor security** - state file in temp, decoding needs validation (already has some checks)
10. **Elevation handling** - correctly blocks elevated windows; good

### Docs
11. **README needs major expansion** - Gyotaku-style docs wanted. Add architecture, troubleshooting, file locations (updated), update flow, development, etc.
12. **Missing docs/** - `.gitignore` has `/docs` ignored; need to create comprehensive docs.
13. **AGENTS.md/CONTRIBUTING.md** good but could reference new docs.

### Fixes
- [ ] **Make repo configurable** or document for forks
- [ ] **Fix/remove run.cmd**
- [ ] **Improve error handling/logging**
- [ ] **Create comprehensive docs/** like Gyotaku (architecture.md, updates.md, troubleshooting.md, development.md, etc.)
- [ ] **Update README to be very well defined**
- [ ] **Add SECURITY.md notes if needed**

## 8) Prioritized action plan

### Phase 1: Foundation (immediate)
1. Create `src/config/paths.rs` with new path structure
2. Add logging framework with file output + rotation
3. Fix/update run.cmd or remove
4. Start docs structure (`docs/`)

### Phase 2: Storage migration (high priority)
1. Implement split paths (install vs data)
2. Auto-migrate from `.burntfern` with backup
3. Update all path usages
4. Update tests and README

### Phase 3: Updates (high priority)
1. Replace curl/certutil with Rust HTTP + sha2
2. Make checksums mandatory
3. Implement in-app auto-download/install
4. Harden swap helper

### Phase 4: Hardening & docs (medium)
1. Janitor improvements + logging
2. Lifecycle documentation
3. Comprehensive docs like Gyotaku
4. Schema migrations framework

## 9) Questions for clarification (if needed)
Based on choices made - migration strategy seems clear. Any edge cases for portable mode? Should old `.burntfern` be deleted after successful migration or kept? Keep both for safety - recommend keeping until user confirms or after N days.

## Notes
This report covers all issues found. Ready for review. When you approve, I'll create a detailed implementation plan and start executing.
