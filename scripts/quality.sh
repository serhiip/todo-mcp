#!/usr/bin/env bash
set -e
# Full quality gate: warning-free build, tests, and clippy for all targets.
# Warnings are denied via .cargo/config.toml for build/test.
# Use deterministic options so CI and local runs stay in sync.
cargo build --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
# Release profile: catch profile-specific code paths and config drift (e.g. opt-level, debug asserts).
cargo build --release
cargo clippy --release -- -D warnings
