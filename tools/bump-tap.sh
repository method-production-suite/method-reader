#!/usr/bin/env bash
#
# tools/bump-tap.sh <version> <tap-checkout>
#
# Rewrite a homebrew-tap checkout to serve <version>. Edits only; it does
# not commit, so it can be run locally to see the diff, and release.yml's
# `tap` job does the committing.
#
# WHY A SCRIPT AND NOT YAML: this is the step that has silently failed
# before. Inline in a workflow it can only be tested by cutting a release.
# Here it can be run against a real checkout, and its most useful property
# checked directly: run it at the version the tap already serves and the
# diff must be EMPTY. tests/tap-scripts.test.ts runs it against a fake tap.
#
# SHAs come from the digests GitHub reports for the release assets, so
# nothing is downloaded and nothing is re-hashed. The release body quotes
# the same values, but that is prose; this is the API's own record.
#
# bash 3.2 safe (macOS runners): no arrays, no process substitution. A
# bash-3.2 array trap has already killed one release here after
# notarization.
set -euo pipefail

VERSION="${1:-}"
TAP="${2:-}"
if [ -z "$VERSION" ] || [ -z "$TAP" ]; then
  echo "usage: tools/bump-tap.sh <version> <tap-checkout>" >&2
  exit 2
fi
VERSION="${VERSION#v}"
REPO="${SCREEPUB_REPO:-ssandweiss/screepub}"

CASK="$TAP/Casks/screepub.rb"
FORMULA="$TAP/Formula/screepub.rb"
for f in "$CASK" "$FORMULA"; do
  [ -f "$f" ] || { echo "not a tap checkout: $f missing" >&2; exit 1; }
done

# The release's asset list, read ONCE: every decision below (the digest,
# and whether the release still serves Intel) comes from the same answer.
# Fail closed: if the list cannot be read, nothing is written. (Read per
# asset with `|| true`, a transient failure on the Intel lookup once looked
# exactly like "no Intel tarball".) One "name sha256:<hex>" line per asset.
if ! ASSETS="$(gh api "repos/$REPO/releases/tags/v$VERSION" \
    -q '.assets[] | "\(.name) \(.digest)"' 2>/dev/null)"; then
  echo "bump-tap: could not read the assets of release v$VERSION; nothing written" >&2
  echo "bump-tap: is release v$VERSION published? Try again if GitHub was having a moment." >&2
  exit 1
fi
digest() {
  printf '%s\n' "$ASSETS" | awk -v n="$1" '$1 == n { sub(/^sha256:/, "", $2); print $2 }'
}
has_asset() { printf '%s\n' "$ASSETS" | awk -v n="$1" '$1 == n { found = 1 } END { exit !found }'; }
ARM_SHA="$(digest screepub-cli-macos-arm64.tar.gz)"

# Check the SHAPE, not merely that something came back. For a tag with no
# release, `gh api` prints its 404 JSON body on STDOUT — so a non-empty
# answer is not evidence of success, and an emptiness test happily wrote
# `{"message":"Not Found",...}` into the tap as a checksum. Caught by
# running the failure case rather than reasoning about it.
#
# Called plainly, never inside $(...): `exit` in a command substitution
# only leaves the subshell, which is the same class of trap.
check_sha() { # <value> <asset-name>
  if [ ${#1} -ne 64 ] || [ -n "$(printf '%s' "$1" | tr -d '0-9a-f')" ]; then
    echo "bump-tap: no usable sha256 for $2 at v$VERSION" >&2
    echo "bump-tap: got: ${1:-<empty>}" >&2
    echo "bump-tap: is release v$VERSION published, with its assets uploaded?" >&2
    exit 1
  fi
}
check_sha "$ARM_SHA" screepub-cli-macos-arm64.tar.gz

# This script writes the Apple-silicon-only tap. A release that still
# carries the Intel tarball (the identifier release, and every one before
# it) is served by a two-arch formula and a deprecated cask, which ITS OWN
# copy of this script writes; bumping it with this one would drop Intel
# brew users a release early. Refused, before anything is written.
INTEL=screepub-cli-macos-x64.tar.gz
if has_asset "$INTEL"; then
  echo "bump-tap: release v$VERSION still carries $INTEL, so its tap serves Intel Macs too." >&2
  echo "bump-tap: this script writes the Apple-silicon-only tap of the releases after it." >&2
  echo "bump-tap: bump v$VERSION with its own script: git show v$VERSION:tools/bump-tap.sh > /tmp/bump-tap.sh" >&2
  exit 1
fi

# --- cask: RETIRED at the identifier release, DISABLED after it ---------
# The app updates itself and is downloaded from the releases page, so the
# cask is never bumped again: it stays pinned to the last Swift release it
# served. The identifier release DEPRECATED it, which still installs that
# Swift app, whose own updater then carried people to the window. From the
# release after it that is a trap: the window is Apple silicon only, and
# the Swift updater installs the newest image with no architecture check,
# so an Intel Mac that installs this cask would be offered an app it cannot
# open. So from here on the cask is DISABLED: `brew install --cask` stops
# with "has been disabled because it is now downloaded from the releases
# page and updates itself!" (Homebrew's DeprecateDisable.message puts the
# because: text after "because it").
#
# The DATE must already be past wherever the person is: Homebrew compares
# it with the user's LOCAL day (cask/dsl.rb disable!: a date after
# Date.today only deprecates, and still installs), so "today in UTC" from a
# US-evening tag left the cask installable in the Americas until local
# midnight, saying "It will be disabled on" that day. So a deprecate!
# keeps its OWN date, which the identifier release's tap job wrote at
# least one release earlier; a cask with neither gets the day Intel was
# dropped, fixed, so a second run writes the same bytes.
#
# Where it goes is Homebrew's rule, not ours: `brew style` wants deprecate!
# and disable! as one stanza group right after the livecheck block (found by
# running it; placed after `homepage` it fails Cask/StanzaOrder). A
# deprecate! line is replaced where it stands; with neither, the line is
# inserted there. A cask already disabled is left alone, which keeps this
# script's rule: a second run produces no diff.
INTEL_DROPPED="2026-10-01"
DEPRECATED_ON="$(awk '/^  deprecate! date: "/ && !done {
  s = $0; sub(/^  deprecate! date: "/, "", s); sub(/".*/, "", s); print s; done = 1
}' "$CASK")"
case "$DEPRECATED_ON" in
  [0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]) ;;
  *) DEPRECATED_ON="" ;;
esac
disable_line() { # <date>
  printf '  disable! date: "%s", because: "is now downloaded from the releases page and updates itself"' "$1"
}
if grep -q '^  disable! ' "$CASK"; then
  :
elif grep -q '^  deprecate! ' "$CASK"; then
  DISABLE="$(disable_line "${DEPRECATED_ON:-$INTEL_DROPPED}")"
  awk -v line="$DISABLE" '/^  deprecate! / { print line; next } { print }' "$CASK" > "$CASK.new"
  mv "$CASK.new" "$CASK"
else
  DISABLE="$(disable_line "$INTEL_DROPPED")"
  awk -v line="$DISABLE" '
    { print }
    /^  livecheck do/ { inlive = 1 }
    inlive && /^  end$/ && !done { print ""; print line; done = 1; inlive = 0 }
  ' "$CASK" > "$CASK.new"
  mv "$CASK.new" "$CASK"
fi

# --- formula: Apple silicon only, written WHOLE --------------------------
# Through the identifier release the formula served two tarballs from an
# if/else on the CPU. From the release after it Intel Macs are
# not supported (owner, 2026-10-01; plan
# docs/superpowers/plans/2026-10-01-f2-handover-amendment.md): one tarball,
# and `depends_on arch: :arm64`, so brew on an Intel Mac refuses with
# Homebrew's own "The arm64 architecture is required for this software."
# rather than installing a binary that cannot run there.
#
# Written whole rather than patched, because the edit changes the file's
# SHAPE (a two-branch block becomes one url) and a pattern rewrite would
# need to know both shapes forever. The whole file is a pure function of
# the version and the digest, so a second run is byte-identical, which is
# this script's rule. The tap's formula is therefore this script's output:
# change it HERE.
#
# Two things brew enforces, both learned the hard way:
#  - NO version stanza. Homebrew scans the version out of the tag in the
#    url, and an explicit one audits as redundant. So the tag is literal.
#  - `depends_on arch:` BEFORE `depends_on :macos`. brew style's
#    FormulaAudit/DependencyOrder sorts dependencies by name, and "arch"
#    sorts before "macos".
# (The heredoc is unquoted so $REPO, $VERSION and $ARM_SHA expand: no other
# dollar sign or backtick may appear in it. Ruby's #{bin} is safe.)
cat > "$FORMULA.new" <<EOF
class Screepub < Formula
  desc "Screenplay PDF to reflowable EPUB3/MOBI converter"
  homepage "https://github.com/$REPO"
  # Written whole by tools/bump-tap.sh in the screepub repository at every
  # release: edit it there, not here. Apple silicon only since the release
  # after 0.9.0; Intel Macs are no longer supported.
  url "https://github.com/$REPO/releases/download/v$VERSION/screepub-cli-macos-arm64.tar.gz"
  sha256 "$ARM_SHA"
  license "AGPL-3.0-or-later"

  livecheck do
    url "https://github.com/$REPO/releases/latest"
    strategy :github_latest
  end

  # A signed, notarized Mach-O built by the project's release workflow.
  # There is no source build here, so no bottles.
  depends_on arch: :arm64
  depends_on :macos

  def install
    # The tarball holds a single file, already named plain screepub.
    bin.install "screepub"
  end

  test do
    # Pins the download to the version this formula claims: a stale url or a
    # mismatched asset fails here rather than at somebody's prompt.
    assert_match version.to_s, shell_output("#{bin}/screepub --version")
    assert_match "screenplay", shell_output("#{bin}/screepub --help")
  end
end
EOF
mv "$FORMULA.new" "$FORMULA"

# --- prove the edits actually landed -----------------------------------
# The cask edit is a pattern match against a structure this script
# assumes, and the formula is a template someone will one day edit. If
# either goes wrong, the result must fail here rather than commit cleanly
# and serve the OLD version — the exact shape of failure this whole thread
# is about. So assert the result rather than trusting the write.
fail() { echo "bump-tap: $1" >&2; exit 1; }
grep -q '^  disable! date: "[0-9-]*", because: ' "$CASK" \
  || fail "cask disable! did not take (no livecheck block to insert it after?)"
if grep -q '^  deprecate! ' "$CASK"; then
  fail "cask still carries deprecate! beside disable!"
fi
grep -q "/download/v$VERSION/screepub-cli-macos-arm64\.tar\.gz" "$FORMULA" \
  || fail "formula arm64 url did not take"
grep -q "\"$ARM_SHA\"" "$FORMULA" || fail "formula arm64 sha256 did not take"
grep -q '^  depends_on arch: :arm64$' "$FORMULA" || fail "formula does not declare arm64"
if grep -q 'screepub-cli-macos-x64' "$FORMULA"; then
  fail "formula still names the Intel tarball"
fi
if grep "/download/v[^/]*/" "$FORMULA" | grep -qv "/download/v$VERSION/"; then
  fail "formula still carries a url for some other version"
fi

echo "tap rewritten to $VERSION"
echo "  cask    retired (disabled, never bumped)"
echo "  formula arm64 $ARM_SHA (Apple silicon only)"
