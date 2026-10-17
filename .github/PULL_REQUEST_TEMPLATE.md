## What

<!-- One or two sentences on what this changes. -->

## Why

<!-- The problem it fixes or the reason for the change. Skip if obvious. -->

## Checklist

- [ ] `cargo fmt --all -- --check` is clean
- [ ] `cargo clippy --locked --all-targets -- -D warnings` is clean
- [ ] `cargo build --locked` succeeds
- [ ] `cargo test --locked` passes
- [ ] Ran the app and confirmed the change (UI / Win32 paths)

<!-- If cargo fails with `os error 1455` (paging/commit limit), rerun with `-j 1`. -->

## Related

<!-- Issue or discussion links. Delete if none. -->
