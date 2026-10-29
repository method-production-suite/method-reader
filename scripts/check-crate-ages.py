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
its reason, may be younger than the cooldown (a security fix that cannot wait, a version
waiting out its cooldown, or the owner's override for one named release). A yanked
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
# A RustSec id, or a GitHub advisory id: GitHub can publish one days before RustSec does.
# Whole ids only: "RUSTSEC-2026-03460" or "GHSA-m665-v7pm-7grjzz" is not one.
SECURITY = re.compile(r"\b(?:RUSTSEC-\d{4}-\d{4}|GHSA(?:-[23456789cfghjmpqrvwx]{4}){3})\b")
AGES_OUT = re.compile(r"^ages-out \d{4}-\d{2}-\d{2}\b")
OWNER_APPROVED = "owner-approved override"
# The version must end there: "v1.0.0-rc1", "v1.0.0.1" or "v1.0.0+build" is not v1.0.0.
RELEASE = re.compile(r"\brelease (v\d+\.\d+\.\d+)(?![\w.+-])")


class BadExceptions(Exception):
    pass


def parse_exceptions(text):
    """{(name, version): "security" | "ages-out" | ("owner-approved", tag)}, or
    BadExceptions naming each bad line.

    A comment-only or blank line is skipped. Every other line is "name version  # reason",
    and the reason says its kind: a RUSTSEC or GHSA id makes it a security exception (whatever
    else it says), a leading "ages-out YYYY-MM-DD" an ages-out one, and a leading
    "owner-approved override" the owner's override for the one release it names
    ("release vX.Y.Z"), which a line of that kind must name.
    """
    out, bad = {}, []
    for n, line in enumerate(text.splitlines(), 1):
        entry, _, reason = line.partition("#")
        words = entry.split()
        if not words:
            continue
        reason = reason.strip()
        if len(words) != 2:
            bad.append(f"line {n}: not \"name version  # reason\": {line.strip()}")
        elif SECURITY.search(reason):
            out[(words[0], words[1])] = "security"
        elif AGES_OUT.match(reason):
            out[(words[0], words[1])] = "ages-out"
        elif reason.startswith(OWNER_APPROVED) and RELEASE.search(reason):
            out[(words[0], words[1])] = ("owner-approved", RELEASE.search(reason).group(1))
        elif reason.startswith(OWNER_APPROVED):
            bad.append(
                f"line {n}: {words[0]} {words[1]} is an owner-approved override that names "
                f"no \"release vX.Y.Z\": {line.strip()}"
            )
        else:
            bad.append(
                f"line {n}: {words[0]} {words[1]} says neither a RUSTSEC or GHSA id, "
                f"\"ages-out YYYY-MM-DD\" nor \"owner-approved override\": {line.strip()}"
            )
    if bad:
        raise BadExceptions("\n".join(bad))
    return out


def excepted(path, release, tag=None):
    """{(name, version)} allowed under the cooldown. Under --release, the security ones,
    and an owner-approved override only when it names this release's tag."""
    if not path.exists():
        return set()
    kinds = parse_exceptions(path.read_text())
    return {
        k
        for k, kind in kinds.items()
        if not release or kind == "security" or (tag is not None and kind == ("owner-approved", tag))
    }


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
    ap.add_argument(
        "--release",
        action="store_true",
        help="honour only security (RUSTSEC or GHSA) exceptions and the owner's override for --tag; "
        "a young ages-out crate fails",
    )
    ap.add_argument(
        "--tag",
        metavar="vX.Y.Z",
        help="the release being tagged: with --release, an owner-approved override naming it is honoured",
    )
    ap.add_argument("--exceptions", type=pathlib.Path, default=EXCEPTIONS)
    ap.add_argument(
        "--print-exceptions",
        action="store_true",
        help="print the exceptions this mode honours, then exit (no network)",
    )
    args = ap.parse_args()

    try:
        allowed_young = excepted(args.exceptions, args.release, args.tag)
    except BadExceptions as e:
        print(f"{args.exceptions}: every exception needs a kind\n{e}", file=sys.stderr)
        sys.exit(2)
    if args.print_exceptions:
        for name, version in sorted(allowed_young):
            print(f"{name} {version}")
        return

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
