# Device requests

The small service behind the window's "My device isn't here" field and the
"It worked" / "It did not work" buttons beside an asterisk. A Cloudflare
Worker on the free plan with a free D1 database. It counts requests per
device and reports per route and platform, and stores no IP address, no
user agent and nothing else (see `schema.sql`).

It takes only `POST /request` with one of two JSON forms:

- `{ "kind": "device", "device": "<1 to 80 characters>", "version": "0.7.4", "platform": "mac" | "windows" | "linux" }`
- `{ "kind": "report", "route": "kobo" | "tolino" | "remarkable" | "kindle-windows" | "kindle-linux", "worked": true | false, "version": "0.7.4", "platform": "mac" | "windows" | "linux" }`

Anything else is a 400, a body over 1 KB a 413, any other method or path a
404, and a sixth accepted request from one sender in an hour a 429. Tests:
`bun test tests/device-requests-worker.test.ts` from the repo root.

## Deploy (once)

Run these from this folder (`services/device-requests/`). Sign in first with
`bunx wrangler login`; no key ever needs to leave your machine.

1. Make the database:

   ```bash
   bunx wrangler d1 create screepub-device-requests
   ```

2. Paste the `database_id` it prints into `wrangler.toml`, in place of
   `PASTE-DATABASE-ID-HERE`.

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
