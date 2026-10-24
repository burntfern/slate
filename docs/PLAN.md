# Slate Improvement Plan

Generated: 2026-10-24  
Repo: `D:\github-desktop\burntfern\slate`  
Version (Cargo): 0.2.0

Following the commit style from git log (conventional commits like `feat(scope): ...`, `fix(scope): ...`, `refactor(scope): ...`, `chore(scope): ...`), this plan splits the work from the improvement report into 4 commits. 

Once approved, these will be executed in the exact order below.


---


## Commit Plan 

### 1. `chore(infra): add path abstraction, file logging, and Gyotaku-style docs scaffold`

This commit lays the foundation for everything else. No behavior changes to the app itself, just the new utilities and documentation structure. 

#### Changes
| Type | File(s) | Description |
| :-- | :-- | :-- |
| New | `src/config/paths.rs` | Centralized path abstraction. Will expose `data_dir()`, `install_dir()`, `cache_dir()`, `updates_dir()`, and `backups_dir()`. Will support `SLATE_PORTABLE` as well as a fallback to the old layout. |
| New | `src/logging.rs` | Tracing based logging to the data dir with rotation (size based) as requested. Configurable levels. |
| New | `docs/architecture.md`, `docs/updates.md`, `docs/troubleshooting.md`, `docs/development.md`, `docs/file-locations.md` | Gyotaku inspired docs. Will document the layers, Win32 hosting, update flow, crash recovery, and file locations. |
| Modify | `src/config/mod.rs` | Export the new `paths` module. |
| Modify | `src/main.rs` | Initialize logging as early as possible. |
| Modify | `Cargo.toml` | Add `tracing`, `tracing-subscriber`, `tracing-appender`, `thiserror` (if needed) and prep for HTTP/checksum deps for later commits. |
| Modify | `run.cmd` | Fix the broken relative path or replace with a correct one. Will default to the new install/data layout once migrated, but for now will point to the correct debug exe. |
| Modify | `.gitignore` | Ensure `docs/` is not ignored anymore (currently `/docs` is ignored) and add log paths if needed. |
| Modify | `README.md` | Restructure to link to the new docs and add a "Files and data" section that is ready to be updated by commit 2. | 

#### Goal
A scaffolding commit. No functional changes to window hosting, updates, or config loading yet. Everything compiles. 


---


### 2. `refactor(storage): split install/data dirs and auto-migrate from .burntfern`

This is the big storage change. Implements the split layout that was requested, migrates existing installs, and updates every call site. 

#### Changes
| Type | File(s) | Description |
| :-- | :-- | :-- |
| Modify | `src/config/paths.rs` | Implement the split paths. Install dir: `%LOCALAPPDATA%\Programs\Burntfern\Slate\`, Data dir: `%LOCALAPPDATA%\Burntfern\Slate\`. Add env overrides and portable mode. |
| Modify | `src/config/load.rs` | Migrate all path usage to `config::paths`. Add `migrate_from_burntfern()` that auto-migrates on first run with a backup of the old `.burntfern/` directory before moving files. Update `ensure()` to use new directories. |
| Modify | `src/config/install.rs` | Change `slate_dir()`, `canonical_exe_path()`, etc to use the new split (exe from install dir, metadata in data dir). Update all tests to assert the new layout. |
| Modify | `src/config/save.rs` | Write to data dir via the new paths. |
| Modify | `src/controller/dispatch.rs` | `OpenSlateFolder` now opens the data directory (as opposed to mixing exe/data). |
| Modify | `src/controller/update.rs` | Update startup install classification to account for exe living in the install dir and records living in the data dir. |
| Modify | `src/core/install.rs` | Adjust path comparisons to be robust against the new split (and handle migration state). |
| Modify | `README.md` | Update the "Files and data" table to reflect the split layout and document migration. |
| Modify | `docs/file-locations.md` | Fill in the finalized paths. |

#### Goal
Existing users with `.burntfern/` will be auto-migrated with a backup on first launch. Fresh installs will use the new split layout. All code paths will be using the new abstraction. 


---


### 3. `feat(update): in-app auto-download with mandatory checksum verification`

Implements the update flow that was requested. Replaces the external tool dependency with Rust, enforces checksums, and adds an in-app download + install flow. 

#### Changes
| Type | File(s) | Description |
| :-- | :-- | :-- |
| Modify | `Cargo.toml` | Add `ureq` (blocking), `sha2`, `hex` (or just compare bytes). Remove reliance on spawning `curl.exe`/`certutil.exe`. |
| Modify | `src/win/update.rs` | Rewrite download/checksum/swap to use `ureq` and `sha2`. Make SHA256SUMS fetching and verification mandatory. Fail with actionable errors if missing/mismatched. Improve swap helper generation with better logging and error handling. |
| Modify | `src/core/update.rs` | Adjust for new install dir/data dir and better version comparison if needed. |
| Modify | `src/controller/update.rs` | Implement the in-app auto-download/install flow (check -> confirm -> download -> verify -> swap -> restart). Update UI toasts to reflect the new flow. |
| Modify | `src/controller/dto.rs` | Add any new commands/states for the download progress if needed (e.g to drive UI feedback). |
| Modify | `src/ui/*` | Wire up UI feedback for the new in-app update flow (progress toasts/status). |
| Modify | `docs/updates.md` | Document the new flow end to end (threat model, verification, rollback). |

#### Goal
Updates can be done entirely in-app with enforced checksum verification. No more silent skipping of SHA256SUMS, no reliance on external CLI tools. 


---


### 4. `fix(reliability): harden janitor, improve observability, and finalize docs`

Final hardening pass. Focuses on the most fragile parts (crash recovery) and polishes the docs to match the Gyotaku goal. 

#### Changes
| Type | File(s) | Description |
| :-- | :-- | :-- |
| Modify | `src/win/janitor/mod.rs` | Add structured logging, stronger validation on state files, better error handling, and timeouts where applicable. |
| Modify | `src/win/janitor/wire.rs` | Version the wire format, add stricter decode checks, and expand tests for corruption cases. |
| Modify | `src/main.rs` | Ensure janitor process logs to the data dir as well. |
| Modify | `src/win/events.rs`, `src/win/host/*` | Add more defensive logging around hosting/lifecycle events (without spamming). |
| Modify | `docs/architecture.md` | Flesh out the explicit lifecycle, Win32 contract, event flow, and recovery guarantees. |
| Modify | `docs/troubleshooting.md` | Add entries for migration, updates, janitor/crash recovery, elevated windows, and DPI/multi-monitor issues. |
| Modify | `README.md` | Final polish, ensure every claim matches the new behavior. |
| Modify | `AGENTS.md`/`CONTRIBUTING.md` | Update any references to paths/logging to match the new reality. |

#### Goal
Crash recovery is more observable and robust, the docs are comprehensive (Gyotaku-style), and the codebase is in a much better state to continue iterating on. 


--- 

## Notes
- Each commit will be kept small and focused. I'll run `cargo fmt`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, and `cargo build --locked` after each commit as per your AGENTS.md. 
- For the migration (commit 2), I'll be extra careful to never delete the old `.burntfern/` until after a successful migration and log everything to the new log file. 
- For updates (commit 3), checksums will be mandatory. If SHA256SUMS cannot be fetched or matched, the update will fail with a clear error.
