#!/usr/bin/env python3
"""Supply-chain cooldown check: fail if any crates.io package in Cargo.lock is too new.

Usage: python3 scripts/check-crate-ages.py [--days 14] [--lock Cargo.lock] [--only-changed REF]

Looks up each locked crates.io version's publish date in crates.io's sparse index (the
CDN cargo reads; one file per crate, every version with its pubtime) and lists any
younger than the cooldown. A package from
anywhere else (a git repository, another registry) has no publish date to check, so it
fails the check too: fail closed. With --only-changed REF, only packages added or bumped
since that git ref are checked. Stdlib only, so it runs anywhere Python 3 does.

Method Reader's copy (from Method Budget's) adds one exception: a package from a git
repository listed in scripts/crate-age-git-allow.txt passes when Cargo.toml pins it with
rev = the full 40-character commit (and the lock agrees), because a pinned commit cannot change under us and the
repository was reviewed when it was added to that list. Anything else from git still
fails. And one more: an exact NAME VERSION listed in scripts/crate-age-exceptions.txt, with
its reason, may be younger than the cooldown (a security fix that cannot wait). A yanked
version always fails.
"""

import argparse
import datetime as dt
import json
import pathlib
import re
import subprocess
import sys
import time
import tomllib
import urllib.error
import urllib.request

UA = "method-reader cooldown check (https://github.com/method-production-suite/method-reader)"


CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
GIT_ALLOW = pathlib.Path(__file__).with_name("crate-age-git-allow.txt")
# The manifest's own pin, not only the lock's: the lock writes the full commit
# after "#" whatever Cargo.toml asked for (a branch, a tag), so require the query
# to be rev=<that same 40-character commit>.
PINNED = re.compile(r"^git\+(https://[^?#]+)\?rev=([0-9a-f]{40})#\2$")


EXCEPTIONS = pathlib.Path(__file__).with_name("crate-age-exceptions.txt")


def excepted():
    """{(name, version)} allowed under the cooldown, each listed with a reason."""
    if not EXCEPTIONS.exists():
        return set()
    out = set()
    for line in EXCEPTIONS.read_text().splitlines():
        words = line.split("#", 1)[0].split()
        if len(words) == 2:
            out.add((words[0], words[1]))
    return out


def git_allowed(source):
    """True for a git source from an allowlisted repository, pinned to a full commit."""
    m = PINNED.match(source)
    if not m or not GIT_ALLOW.exists():
        return False
    allowed = {
        line.split("#", 1)[0].strip()
        for line in GIT_ALLOW.read_text().splitlines()
    } - {""}
    return m.group(1) in allowed


def external_packages(lock_text):
    """(name, version, source) of every locked package that isn't a workspace member."""
    lock = tomllib.loads(lock_text)
    return {
        (p["name"], p["version"], p["source"]) for p in lock.get("package", []) if p.get("source")
    }


def index_path(name):
    """A crate's file in crates.io's sparse index (the layout cargo reads)."""
    n = name.lower()
    if len(n) <= 2:
        return f"{len(n)}/{n}"
    if len(n) == 3:
        return f"3/{n[0]}/{n}"
    return f"{n[:2]}/{n[2:4]}/{n}"


INDEX = {}


def fetch(url, tries=6):
    """GET with patience: a 429 or a server error waits (honouring
    Retry-After) and asks again; anything else fails at once."""
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    wait = 5
    for attempt in range(tries):
        try:
            with urllib.request.urlopen(req, timeout=30) as r:
                return r.read().decode()
        except urllib.error.HTTPError as e:
            if e.code != 429 and e.code < 500 or attempt == tries - 1:
                raise
            after = e.headers.get("Retry-After", "")
            time.sleep(int(after) if after.isdigit() else wait)
            wait = min(wait * 2, 120)


def published(name, version):
    """The version's publish time and yanked flag. Read from crates.io's sparse
    index (index.crates.io, the CDN cargo itself uses; one file per crate
    holds every version with its pubtime), which a full-lock check cannot
    exhaust the way one API call per version did. A version the index gives
    no pubtime falls back to the API."""
    if name not in INDEX:
        entries = {}
        for line in fetch(f"https://index.crates.io/{index_path(name)}").splitlines():
            if line.strip():
                e = json.loads(line)
                entries[e["vers"]] = e
        INDEX[name] = entries
    e = INDEX[name].get(version)
    if e and e.get("pubtime"):
        return dt.datetime.fromisoformat(e["pubtime"].replace("Z", "+00:00")), e["yanked"]
    v = json.loads(fetch(f"https://crates.io/api/v1/crates/{name}/{version}"))["version"]
    return dt.datetime.fromisoformat(v["created_at"].replace("Z", "+00:00")), v["yanked"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--days", type=int, default=14)
    ap.add_argument("--lock", default="Cargo.lock")
    ap.add_argument("--only-changed", metavar="REF")
    args = ap.parse_args()

    with open(args.lock) as f:
        pkgs = external_packages(f.read())
    if args.only_changed:
        old = subprocess.run(
            ["git", "show", f"{args.only_changed}:{args.lock}"], capture_output=True, text=True
        )
        if old.returncode == 0:
            pkgs -= external_packages(old.stdout)

    bad = []
    for name, version, source in sorted(p for p in pkgs if p[2] != CRATES_IO):
        if git_allowed(source):
            print(f"{name:40} {version:16}  pinned git commit, allowlisted repository")
            continue
        print(f"{name:40} {version:16}  NOT FROM CRATES.IO ({source}): no publish date to check")
        bad.append(f"{name} {version} ({source})")
    pkgs = {(name, version) for name, version, source in pkgs if source == CRATES_IO}

    allowed_young = excepted()
    stale = []
    now = dt.datetime.now(dt.timezone.utc)
    for name, version in sorted(pkgs):
        when, yanked = published(name, version)
        age = (now - when).days
        flag = ""
        if age < args.days and (name, version) in allowed_young:
            flag = "  young, listed as an exception"
        elif (name, version) in allowed_young:
            flag = "  listed as an exception but old enough now"
            stale.append(f"{name} {version}")
        elif age < args.days:
            flag = f"  TOO NEW (< {args.days} days)"
            bad.append(f"{name} {version}")
        if yanked:
            flag += "  YANKED"
            bad.append(f"{name} {version} (yanked)")
        print(f"{name:40} {version:16} {age:5} days{flag}")

    print(f"\nchecked {len(pkgs)} crates.io crates; {len(bad)} problem(s)")
    # Not a failure: a listed version that has aged past the cooldown only
    # needs its line removed. A warning, so the list stays honest.
    for entry in stale:
        print(f"::warning::{entry} is {args.days}+ days old now: remove its line from "
              f"scripts/crate-age-exceptions.txt")
    if bad:
        print("downgrade with: cargo update -p NAME --precise VERSION")
        sys.exit(1)


if __name__ == "__main__":
    main()
