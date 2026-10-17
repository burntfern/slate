# Slate — codebase review fixes

Working tracker for the full codebase review. Untracked on purpose (not committed).
Numbering matches the review report. `[x]` = fixed, `[~]` = partially / deferred with note.

## A. High

- [x] 1. Release assets may never attach — added `RELEASE_PLEASE_TOKEN` PAT support to `release-please.yml` (`|| secrets.GITHUB_TOKEN` fallback) + comments; `workflow_dispatch` fallback kept. **Manual:** create the PAT secret.
- [x] 2. Janitor never restores after a crash — `process_alive` now treats `ERROR_INVALID_PARAMETER` (low word 87) as dead, other errors as alive.
- [x] 3. `settings.json` shadows shared `config.json` — app file holds only `schema_version`/`tabs`/`updates`; theme/accent read+written only in shared `config.json` (`save_theme`). Seed rewritten.
- [x] 4. One bad value nuked all settings — per-key read from `serde_json::Value` (`read_partial`); wrong-typed keys ignored, rest kept.
- [x] 5. Cross-thread positioning can freeze the app — worker `apply` passes `IsHungAppWindow` so a hung member is placed with `SWP_ASYNCWINDOWPOS`.
- [x] 6. Release size — CI-only: `CARGO_PROFILE_RELEASE_LTO=thin`, `CODEGEN_UNITS=1`, `STRIP=debuginfo` in `release.yml`. Local `--release` stays unstripped (per decision).

## B. Medium

- [x] 7. Group leak / dangling active group — `remove_empty`, `reconcile_active_group`, and `add_window` validates the group before allocating a `TabId`.
- [x] 8. `close_tab` desync — `close_tab` now only closes the window; the `Destroyed` event owns teardown.
- [x] 9. UI stall on hover — `show_hover_preview` captures on the background executor and renders the frame back on the main thread (cached image shown first).
- [x] 10. Update fetch off the UI thread — already ran on `background_executor`; no change needed.
- [x] 11. Cooldown overflow — `now.saturating_sub(checked_at)`.
- [x] 12. Silent write failures — `save`/`save_sets`/`save_theme`/`atomic_write` return `io::Result`; callers handle/ignore explicitly.
- [x] 13. No schema gate — `read_partial` rejects `schema_version` newer than `SCHEMA_VERSION` (backs the file up).
- [x] 14. Overlay not clamped vertically — `overlay_layer` takes an estimated height, clamps `top`, caps height and scrolls; hover card flips above near the viewport bottom.
- [x] 15. Menu width mismatch — tab menu overlay now uses the real `260.0` width (was `220.0`).
- [~] 16. App-menu state tracked twice — `OverflowMenu` owns its open state and exposes no prop/callback, so `info.overlay` mirrors it (trigger toggles both, actions clear it). Documented; a true single source needs an upstream change.
- [x] 17. Dismiss ran full dispatch on every click — `dismiss`/`escape` now early-return when nothing is open and dispatch only the open overlays.
- [x] 18. Hover preview swapped body to empty state — `hover_preview.is_some()` removed from the body `overlaying` set.
- [x] 19. `sync_worker` clones / `MoveStarted` re-enumerates — `MoveStarted` only re-enumerates when the window is unknown. (Styling clone is cheap; left as is.)
- [x] 20. `invalidate_info()` every 50 ms — `tick` invalidates only when state changed; `dispatch` already invalidates up front.
- [x] 21. Picker O(n²) + per-compare allocations — `HashSet` for hosted lookup, `sort_by_cached_key` for the lowercase sort.
- [x] 22. Original window owner lost — `StylingState.owner` recorded on register and restored on unregister (wire format bumped to MAGIC `…4A32`).
- [x] 23. `GetDIBits` while bitmap selected — deselect before `GetDIBits` in `capture.rs` and `icon/decode.rs`.
- [x] 24. Thread-local vs global queue — `ForegroundChanged` variant + hook removed (dead); `QUEUE` stays `thread_local!` (hook delivers on the installing thread).
- [x] 25. `HostWorker` `Clone`+`Drop` — stop happens in a shared `Inner`'s `Drop`, so only the last handle stops the worker.
- [x] 26. Capture pool / `GetDC` — `GetDC`, `CreateCompatibleDC` and `CreateCompatibleBitmap` handles now null-checked. (Pool still keyed by size.)

## C. Low

- [x] 27. Truncated titles/class — title sized via `GetWindowTextLengthW`; class buffer grows to 4096. `MAX_OWNED` raised 64 → 1024.
- [x] 28. GDI/handle checks + tray — region handles + `CombineRgn` checked in `highlight.rs`; tray window is message-only (`HWND_MESSAGE`); tray menu posts `WM_NULL` to dismiss.
- [x] 29. `open_url` — single long-lived worker thread with a channel; result ignored intentionally (comment).
- [x] 30. Overflow/collision corners — `u32::try_from`/saturating ids, unique millis+counter backup names.
- [x] 31. `home_dir()` fallback — `USERPROFILE` → `HOMEDRIVE`+`HOMEPATH` → temp.
- [x] 32. `build.rs` — resolved once; `.ico`/`.rc` written under `OUT_DIR` (no source-tree writes); errors surfaced with `panic!`.
- [x] 33. `main.rs` `expect` — startup failures `eprintln!` then `std::process::exit(1)`.
- [x] 34. Dead `logs_dir()` — removed; README claim dropped.
- [~] 35. UI refactors — safe slice done: `overlays.rs`/`panels.rs`/`title_bar.rs` magic px → named consts, panel width shared between panel and overlay, per-frame `format!` for caption/version/update label/saved-set detail hoisted into `HostInfo`/`SavedSetInfo` (SharedString). Remaining (oversized render fns, duplicated dropdown chrome, tab element-id `format!`) deferred — needs visual verification.
- [~] 36. A11y / central string table — deferred; needs visual verification.
- [x] 37. `index()` borrow convention — replaced with `ensure_index()` + short-lived borrows in `hwnd_of_tab`/`tab_of_hwnd`; no `Ref::map`, no `expect`, no call-site convention.

## D. CI / workflows

- [x] 38. `--locked` added to clippy/build/test in `ci.yml`.
- [x] 39. See 1 (PAT).
- [x] 40. `concurrency:` added to ci/codeql/release (and the new deny workflow).
- [x] 41. Toolchain — CI channel matches `rust-toolchain.toml` (stable, rustfmt+clippy); comment added.
- [x] 42. `cargo-deny` — `deny.toml` added + `deny.yml` workflow.
- [~] 43. Dependabot groups cargo deps — fine; noted.
- [x] 44. `codeql`/`audit` otherwise good — verified.

## E. Docs / metadata

- [x] 45. README `slate\logs\` claim dropped (backups only).
- [~] 46. Discussions must be enabled in repo settings (manual).
- [x] 47. `CONTRIBUTING.md` commit types now include `build` and `init`.
- [~] 48. `SECURITY.md` "tip of main" — correct once `main` has the code (manual push).
- [x] 49. Verified consistent: LICENSE/NOTICE/Cargo.toml/README copyright; editorconfig/gitattributes.

## F. Missing / nice-to-have

- [x] 50. `deny.toml` + `deny.yml` — verified locally: `advisories ok, bans ok, licenses ok, sources ok` (added `bzip2-1.0.6`, `GPL-3.0-only`; wildcards→warn for the git dep; ignored 3 unmaintained-only advisories, mirrored in `audit.yml`).
- [~] 51. `.github/FUNDING.yml` — skipped by choice (no Sponsor button).
- [~] 52. `CHANGELOG.md` — do not add by hand; release-please creates it.
- [x] 53. README images live under `assets/` (not `/docs`).

## G. Verified clean

- [x] `src/ui/`: no reachable panic paths.
- [x] No `todo!`/`unimplemented!`; no non-ASCII slicing panics.
- [x] Tests exist for `core/update`, `config/schema`, `config/load`, `core/group`, `win/janitor/wire`.

## H. Added after the review

- [x] Ungroup bug — batch ungroup (left/right/others) left windows below the host with no taskbar button. `unregister_member` now raises with `HWND_TOP` and rebuilds the taskbar button synchronously.
- [x] Comment sweep — `src/**` + `build.rs` audited for AI-slop/restating comments; only 1 removed (`panels.rs` badge group label). 37 comment lines remain, all "why" notes (Win32 quirks, magic-number rationale).
- [x] Window-bounds persistence — implemented then removed at user request (behaviour was already fine). Back to the fixed centered `1100x720`.

## I. Follow-up round (in progress)

- [~] 16. App-menu state single source — kept the documented mirror (`OverflowMenu` owns open state; ely exposes no controlled mode). Per decision, left as is.
- [x] 35. UI refactors — shared dropdown chrome via `overlays::menu_panel`, tab animation element-id `format!` -> `("tab-in", ix)`, and every oversized render fn split into named helpers (`panels.rs`: `tab_overflow` 190->~115, `picker` 166->~65, `saved` 233->~80, `tab_menu` 310->~135 plus `overflow_row`/`picker_row`/`saved_set_row`/`menu_item`/`accent_row`; `title_bar.rs`: `build` 360->~65 plus `app_menu`/`menu_trigger`/`tab_strip`/`theme_button`/`window_controls`/`picking_spinner`/`leading_inset`).
- [x] 36. A11y — roles + labels only: `Role::Tab`/`TabList`/`Menu`/`MenuItem`/`Button` and `aria_label`/`aria_selected` on the hand-rolled tabs, menu rows, picker/overflow/saved rows and colour swatches.
- [x] 43. Dependabot — already groups cargo + actions with `patterns: ["*"]`; no change.
- [x] Comment pass — all 11 multi-line `///` / `//` blocks condensed to single lines (37 -> 21 comment lines).
- [x] Icon / preview colour flip — `decode.rs` and `capture.rs` swapped R/B into RGBA before handing pixels to `RenderImage`, which gpui documents and treats as BGRA, so every non-grey colour flipped (blue -> orange); white was unaffected because R=G=B. Both now pass the raw `GetDIBits` BGRA through, field renamed `bgra`.
- [x] Picker staleness — windows opened after Slate's start were missing from the `+` picker until the menu's "Refresh window list", because `set_picker_open` rebuilt rows from the cached window list without re-scanning. It now calls `enumerate_windows()` on open.

## Manual follow-ups (user)

- Push `main` first, then `development`; open PR `development → main` (merge commit).
- Add repo secret `RELEASE_PLEASE_TOKEN` (PAT with `contents`+`pull-requests`) so release-please's tag/release triggers `release.yml`.
- Enable Settings → Actions → "Allow GitHub Actions to create and approve pull requests".
- Enable Discussions (used by `.github/ISSUE_TEMPLATE/config.yml`).
- Visually verify: hover preview, overlay placement/scroll, hover card flip, tray menu.
- Confirm the `deny.toml` license allowlist on the first CI run (may need additions).

## Promo line

- Cargo.toml `description`: "Real tabs for Windows".
- README tagline: "**Real tabs for Windows.** Turn any app's window into a tab, in one window."
