# Windows Remediation Handle Fix SDD

## Goal

Fix the remaining `windows-latest` CI failure in CancerBroker by correcting the Windows `HANDLE` null checks in `src/remediation.rs` so the remediation code compiles cleanly under `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`.

## Current State

- `windows-latest` is already part of `.github/workflows/ci.yml`.
- `ubuntu-latest` and `macos-latest` currently pass the CI workflow.
- The latest failing run is `22990723184`.
- The failure happens in the Windows `Clippy` step before tests run.
- The failing code is in `src/remediation.rs` inside the Windows-only functions `platform_remediate_process()` and `is_alive_windows()`.

## Problem Statement

The Windows implementation uses `OpenProcess()` from `windows-sys` and stores the result in a `HANDLE`.

Current code:

- `src/remediation.rs:121` -> `if handle == 0 {`
- `src/remediation.rs:160` -> `if handle == 0 {`

GitHub Actions reports:

- `error[E0308]: mismatched types`
- expected `*mut c_void`, found `usize`

This means the current code is treating a Windows `HANDLE` like an integer sentinel instead of using a null pointer check appropriate for the `windows-sys` type.

## Design

### 1. Replace invalid integer comparisons

In the Windows-only remediation path, replace both `handle == 0` checks with a pointer-validity check that matches the actual `HANDLE` type.

Preferred form:

- `handle.is_null()`

Acceptable equivalent form:

- `handle == std::ptr::null_mut()`

The fix should be applied only at the two failing call sites and should not broaden the scope beyond the Windows compile error.

### 2. Preserve existing cleanup semantics

The fix must not change the remediation flow:

- call `OpenProcess()`
- treat a null handle as `AlreadyExited` / unavailable target
- continue to use `WaitForSingleObject()` for the grace window
- continue to call `TerminateProcess()` only when forced termination is required
- continue to call `CloseHandle()` on successfully opened handles

### 3. Add narrow verification coverage

Verification should prove the exact failure is gone without expanding feature scope:

- local `cargo fmt --all`
- local `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- local `cargo test --workspace`
- local `cargo build --workspace`
- GitHub Actions run showing:
  - `macos-latest` success
  - `ubuntu-latest` success
  - `windows-latest` success

## Files Expected To Change

- `src/remediation.rs`

## Non-Goals

- refactoring the wider Windows remediation design
- changing Unix remediation behavior
- changing workflow structure beyond what is already merged
- addressing the separate `Swatinem/rust-cache@v2` Node 20 deprecation warning
