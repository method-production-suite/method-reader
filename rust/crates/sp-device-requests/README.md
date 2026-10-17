# Device requests

The small service behind Method Reader's "My device isn't here" field and the
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
own). Tests: `cargo test -p sp-device-requests` from `rust/` (the SQL
checks run on real SQLite, so they need the `sqlite3` tool).

This is the Rust edition of the service, built with workers-rs. It answers
every request byte for byte as the TypeScript Worker did, and its
`wrangler.toml` names the same Worker, database and rate limiter, so
deploying it **replaces the TypeScript Worker at the same address** with no
database change. **It is not deployed yet**: the live address still runs
the TypeScript Worker. The release design
(docs/superpowers/specs/2026-10-11-rust-release-design.md, part f) has the
details.

Wrangler, Cloudflare's own tool, is JavaScript (like Calibre, an outside
tool the platform requires), so the commands below run it with `npx`. Sign
in first with `npx wrangler login`; no key ever needs to leave your
machine. Run them from this folder, `rust/crates/sp-device-requests/`.

The Worker and its database keep the app's old name,
`screepub-device-requests`, after the rename to Method Reader: they are live
under it, and the window posts to that address. A new name would mean a new
Worker, a new database and a new window build, for nothing a person sees.

## Deploy the Rust Worker (owner, once, when wanted)

`wrangler.toml`'s `[build]` runs `worker-build` (pinned to 0.8.7, the
workers-rs release this crate uses), which compiles the crate for wasm32
and writes the JavaScript shim Cloudflare loads. It needs the wasm32
target: `rustup target add wasm32-unknown-unknown`. The first build also
downloads wasm-bindgen and esbuild.

1. Try it locally first, on Cloudflare's local simulator:

   ```bash
   npx wrangler dev
   ```

2. Deploy it. This replaces the live TypeScript Worker at the same address:

   ```bash
   npx wrangler deploy
   ```

3. If anything is wrong, go back to the TypeScript Worker:

   ```bash
   npx wrangler rollback
   ```

The schema and the one migration are unchanged, so the database needs
nothing.

## The 2026-10-02 database clean-up (done)

The first deploy (2026-10-02) made a database with an hourly table of
hashed senders and dates to the millisecond. This code has neither.
`migrations/0001-no-senders-day-dates.sql` dropped the senders table and
rounded the stored dates to their day, after the Worker that writes days
was deployed. The release design records it as already applied. To check
that only the two count tables are left:

```bash
npx wrangler d1 execute screepub-device-requests --remote --command "SELECT name FROM sqlite_master WHERE type = 'table'"
```

It should list `device_requests` and `route_reports` (and Cloudflare's
own `_cf_KV`, if it shows one), and no `rate_limits`.

## Set up from nothing (once, for a fresh account)

Done on 2026-10-02 for the owner's account. Since 0.13 the window posts to
the Worker's custom domain, `https://requests.methodreader.com`; the
Worker's workers.dev address still answers for 0.12 and earlier. After a change to `src/lib.rs` or `src/glue.rs`, only
step 4 is needed again; after a change to `schema.sql`, step 3 (every
statement in it is safe to run twice), or a new file in `migrations/` for a
live database.

1. Make the database:

   ```bash
   npx wrangler d1 create screepub-device-requests
   ```

2. Paste the `database_id` it prints into `wrangler.toml`, in place of the
   one there, and into `LIVE_BINDINGS` in `tests/worker.rs`, which holds
   `wrangler.toml` to the live service's values.

3. Make the tables:

   ```bash
   npx wrangler d1 execute screepub-device-requests --remote --file=schema.sql
   ```

4. Deploy:

   ```bash
   npx wrangler deploy
   ```

   It prints the address, `https://screepub-device-requests.<your-subdomain>.workers.dev`.

5. Give the Worker a custom domain on a zone the account holds (Workers >
   the Worker > Settings > Domains & Routes > Custom domain), put that
   address into `REQUEST_URL` in `rust/crates/sp-app/src/model/requests.rs`,
   and rebuild the window.

## Read the list

```bash
npx wrangler d1 execute screepub-device-requests --remote --command "SELECT spelling, count, first_seen, last_seen FROM device_requests ORDER BY count DESC"
npx wrangler d1 execute screepub-device-requests --remote --command "SELECT route, platform, worked, did_not_work, last_seen FROM route_reports ORDER BY route, platform"
```

Or in the Cloudflare dashboard: Storage & Databases, D1, the
`screepub-device-requests` database, then its Console or the table view.
