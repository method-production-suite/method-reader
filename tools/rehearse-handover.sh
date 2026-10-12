#!/usr/bin/env bash
#
# The handover rehearsal: watch a real Tauri 0.11.1 window update itself
# into the Rust app, on this Mac, with nothing published and no real key.
#
#   tools/rehearse-handover.sh prepare        build both apps, the archive, a signed latest.json
#   tools/rehearse-handover.sh run <work>     serve latest.json locally, open the Tauri window
#   tools/rehearse-handover.sh verify <work>  check what the update left behind
#   tools/rehearse-handover.sh clean <work>   remove the rehearsal's own folders
#
# What it builds, all in a fresh `mktemp -d` folder:
#   - the Tauri window exactly as tagged v0.11.1 (git archive of the tag),
#     with a config overlay that changes ONLY: the identifier (to
#     com.darkwell.screepub.rehearsal, so WebKit storage, caches and the
#     Rust app's own storage stay away from the real app's), the updater's
#     public key (a throwaway one) and its endpoint (http://127.0.0.1).
#   - the Rust app from this checkout, as version 0.12.0, with the same
#     identifier, its own updater baked with the same throwaway key and
#     endpoint (SCREEPUB_UPDATE_* at compile time).
#   - Screepub-macOS.app.tar.gz of it, signed with the throwaway key by
#     `sp-release sign-update`, and latest.json pointing at the local
#     server, checked by `sp-release verify-update` before anything runs.
#
# Both apps run with SCREEPUB_CONFIG_DIR and SCREEPUB_LIBRARY inside the
# work folder, inherited across the restart, so the real settings and
# library are never read or written. Nothing is code-signed: locally built
# apps carry no quarantine, and the linker signs arm64 binaries ad hoc.
#
# Needs: macOS on Apple silicon, cargo (rustup), bun 1.3.14, the Tauri
# CLI 2.11.4 (`cargo install tauri-cli --version 2.11.4 --locked`), git.
# The first prepare downloads crates and bun packages.
#
# bash 3.2 safe (macOS): no arrays, no process substitution.
set -euo pipefail

ID="com.darkwell.screepub.rehearsal"
PORT="${REHEARSAL_PORT:-8765}"
TAG="v0.11.1"
NEW="0.12.0"
REPO_DIR="$(cd "$(dirname "$0")/.." && pwd)"

die() { echo "rehearse-handover: $*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "$1 is not installed. $2"; }

prepare() {
  [ "$(uname -s)" = Darwin ] || die "the handover is a Mac's (latest.json lists darwin-aarch64 only)"
  [ "$(uname -m)" = arm64 ] || die "Apple silicon only, like the release"
  need cargo "Install Rust with rustup."
  need bun "Install bun 1.3.14."
  cargo tauri --version 2>/dev/null | grep -q '2\.11\.4' \
    || die "the Tauri CLI 2.11.4 is needed: cargo install tauri-cli --version 2.11.4 --locked"

  W="$(mktemp -d "${TMPDIR:-/tmp}/screepub-handover.XXXXXX")"
  echo "work folder: $W"
  mkdir -p "$W/site" "$W/Applications" "$W/config" "$W/library" "$W/keys"

  # 1. The release tools, from this checkout.
  (cd "$REPO_DIR/rust" && cargo build --release -p sp-release)
  SP="$REPO_DIR/rust/target/release/sp-release"
  "$SP" keygen --out "$W/keys/rehearsal.key"
  PUB="$(cat "$W/keys/rehearsal.key.pub")"
  ENDPOINT="http://127.0.0.1:$PORT/latest.json"

  # 2. The Tauri window as tagged, with the rehearsal overlay.
  mkdir -p "$W/tauri-src"
  git -C "$REPO_DIR" archive "$TAG" | tar -x -C "$W/tauri-src"
  cat > "$W/rehearsal.conf.json" <<JSON
{
  "identifier": "$ID",
  "plugins": {
    "updater": {
      "pubkey": "$PUB",
      "endpoints": ["$ENDPOINT"],
      "dangerousInsecureTransportProtocol": true
    }
  }
}
JSON
  (cd "$W/tauri-src" && bun install --frozen-lockfile && bun tools/build-sidecar.ts --host)
  (cd "$W/tauri-src/desktop/src-tauri" \
    && cargo tauri build --bundles app --config "$W/rehearsal.conf.json")
  cp -R "$W/tauri-src/desktop/src-tauri/target/release/bundle/macos/Screepub.app" "$W/Applications/"

  # 3. The Rust app as 0.12.0, baked with the rehearsal key and endpoint.
  mkdir -p "$W/rust-src"
  # The whole commit: the crates read files outside rust/ at build time
  # (format-defaults.json, the release notes, the icons).
  git -C "$REPO_DIR" archive HEAD | tar -x -C "$W/rust-src"
  sed -i '' -E '/^\[workspace\.package\]/,/^\[/ s/^version = "[^"]*"/version = "'"$NEW"'"/' \
    "$W/rust-src/rust/Cargo.toml"
  (cd "$W/rust-src/rust" \
    && SCREEPUB_UPDATE_ENDPOINT="$ENDPOINT" SCREEPUB_UPDATE_PUBKEY="$PUB" SCREEPUB_UPDATE_INSECURE=1 \
       cargo build --release -p sp-app)
  "$SP" mac-app --version "$NEW" --identifier "$ID" \
    --window "$W/rust-src/rust/target/release/screepub-window" \
    --icon "$W/rust-src/desktop/src-tauri/icons/icon.icns" \
    --resource "$W/rust-src/LICENSE" --resource "$W/rust-src/THIRD-PARTY-NOTICES.md" \
    --out "$W/build"

  # 4. The update, signed and described, as release.yml would publish it.
  "$SP" update-archive --app "$W/build/Screepub.app" --out "$W/site/Screepub-macOS.app.tar.gz"
  TAURI_SIGNING_PRIVATE_KEY="$(cat "$W/keys/rehearsal.key")" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
    "$SP" sign-update --file "$W/site/Screepub-macOS.app.tar.gz" --version "$NEW"
  "$SP" manifest --dir "$W/site" --version "$NEW" --out "$W/site/latest.json" \
    --asset-base "http://127.0.0.1:$PORT"
  "$SP" verify-update --dir "$W/site" --manifest "$W/site/latest.json" --pubkey "$PUB"

  echo
  echo "Prepared. Next: tools/rehearse-handover.sh run $W"
}

run() {
  W="${1:?usage: rehearse-handover.sh run <work folder>}"
  SP="$REPO_DIR/rust/target/release/sp-release"
  [ -f "$W/site/latest.json" ] || die "$W is not a prepared work folder"
  "$SP" serve --dir "$W/site" --port "$PORT" 2> "$W/serve.log" &
  echo $! > "$W/serve.pid"
  sleep 1
  echo "serving $W/site on http://127.0.0.1:$PORT (log: $W/serve.log)"
  echo
  echo "Opening the Tauri 0.11.1 window. In it:"
  echo "  1. Answer the once-a-day question (either answer; 'yes' also tests that"
  echo "     the consent carries over), or open the release notes and press"
  echo "     Check for updates."
  echo "  2. Press 'Update to $NEW' (beside the stamp, or Install $NEW in the notes)."
  echo "  3. Watch it download, install and restart. The window that comes back"
  echo "     is the Rust app: its release notes say $NEW."
  echo "Then: tools/rehearse-handover.sh verify $W"
  SCREEPUB_CONFIG_DIR="$W/config" SCREEPUB_LIBRARY="$W/library" \
    "$W/Applications/Screepub.app/Contents/MacOS/screepub-desktop" >"$W/app.log" 2>&1 &
  echo $! > "$W/app.pid"
}

plist_get() { /usr/libexec/PlistBuddy -c "Print :$2" "$1/Contents/Info.plist" 2>/dev/null || true; }

verify() {
  W="${1:?usage: rehearse-handover.sh verify <work folder>}"
  APP="$W/Applications/Screepub.app"
  FAIL=0
  check() { if eval "$2"; then echo "  ok   $1"; else echo "  FAIL $1"; FAIL=1; fi; }
  echo "after the update:"
  check "the bundle at the same path says $NEW" "[ \"\$(plist_get \"$APP\" CFBundleShortVersionString)\" = $NEW ]"
  check "with the same identifier" "[ \"\$(plist_get \"$APP\" CFBundleIdentifier)\" = $ID ]"
  check "and runs screepub-desktop, the Rust window" "[ \"\$(plist_get \"$APP\" CFBundleExecutable)\" = screepub-desktop ]"
  check "the Tauri engine sidecar is gone" "[ ! -e \"$APP/Contents/MacOS/screepub-engine\" ]"
  check "nothing left beside the bundle" "[ \"\$(ls -A \"$W/Applications\")\" = Screepub.app ]"
  check "the window fetched latest.json" "grep -q 'GET /latest.json -> 200' \"$W/serve.log\""
  check "and the signed archive" "grep -q 'GET /Screepub-macOS.app.tar.gz -> 200' \"$W/serve.log\""
  check "the Rust app is running from the bundle" "pgrep -f \"$APP/Contents/MacOS/screepub-desktop\" >/dev/null"
  STORE="$HOME/Library/Application Support/$ID/window-storage.json"
  if [ -f "$STORE" ]; then
    echo "  ok   the Rust app wrote its storage: $(cat "$STORE")"
    echo "       (updateAsked/updateOptIn there are what the Tauri window had: compare with your answer)"
  else
    echo "  FAIL the Rust app has not written $STORE yet"
    FAIL=1
  fi
  if [ "$FAIL" = 0 ]; then echo "The handover worked."; else echo "Something did not hand over; see above."; exit 1; fi
}

clean() {
  W="${1:?usage: rehearse-handover.sh clean <work folder>}"
  for f in serve.pid app.pid; do
    [ -f "$W/$f" ] && kill "$(cat "$W/$f")" 2>/dev/null || true
  done
  pkill -f "$W/Applications/Screepub.app/Contents/MacOS/" 2>/dev/null || true
  # Only the rehearsal identifier's folders, never com.darkwell.screepub's.
  rm -rf "$HOME/Library/WebKit/$ID" "$HOME/Library/Caches/$ID" \
    "$HOME/Library/Application Support/$ID" "$HOME/Library/Saved Application State/$ID.savedState"
  rm -rf "$W"
  echo "removed $W and the $ID folders"
}

case "${1:-}" in
  prepare) prepare ;;
  run) run "${2:-}" ;;
  verify) verify "${2:-}" ;;
  clean) clean "${2:-}" ;;
  *) sed -n '3,9p' "$0"; exit 2 ;;
esac
