-- The device-request service's D1 tables. Counts only: no IP address, no
-- user agent, nothing that says who asked, and dates to the day, never the
-- time. The rate limit is Cloudflare's rate-limit binding (wrangler.toml),
-- which keeps nothing here.

-- "My device isn't here": one row per device, keyed by its name lowercased,
-- trimmed and with runs of spaces collapsed; `spelling` is the latest one
-- typed, as typed (trimmed). Dates are YYYY-MM-DD.
CREATE TABLE IF NOT EXISTS device_requests (
  name_key   TEXT PRIMARY KEY,
  spelling   TEXT NOT NULL,
  count      INTEGER NOT NULL,
  first_seen TEXT NOT NULL,
  last_seen  TEXT NOT NULL
);

-- "It worked" / "It did not work" on a route nobody here has tried on real
-- hardware: one row per route and platform. Dates are YYYY-MM-DD.
CREATE TABLE IF NOT EXISTS route_reports (
  route        TEXT NOT NULL,
  platform     TEXT NOT NULL,
  worked       INTEGER NOT NULL DEFAULT 0,
  did_not_work INTEGER NOT NULL DEFAULT 0,
  first_seen   TEXT NOT NULL,
  last_seen    TEXT NOT NULL,
  PRIMARY KEY (route, platform)
);
