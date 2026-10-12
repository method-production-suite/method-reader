#!/usr/bin/env bash
# Regenerates the WebKit localStorage fixtures tests/webkit.rs reads, with
# the sqlite3 command-line tool (macOS ships one). Invented values only:
# these stand in for the Tauri window's storage, never a real one.
#
#   rust/crates/sp-update/tests/data/make-webkit-fixtures.sh
#
# ItemTable is WebKit's own schema; values are UTF-16LE blobs, as WebKit
# writes them.
set -euo pipefail
cd "$(dirname "$0")"

u16() { printf '%s' "$1" | iconv -f UTF-8 -t UTF-16LE | xxd -p | tr -d '\n'; }

SCHEMA='CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB NOT NULL ON CONFLICT FAIL);'

# 1. The older layout: one file per origin, rollback journal.
rm -rf webkit-legacy
mkdir -p webkit-legacy/LocalStorage
sqlite3 webkit-legacy/LocalStorage/tauri_localhost_0.localstorage <<SQL
$SCHEMA
INSERT INTO ItemTable VALUES ('updateOptIn', X'$(u16 true)');
INSERT INTO ItemTable VALUES ('updateAsked', X'$(u16 true)');
INSERT INTO ItemTable VALUES ('updateLastChecked', X'$(u16 1767225600000)');
INSERT INTO ItemTable VALUES ('updateFound', X'$(u16 0.11.1)');
INSERT INTO ItemTable VALUES ('somethingElse', X'$(u16 ignored)');
SQL

# 2. The newer layout, with a second origin beside it, and the window's
#    rows still in the write-ahead log (copied while the database is open,
#    with automatic checkpoints off, as a running window leaves it).
rm -rf webkit-modern
W=webkit-modern/Default/a1/b2
O=webkit-modern/Default/c3/d4
mkdir -p "$W/LocalStorage" "$O/LocalStorage"
printf '\x05\x00tauri\x09\x00localhost\x00\x00' > "$W/origin"
printf '\x05\x00https\x0b\x00example.com\x00\x00' > "$O/origin"
sqlite3 "$O/LocalStorage/localstorage.sqlite3" <<SQL
$SCHEMA
INSERT INTO ItemTable VALUES ('updateOptIn', X'$(u16 false)');
SQL
sqlite3 /tmp/screepub-wal-fixture.sqlite3 <<SQL
PRAGMA journal_mode=WAL;
PRAGMA wal_autocheckpoint=0;
$SCHEMA
INSERT INTO ItemTable VALUES ('updateOptIn', X'$(u16 false)');
INSERT INTO ItemTable VALUES ('updateAsked', X'$(u16 true)');
INSERT INTO ItemTable VALUES ('updateFound', X'');
.shell cp /tmp/screepub-wal-fixture.sqlite3 $W/LocalStorage/localstorage.sqlite3
.shell cp /tmp/screepub-wal-fixture.sqlite3-wal $W/LocalStorage/localstorage.sqlite3-wal
SQL
rm -f /tmp/screepub-wal-fixture.sqlite3 /tmp/screepub-wal-fixture.sqlite3-wal /tmp/screepub-wal-fixture.sqlite3-shm

# 3. A table big enough for interior pages and an overflow chain.
rm -f big.sqlite3
{
  echo "PRAGMA page_size=1024;"
  echo "$SCHEMA"
  echo "BEGIN;"
  for i in $(seq 1 400); do echo "INSERT INTO ItemTable VALUES ('key$i', X'$(u16 "value$i")');"; done
  echo "INSERT INTO ItemTable VALUES ('long', X'$(u16 "$(printf 'x%.0s' $(seq 1 3000))")');"
  echo "COMMIT;"
} | sqlite3 big.sqlite3
