-- For the database the first deploy made (2026-10-02). Run it AFTER the
-- Worker that no longer uses rate_limits is deployed, never before: the old
-- Worker writes to that table on every request. Safe to run twice, and on
-- a database made from today's schema.sql.
--
-- 1. The hourly table of hashed senders goes, with its index. The rate
--    limit is now Cloudflare's binding, which stores nothing here.
DROP INDEX IF EXISTS rate_limits_hour;
DROP TABLE IF EXISTS rate_limits;

-- 2. Every date becomes its day: "2026-10-02T10:00:00.000Z" to "2026-10-02".
UPDATE device_requests SET first_seen = substr(first_seen, 1, 10), last_seen = substr(last_seen, 1, 10);
UPDATE route_reports SET first_seen = substr(first_seen, 1, 10), last_seen = substr(last_seen, 1, 10);
