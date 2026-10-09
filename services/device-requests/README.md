# Device requests

The small service behind the window's "My device isn't here" field and the
"It worked" / "It did not work" buttons beside an asterisk. A Cloudflare
Worker on the free plan with a free D1 database. It counts requests per
device and reports per route and platform, with the day each was first and
last seen (the day, never the time). That is all it stores: no IP address,
nothing made from one, no user agent, and not the app version it is sent
(see `schema.sql`).

The rate limit is Cloudflare's Workers rate-limit binding (`[[ratelimits]]`
in `wrangler.toml`). It counts requests by the connecting address for a
60-second window, inside Cloudflare, and writes nothing to the database.
Cloudflare sees that address anyway, as it does for any site it serves. The
binding cannot hand a place back, so a request whose database write fails
still used one; with a one-minute window that costs a sender a minute at
most.

It takes only `POST /request` with one of two JSON forms:

- `{ "kind": "device", "device": "<1 to 80 characters>", "version": "0.7.4", "platform": "mac" | "windows" | "linux" }`
- `{ "kind": "report", "route": "kobo" | "tolino" | "remarkable" | "kindle-windows" | "kindle-linux", "worked": true | false, "version": "0.7.4", "platform": "mac" | "windows" | "linux" }`

Anything else is a 400, a body over 1 KB a 413, any other method or path a
404, and a sixth accepted request from one sender within a minute a 429
(per Cloudflare location: the binding counts in each data centre on its
own). Tests:
`bun test tests/device-requests-worker.test.ts` from the repo root.

## Bring the live service up to date (owner, once)

The first deploy (2026-10-02) made a database with an hourly table of
hashed senders and dates to the millisecond. This code has neither. Run
these from this folder, signed in with `bunx wrangler login`, IN THIS ORDER:
the old Worker writes to the table the migration drops, so the new Worker
goes first.

1. Deploy the Worker that uses the rate-limit binding and writes days:

   ```bash
   bunx wrangler deploy
   ```

2. Drop the senders table and round the dates already stored to their day
   (safe to run twice):

   ```bash
   bunx wrangler d1 execute screepub-device-requests --remote --file=migrations/0001-no-senders-day-dates.sql
   ```

3. Check that only the two count tables are left:

   ```bash
   bunx wrangler d1 execute screepub-device-requests --remote --command "SELECT name FROM sqlite_master WHERE type = 'table'"
   ```

   It should list `device_requests` and `route_reports` (and Cloudflare's
   own `_cf_KV`, if it shows one), and no `rate_limits`.

## Deploy (once, for a fresh account)

Done on 2026-10-02 for the owner's account: the Worker answers at
`https://screepub-device-requests.samuelsandweiss.workers.dev`, the address
the window posts to. After a change to `src/worker.ts`, only step 4 is
needed again; after a change to `schema.sql`, step 3 (every statement in it
is safe to run twice), or a new file in `migrations/` for a live database.

Run these from this folder (`services/device-requests/`). Sign in first with
`bunx wrangler login`; no key ever needs to leave your machine.

1. Make the database:

   ```bash
   bunx wrangler d1 create screepub-device-requests
   ```

2. Paste the `database_id` it prints into `wrangler.toml`, in place of the
   one there.

3. Make the tables:

   ```bash
   bunx wrangler d1 execute screepub-device-requests --remote --file=schema.sql
   ```

4. Deploy:

   ```bash
   bunx wrangler deploy
   ```

   It prints the address, `https://screepub-device-requests.<your-subdomain>.workers.dev`.

5. Put `<your-subdomain>` into the window, in two places, then rebuild it:
   `SUBDOMAIN` in `desktop/ui/requests.js`, and the same address in the
   `connect-src` of the CSP in `desktop/src-tauri/tauri.conf.json`. A test
   (`tests/device-requests-ui.test.ts`) fails if the two disagree.

## Read the list

```bash
bunx wrangler d1 execute screepub-device-requests --remote --command "SELECT spelling, count, first_seen, last_seen FROM device_requests ORDER BY count DESC"
bunx wrangler d1 execute screepub-device-requests --remote --command "SELECT route, platform, worked, did_not_work, last_seen FROM route_reports ORDER BY route, platform"
```

Or in the Cloudflare dashboard: Storage & Databases, D1, the
`screepub-device-requests` database, then its Console or the table view.
