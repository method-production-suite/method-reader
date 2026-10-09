#!/usr/bin/env bash
#
# tools/check-tap.sh [version]
#
# Exit 0 if ssandweiss/homebrew-tap serves `version` (default: the newest
# published non-prerelease) with checksums matching that release's assets.
# Exit 1 otherwise, having said which part is wrong.
#
# Reads the tap FROM GITHUB, never from a checkout. That distinction is the
# entire point: the tap sat at 0.3.0 for five releases while a local
# checkout showed 0.4.2, because the edits were never committed and
# `homebrew-tap/` is hidden from `git status` by `.git/info/exclude`.
#
# Needs no secret. Both repos are public. tests/tap-scripts.test.ts runs it
# against a fake tap and a fake `gh`.
# bash 3.2 safe: no arrays, no process substitution.
set -euo pipefail

REPO="${SCREEPUB_REPO:-ssandweiss/screepub}"
TAP="${SCREEPUB_TAP:-ssandweiss/homebrew-tap}"

VERSION="${1:-}"
if [ -z "$VERSION" ]; then
  VERSION="$(gh release list --repo "$REPO" \
    --exclude-drafts --exclude-pre-releases --limit 1 \
    --json tagName -q '.[0].tagName')"
fi
VERSION="${VERSION#v}"
if [ -z "$VERSION" ]; then
  echo "check-tap: no published, non-prerelease release to compare against" >&2
  exit 1
fi

fetch() { gh api "repos/$TAP/contents/$1" -q .content | base64 -d; }
CASK="$(fetch Casks/screepub.rb)"
FORMULA="$(fetch Formula/screepub.rb)"

# Checksums, from the digests GitHub reports per asset, so nothing is
# downloaded. A right version with a wrong SHA still breaks every install.
digest() {
  gh api "repos/$REPO/releases/tags/v$VERSION" \
    -q ".assets[] | select(.name == \"$1\") | .digest" 2>/dev/null \
    | sed 's/^sha256://'
}
is_sha() { [ ${#1} -eq 64 ] && [ -z "$(printf '%s' "$1" | tr -d '0-9a-f')" ]; }

# Intel. The identifier release is the last to carry an Intel tarball; from
# the release after it Intel Macs are not supported (owner, 2026-10-01;
# plan docs/superpowers/plans/2026-10-01-f2-handover-amendment.md). Keyed
# on what the RELEASE carries, never on a version number, so each release
# is judged as it shipped (the weekly check reads the identifier release
# until the next one is tagged): the cask below, and the formula's urls and
# arch further down.
INTEL="screepub-cli-macos-x64.tar.gz"
INTEL_SHA="$(digest "$INTEL" || true)"
if is_sha "$INTEL_SHA"; then INTEL_ERA=1; else INTEL_ERA=0; fi

# The CASK was retired at the identifier release (owner, 2026-10-01): it is
# never bumped again, so its version is not compared. What must hold is
# that it says so. Through the identifier release it is DEPRECATED (it
# still installs the last Swift app, whose updater carries people to the
# window); from the release after it, DISABLED, because that updater would
# offer an Intel Mac an Apple-silicon-only app. A frozen cask that says
# neither serves an old app silently, which is the failure this file
# exists to catch. The formula deliberately has no `version` (it audits as
# redundant), so its version is literal in its url (or, through the
# identifier release, in both of its urls, which must agree).
if [ "$INTEL_ERA" -eq 1 ]; then
  CASK_WANT=deprecate
  CASK_WHY="it still carries $INTEL, and the cask may still install the last Swift app"
else
  CASK_WANT=disable
  CASK_WHY="it carries no $INTEL, and the last Swift app's updater would offer an Intel Mac an app it cannot open"
fi
FORMULA_V="$(printf '%s\n' "$FORMULA" \
  | sed -n 's|.*/releases/download/v\([^/]*\)/.*|\1|p' | sort -u)"

echo "newest release: $VERSION"
if printf '%s\n' "$CASK" | grep -q '^  disable! '; then
  echo "cask:           retired (disabled)"
elif printf '%s\n' "$CASK" | grep -q '^  deprecate! '; then
  echo "cask:           retired (deprecated)"
else
  echo "cask:           NOT retired"
fi
echo "formula pins:   $(printf '%s' "$FORMULA_V" | tr '\n' ' ')"

FAIL=0
if ! printf '%s\n' "$CASK" | grep -q "^  $CASK_WANT! "; then
  echo "::error::the frozen cask must say $CASK_WANT! for release v$VERSION ($CASK_WHY), and it does not"
  FAIL=1
fi
# Disabled means no deprecate! left beside it: Homebrew asks deprecated?
# before disabled?, so a cask carrying both lines still installs.
if [ "$INTEL_ERA" -eq 0 ] && printf '%s\n' "$CASK" | grep -q '^  deprecate! '; then
  echo "::error::the frozen cask still has a deprecate! line, and a cask with deprecate! still installs whatever else it says; release v$VERSION wants disable! alone"
  FAIL=1
fi
if [ "$(printf '%s\n' "$FORMULA_V" | grep -c .)" -ne 1 ]; then
  echo "::error::the formula's download urls disagree: $(printf '%s' "$FORMULA_V" | tr '\n' ' ')"
  FAIL=1
elif [ "$FORMULA_V" != "$VERSION" ]; then
  echo "::error::the formula pins $FORMULA_V but the newest release is $VERSION"
  FAIL=1
fi

check_sha() { # <asset> <human name> <file contents>
  want="$(digest "$1" || true)"
  # Shape, not emptiness: `gh api` prints its 404 body on stdout, so a
  # non-empty answer is not evidence of success.
  if [ ${#want} -ne 64 ] || [ -n "$(printf '%s' "$want" | tr -d '0-9a-f')" ]; then
    echo "::error::release v$VERSION has no usable digest for $1"
    FAIL=1
  elif ! printf '%s\n' "$3" | grep -q "$want"; then
    echo "::error::$2 does not carry the SHA-256 of $1 ($want)"
    FAIL=1
  fi
}
check_sha "screepub-cli-macos-arm64.tar.gz" "the formula" "$FORMULA"

# A release that still has the Intel tarball keeps its two-arch formula and
# both digests; one without it must be served by a formula that names no
# Intel url and says `depends_on arch: :arm64`, or brew on an Intel Mac
# downloads a missing file, or installs a binary that cannot run there.
if [ "$INTEL_ERA" -eq 1 ]; then
  check_sha "$INTEL" "the formula" "$FORMULA"
else
  if printf '%s\n' "$FORMULA" | grep -q 'screepub-cli-macos-x64'; then
    echo "::error::the formula still serves $INTEL, which release v$VERSION does not carry (Apple silicon only)"
    FAIL=1
  fi
  if ! printf '%s\n' "$FORMULA" | grep -q '^  depends_on arch: :arm64$'; then
    echo "::error::the formula does not say depends_on arch: :arm64, so brew would install an Apple silicon binary on an Intel Mac"
    FAIL=1
  fi
fi

if [ "$FAIL" -ne 0 ]; then
  echo "::notice::Bump https://github.com/$TAP: tools/bump-tap.sh does it (and disables the retired cask), and the release run's summary carries the SHA. See docs/release-secrets.md."
  exit 1
fi
echo "The tap serves $VERSION, with matching checksums."
