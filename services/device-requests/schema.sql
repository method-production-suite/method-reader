-- The device-request service's D1 tables. Counts only: no IP address, no
-- user agent, nothing that says who asked.

-- "My device isn't here": one row per device, keyed by its name lowercased,
-- trimmed and with runs of spaces collapsed; `spelling` is the latest one
-- typed, as typed (trimmed).
CREATE TABLE IF NOT EXISTS device_requests (
  name_key   TEXT PRIMARY KEY,
  spelling   TEXT NOT NULL,
  count      INTEGER NOT NULL,
  first_seen TEXT NOT NULL,
  last_seen  TEXT NOT NULL
);

-- "It worked" / "It did not work" on a route nobody here has tried on real
-- hardware: one row per route and platform.
CREATE TABLE IF NOT EXISTS route_reports (
  route        TEXT NOT NULL,
  platform     TEXT NOT NULL,
  worked       INTEGER NOT NULL DEFAULT 0,
  did_not_work INTEGER NOT NULL DEFAULT 0,
  first_seen   TEXT NOT NULL,
  last_seen    TEXT NOT NULL,
  PRIMARY KEY (route, platform)
);

-- The rate limit: accepted requests per sender per hour. `sender` is a
-- SHA-256 of the connecting address and a salt that changes every day, so a
-- sender cannot be followed from one day to the next; the address itself is
-- never stored. Old hours are deleted as new ones start.
CREATE TABLE IF NOT EXISTS rate_limits (
  sender TEXT NOT NULL,
  hour   INTEGER NOT NULL,
  count  INTEGER NOT NULL,
  PRIMARY KEY (sender, hour)
);
CREATE INDEX IF NOT EXISTS rate_limits_hour ON rate_limits (hour);
