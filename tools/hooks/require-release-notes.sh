#!/bin/sh
# The Claude Code PreToolUse hook for Bash calls: no version tag without its
# release notes committed at HEAD. The check itself is Rust,
# `cargo xtask require-release-notes` (rust/crates/xtask/src/release_hook.rs,
# tested by rust/crates/xtask/tests/release_hook.rs); this shim exists only
# because a hook is a command line. It runs the xtask binary already built
# in the suite's shared build folder (CARGO_TARGET_DIR, else
# ~/.cache/method-target/screepub), and builds it first when there is none.
# The payload
# arrives on stdin and goes straight through; exit 2 blocks the call.
#
# An ergonomics guard, not a security boundary: the release workflow's
# checks job gates every tag however it was made.
repo="$(cd "$(dirname "$0")/../.." && pwd)"
target="${CARGO_TARGET_DIR:-$HOME/.cache/method-target/screepub}"
bin="$target/debug/xtask"
if [ ! -x "$bin" ] && ! CARGO_TARGET_DIR="$target" cargo build --quiet --manifest-path "$repo/rust/Cargo.toml" -p xtask >&2; then
  # No check to run. Block only what could touch a tag, as the check would.
  payload="$(cat)"
  case "$payload" in
    *"git tag"* | *"git push"* | *"gh release"*)
      echo "require-release-notes: blocked: the check could not be built (cd rust && cargo build -p xtask)" >&2
      exit 2 ;;
  esac
  exit 0
fi
exec "$bin" require-release-notes
