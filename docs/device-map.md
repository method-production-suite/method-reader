# Device map: every reading surface a screenplay EPUB can land on

Researched 2026-07-30 by a six-cluster parallel sweep (Kindle; Kobo/tolino;
PocketBook/Boox/Android e-ink; note-takers; legacy + Nook; reading apps +
OPDS), cross-referenced against Calibre's device-driver tree
(`github.com/kovidgoyal/calibre`, `src/calibre/devices/`). Market-share
figures are directional, not load-bearing. Spec:
`docs/superpowers/specs/2026-07-30-device-map-design.md`.

Companion registry: `docs/formatting-options-log.md` (formatting behaviors).
This doc is the **device and transfer-route registry**: which surfaces exist,
how a file gets there from a Mac, and what each surface does to a screenplay.

---

## 1. Transfer-method taxonomy

Every route a converted script can travel, and what each demands of a Mac app.
"Offline" means: works with no internet and no vendor account.

| # | Method | Offline | Mac-side detection | What the app must do |
|---|--------|---------|--------------------|----------------------|
| T1 | **USB mass storage** | yes | volume signature under `/Volumes` (never VID/PID alone: Linux-gadget chips collide, e.g. `0x0525/0xA4A5` is shared by ~8 brands) | copy file into the device's expected folder; eject matters on devices that index on unplug (Kobo) |
| T2 | **USB MTP** | yes | never mounts; IOKit/libusb enumeration only (e.g. Kindle VID `0x1949` with non-MSC interface) | macOS has no native MTP. Either link libmtp (Calibre does, `devices/mtp/unix`; Swift precedent exists) or hand off to a helper (Amazon Kindle USB File Manager, OpenMTP). **Since 2026-09-30: pure-Rust `mtp-rs` covers Mac, Windows and Linux with no C library; see §7** |
| T3 | **USB-Ethernet web upload** | yes | tablet is a CDC-ECM/RNDIS gadget; Mac gets `10.11.99.2/29`, device serves HTTP at `10.11.99.1` | reMarkable only. `GET /documents/` then `POST /upload` (multipart, field `file`). The GET is required: upload lands in the last-listed folder |
| T4 | **LAN web upload** | LAN-only (no internet, but needs shared Wi-Fi + app open on device) | probe a known port: Boox `:8085/api/device`, Supernote `:8089/` | POST multipart to the device's endpoint; discovery is /24 scan or user-pasted URL/QR (no mDNS on either) |
| T5 | **Vendor cloud ingest** | no | n/a (browser or vendor app) | Send-to-Kindle web (200 MB) / email (50 MB, approved-sender silent-drop trap) / Mac app; tolino webreader (25 GB); Send-to-PocketBook email; PocketBook Cloud; Kobo Dropbox/GDrive sync (model-gated). App can only open URLs, reveal files, or `open -a` a vendor app |
| T6 | **Local app handoff** | yes | `NSWorkspace` / bundle-id checks | `open -a Books`, `open -a "Send to Kindle"`, AirDrop via `NSSharingService`. The whole Apple-ecosystem story |
| T7 | **Local server pull** (HTTP one-shot + QR; OPDS 1.2) | LAN-only | app runs the server; devices come to it | a plain TCP listener needs NO entitlement or Local Network prompt (Apple TN3179) as long as it skips Bonjour. One OPDS 1.2 Atom feed reaches KOReader on every platform it runs on, plus Moon+, Librera, PocketBook app, Thorium, Yomu |
| T8 | **Folder/SD-card copy** (generic escape hatch) | yes | user picks any mounted volume/folder | Calibre's `folder_device` pattern: copy the file, done. Covers every dead MSC reader, SD sneakernet, and unknown future devices with zero per-brand code |

Rejected/dead routes, for the record: `mailto:` (cannot carry attachments,
the reason in-app email-to-Kindle died); Calibre's wireless "smart device"
protocol (undocumented binary protocol, only real client is KOReader, which
already speaks OPDS); Readarr (archived June 2025); Pocket-to-Kobo (Pocket
shut down July 2025).

---

## 2. Tier 1: hardware, by ecosystem

### 2.1 Kindle (~80% of global e-reader units)

Three transfer eras, all still in circulation:

- **MSC era (2007 to 2023):** Kindle 1 through Paperwhite 5 (2021), Kindle 11
  (2022), Oasis 3. Mounts as `/Volumes/Kindle`: `documents/` for books,
  `system/` alongside. Still the majority of the installed base (19 years of
  devices vs 5-to-10-year lifespans).
- **MTP era (2023 to now):** Scribe flipped to MTP at firmware 5.16.3; every
  model since Oct 2024 (base 2024, Paperwhite 12, Colorsoft, Scribe 3) is
  MTP-only and **invisible to volume scanning**. Only known libmtp PID:
  `0x1949:0x9981` (Scribe 32 GB); Calibre just matches vendor `0x1949`.
- **2026 lineup:** base Kindle $110, Paperwhite 12 $160/$200, Colorsoft
  $250-280, Scribe 3 family $430-630 (Scribe Colorsoft is the 11" flagship;
  arguably the best Kindle screen for a screenplay). Oasis discontinued.

Formats and the two invariants, stated precisely:

- **No Kindle reads sideloaded EPUB. Ever.** A USB-copied `.epub` is ignored
  entirely (not "unindexed": unread). USB formats: AZW3, MOBI, KFX, PDF, TXT.
  EPUB enters only via Send-to-Kindle, where Amazon converts it server-side
  with zero control over the result.
- **Enhanced Typesetting is a property of the format, not the route.**
  Sideloaded KFX gets ET (hyphenation, ligatures, Page Flip) and indexes;
  AZW3/MOBI never do. KFX cannot travel through Send-to-Kindle at all: it is
  USB-only. This corroborates registry §8b (KFX > AZW3 when the toolchain
  exists).
- Sideloads DO index, asynchronously (up to ~24 h on recent firmware; a book
  that crashes the indexer vanishes silently). Collections exist on-device
  but cannot be safely written over USB (`system/collections.json`, SHA1 of
  device-absolute path). `.apnx` page numbers: MOBI/AZW3 only, never KFX.
- Cover quirk: Amazon's servers delete covers on USB-sent non-Amazon books;
  on Colorsoft and newer this is deliberate.

Rendering (live KDP help pages plus device tests; the 2026.2 guidelines
PDF's Appendix B is stale and self-contradictory, so cite the web pages,
not the PDF): `max-width` ignored by both renderer generations; horizontal
margins in %, vertical in em, body margins 0 (§11.3.5); body `line-height`
unsettable (KFX line height is fixed; KF8 clamps near 1.2). Break and keep
CSS: **KFX honors `page-break-*` and `break-*` including `avoid`, and
`widows`/`orphans` from fw 5.12.3** (Kindle Previewer 3.35 and 3.36 added
them circa 2019; jhowell's device tests plus our own #5a and #8b passes
confirm) — with one hard trap: any `background-color` on html or body makes
the KFX converter synthesize a wrapper block of its own, and every keep in
the book then dies silently (MobileRead t=330798, where jhowell frames it
as keeps working on top-level blocks only). Authored nesting depth is NOT
the trigger — Method Reader's keeps sit two divs deep inside `section.scene` and
held on device (registry #8b, 2026-07-29) — so the actionable rule is the
root-background ban, not a flat DOM. KF8/AZW3 honors `always` only; there,
file splits remain the only hard break. The Publisher Font toggle protects
`font-family` only.

Method Reader needs: (a) keep the MSC volume path (correct today); (b) something
for MTP Kindles: IOKit detection of VID `0x1949` non-MSC + guided handoff to
Amazon's Kindle USB File Manager (bundled with Send to Kindle for Mac since
Nov 2024, macOS 12+) or OpenMTP, or a libmtp route of our own; (c) keep STK
affordances as the online path. For (b), §7 (2026-09-30) replaces "a libmtp
route of our own" with a pure-Rust helper, now that the window is Tauri.

### 2.2 Kobo (~10% global, ~45% Canada)

- **Lineup:** Clara BW / Clara Colour / Libra Colour (2024 wave, current),
  Elipsa 2E (10.3", the EPUB-native note-taker), Sage discontinued, Libra 2
  residual. No new 2026 hardware as of July; refresh expected H2.
- **Transfer:** T1 USB MSC, volume `/Volumes/KOBOeReader` containing
  `.kobo/`. **The `.kobo/version` file is the master fingerprint**: field
  `[0]` serial, `[2]` firmware, last field's final 3 digits = model id
  (`390` Libra Colour, `391` Clara BW, `393` Clara Colour, `690`/`691`/`693`
  are tolino-branded twins). This matters because modern Kobo and tolino
  **share USB PIDs** (`0x2237:0x4237` covers six devices across both brands).
  Drop files anywhere (subfolders fine); **the library imports on eject**,
  so eject is a required step to surface in UI copy. Old models (pre Glo HD)
  have SD slots; treat as T8.
- **Cloud:** Dropbox/GDrive sync exists but is model-gated and account-bound:
  an export destination, not a device route.
- **Formats:** EPUB (Adobe RMSDK renderer) and KEPUB (Kobo's own renderer,
  `.kepub.epub` double extension). KEPUB buys pagination, stats, Page Flip,
  faster turns; it costs hyphenation and justification quality, which a
  monospace ragged-right screenplay mostly does not need. **Default EPUB,
  offer KEPUB as opt-in** (current app behavior: KEPUB choice on the send
  block). `kepubify` (Go, static binary) is the clean sidecar if we generate
  KEPUB ourselves; Calibre never kepubifies for tolino devices, so KEPUB is
  Kobo-branded only.
- **Rendering (Kobo's epub-spec):** never style bare `div`/`span`/`p` (Kobo
  injects its own during processing): class-qualify every selector. Vertical
  margins in em only at 1-2; horizontal % is fine. Base font-size in px/pt,
  not %. `line-height` on body only, or omit. CSS page-breaks unreliable on
  e-ink: separate XHTML files are the only real page break (the same
  conclusion as Kindle's KF8/AZW3 renderer — NOT its KFX one, which honors
  the break CSS; §2.1 and §6).

### 2.3 tolino (~40% of DACH, tied with Kindle there)

Two generations that behave like different brands:

- **Gen A (Android/RMSDK era: shine 3, vision 4-6, epos, page 2):** plain
  USB MSC, **no database**: write into the `Books/` folder at volume root
  (current app behavior, correct). Windows labels `_TELEKOMTOLINO` /
  `FILE-CD_GADGET`; practical macOS tell is a root `Books/` with no `.kobo/`.
- **Gen B (2024+: shine 5, shine color, vision color):** rebadged Kobo
  hardware running tolino-skinned Kobo firmware 5.x. Mounts like a Kobo
  (`.kobo/` present, import-on-eject); distinguish via `.kobo/version` model
  id `690`/`691`/`693`. **Ship plain EPUB, never KEPUB, to tolino** (Calibre
  hard-codes the same rule).
- tolino webreader (`webreader.mytolino.com`, 25 GB) is the cloud route:
  browser upload only.

### 2.4 PocketBook (strong EU niche; Linux, not Android)

- **Lineup 2026:** Verse (Pro/Color), Era (Color/Lite), InkPad 4 / Color /
  X / One (10.3" + stylus). Older Touch Lux / Touch HD still common.
- **The cheap win of the whole map:** T1 USB MSC ("PC Link" prompt, then a
  plain volume), **EPUB3 is first-class native** (no conversion, best format
  breadth in the industry), and rendering honors publisher CSS. Detection:
  do NOT VID/PID match (unregistered Linux-gadget ids that change with
  PC-Link state). Volume signature: root `system/` directory + `Books/`
  (or `books/`) directory; write into `Books/`. Same shape as our tolino
  path. microSD on most models (T8).
- Cloud: Send-to-PocketBook email (`@pbsync.com`), PocketBook Cloud,
  Dropbox folder sync. All account-bound: export destinations only.
- Rendering: publisher CSS respected (user metrics live in editable configs,
  `system/reader/linespacing.cfg` etc.). Our invariants pass untouched.

### 2.5 Onyx Boox and the Android e-ink long tail (~7% and the only grower)

- **Lineup:** Palma 2/Pro (phone-shaped), Go series, Note Air5 C, Note Max
  (13.3"), Tab series. Android 13-15, NeoReader stock, Google Play on most:
  power users run KOReader/Moon+/Librera instead.
- **Transfer:** USB is **MTP-only** (T2): never mounts, invisible to Finder.
  The interesting route is **BooxDrop over LAN** (T4): unauthenticated HTTP
  on port 8085, `POST /api/storage/upload` (multipart, `file` + optional
  `dir`), default destination `/storage/emulated/0/Books/`, probe/discover
  via `GET /api/device` over the /24 (no mDNS). Dependency-free from Swift
  (URLSession + multipart). Caveats: user must open the BooxDrop app on
  device; cleartext on shared Wi-Fi; not cable-offline. Onyx's cloud
  (push.boox.com / eur.boox.com) is account-bound and its desktop app is
  Windows-only in 2026.
- **Long tail (Bigme, Meebook, Hisense phones, Mudita Kompakt, viwoods,
  Daylight DC-1):** all generic Android -> MTP (T2) or their own cloud apps.
  Rendering depends on whichever reader app the user installs: unknowable.
  Document-only tier; the OPDS/HTTP server route (T7) is what actually
  reaches them, via KOReader or any OPDS-capable reader.
- NeoReader rendering: historically weak CSS fidelity, improving (FW 4.2
  engine rebuild); Reading Themes let users stomp margins/line-height, so
  spacing must degrade gracefully under a hostile line-height.

### 2.6 Note-takers: reMarkable, Supernote

- **reMarkable (category leader, ~790k units/yr):** five current SKUs:
  Paper Pure 10.3" mono $399 (the new volume seller), Paper Pro 11.8" color
  $629, Paper Pro Move 7.3" $449, legacy rM1/rM2. PDF and EPUB only; the
  device converts EPUB into its own page model on ingest (layout bugs are
  re-import territory). EPUB gets user-adjustable typography that PDF never
  does, which is exactly Method Reader's bet. 226-229 PPI (below e-reader 300);
  rM2/Pure have no light.
  - **Route (T3, already ours):** `http://10.11.99.1` when the USB web
    interface is on. Three hardening facts the current code does not know:
    1. `POST /upload` lands in the **last-listed folder**, not root: issue
       `GET /documents/` first to pin the destination.
    2. The Mac side can be detected cheaply before any HTTP: an interface
       holding an address in `10.11.99.0/29` means cable present, so a probe
       failure means "web interface off," a distinct, actionable message.
    3. 100 MB upload cap (Paper Pro); the toggle does not reliably survive
       reboot; the server keeps running after unplug (stale-probe trap).
  - **Cloud route:** Connect is $3.99/mo and the free tier syncs only 50
    documents; reMarkable's own guidance beyond that is "use USB." Our
    USB-first bet is validated by the vendor.
- **Supernote (Ratta):** Manta 10.7" ~300 PPI (the best pure reading panel
  in the note-taker class), Nomad 7.8". EPUB first-class-ish (font, size,
  row spacing, margin controls; layout freezes once annotated). USB-C is
  **MTP** (T2, never mounts). The app route is **Browse & Access** (T4):
  device shows `http://<lan-ip>:8089/`, plain multipart upload, no auth
  documented, no mDNS: prompt for the IP shown on-device, cache it.
- Kindle Scribe belongs to §2.1 (best Kindle screen for scripts); Kobo
  Elipsa to §2.2 (the only note-taker where EPUB is unambiguously native).

### 2.7 Nook (alive; fragmenting into MSC vs MTP)

- B&N retired the pre-2014 fleet in 2024-25 (sideloading still works on
  them), discontinued GlowLight 4e; current: GlowLight 4 (2021), GlowLight
  4 Plus (2023), Tablet 9; two new devices promised for 2026.
- **MSC models (2009-2022 + GlowLight 4):** volume labeled `NOOK`. Folder
  rule from Calibre, verbatim: `NOOK/Books` for product id >= 0xD (2021+),
  `NOOK/My Files` for 2013-2019, `my documents` for the 2009 original.
  EPUB native (RMSDK family). Sideload quality is historically buggy
  (disappearing sideloads documented across GlowLights): set expectations
  in UI copy.
- **MTP models (2016 GlowLight Plus, 2023 GlowLight 4 Plus):** invisible to
  Finder; even Calibre still fails on the 4 Plus. Document the Android File
  Transfer/OpenMTP workaround; do not build.

### 2.8 Sony PRS and the legacy fleet (the USB-era ghosts)

- **Sony PRS (dead 2014, yet the largest legacy community: MobileRead Sony
  forum still active within weeks):** pure USB MSC, EPUB-native from the
  2008 PRS-505 firmware onward. Folders: `database/media/books` (PRS-505
  class) or `Sony_Reader/media/books` (PRS-T1/T2/T3). Volume labels "Sony
  Reader Main Memory" / "Storage Card". Caveat: on-device library indexes
  (`media.xml` / `books.db`) mean a bare file drop may not surface in the
  library UI on some firmware; say so rather than write their databases.
- **Bookeen (Diva line still faintly alive in EU):** folder `Books` or
  `eBooks`; RMSDK.
- **Everything else in Calibre's tree** (Trekstor, Hanlin/Astak/BeBook,
  Hanvon, iRiver Story, Ectaco jetBook, iRex, Entourage Edge, Teclast pile,
  Samsung SNE, Aluratek, bq, ~22 more in `misc.py`): dead brands, all plain
  MSC with a per-brand folder name. **Not worth per-brand code; entirely
  covered by the T8 generic folder-copy escape hatch**, which is the actual
  lesson of Calibre's `folder_device` + `user_defined` drivers.
- **RMSDK is one rendering family** (Sony, Nook, Bookeen, iRiver, Netronix
  rebrands, and Kobo's non-KEPUB path): CSS 2.1-era parser, honors
  `margin-left/right` in % and `text-indent`. Our existing CSS invariants
  are already RMSDK-safe; one Kobo-EPUB test target validates the whole
  legacy family.

---

## 3. Tier 2: reading apps

The fallback surface when hardware is absent or hostile. A Mac app has only
five handoff primitives: `open -a`, AirDrop, watched folders, a local
server, and browser-upload handoffs. Nothing here exposes an ingest API.

| App | EPUB fidelity for a screenplay | Ingest from a Mac | Verdict |
|-----|-------------------------------|-------------------|---------|
| **Apple Books** (macOS/iOS) | WebKit, best mainstream CSS: but the user's Justify setting overrides `text-align` and embedded fonts are ignored **unless the OPF carries `<meta property="ibooks:specified-fonts">true</meta>`** | `open -a Books` (Mac, offline); **AirDrop -> "Copy to Books"** (iPhone, offline, reliable); iCloud Books sync of non-purchased EPUBs is documented-flaky: do not build on it | Green *with* the meta; the one-line OPF change is the single highest-payoff item in this tier |
| **Kindle apps** (iOS/Android/desktop) | KFX conversion, ET asserts its own layout; same invariants as hardware | **No offline route exists, period.** USB sideloads never reach apps; only Send-to-Kindle syncs. `open -a "Send to Kindle"` (drag target; its Mac app also bundles the USB File Manager for MTP Kindles) or the 200 MB web uploader | Amber; be honest in UI that this route transits Amazon's cloud |
| **Google Play Books** | Chrome-derived but "weirdest engine" per MobileRead; post-conversion fidelity unverified | Browser upload only (100 MB/file, 1000 books, no API) | Lowest priority |
| **KOReader** (app + alt firmware on Kobo/Kindle-JB/PocketBook/reMarkable) | crengine; publisher-style switches on by default; embedded fonts honored; **best case in the whole map** | OPDS, calibre-wireless, WebDAV/FTP/SSH, USB | Green; the single highest-value T7 target |
| **Moon+ Reader Pro** (Android) | Overrides CSS by default; "Publisher View" restores it | OPDS, Dropbox, WebDAV | Green if docs say "enable Publisher View" |
| **Librera PRO** (Android) | Preserves embedded fonts, keeps monospace | OPDS, cloud drives (F-Droid build strips both) | Green |
| **ReadEra** (50M+ installs) | Imposes own typography; embedded Courier survives (default font mode "Embedded+Custom") | Local files/open-with only; no OPDS | Amber |
| **Lithium** (Android) | Any user font choice breaks CSS monospace | Local only | Red |
| **Thorium Reader** (desktop, EDRLab) | Readium, publisher styles by default; best a11y on macOS | Local open; OPDS | Green |
| **Yomu** (Mac/iOS) | Faithful; reads EPUB **and** MOBI/AZW3 (both our outputs) | iCloud, AirDrop, OPDS, calibre, browser | Green; sleeper pick for Apple users off Books |
| **calibre viewer** (desktop) | Honors book CSS | Local open | Green; calibre is also the middleman: content server serves OPDS at `/opds`, ingest via `calibredb add`, runs natively on macOS |

**The OPDS/HTTP finding (T7), distilled:** one plain HTTP one-shot download
endpoint with an on-screen QR code reaches every app above including the
OPDS-less ones (phone browser downloads, user taps open-with). Layering an
OPDS 1.2 Atom feed on the same listener adds KOReader on all its platforms
plus Moon+/Librera/PocketBook-app/Thorium/Yomu. Both are LAN-offline. On
macOS a bare TCP listener requires no entitlement and triggers no Local
Network prompt (TN3179) as long as we skip Bonjour and show the URL/QR
instead. What OPDS can never reach: Apple Books, Kindle anything, stock
Kobo/reMarkable firmware, Play Books.

---

## 4. The matrix

Families x transfer methods. ● = works today in the wild, ○ = possible with
caveats (see section), · = not available. Method Reader's current coverage is
marked in §5.

| Family | T1 MSC | T2 MTP | T3 USB-web | T4 LAN-web | T5 cloud | T6 app handoff | T7 OPDS/HTTP | T8 folder |
|---|---|---|---|---|---|---|---|---|
| Kindle pre-2024 | ● | · | · | · | ● STK | ○ via STK app | ○ KOReader-JB | ● |
| Kindle 2024+ / Scribe | · | ● | · | · | ● STK | ● USB File Mgr | · | · |
| Kobo | ● | · | · | · | ○ gated | · | ○ KOReader | ● |
| tolino Gen A | ● `Books/` | · | · | · | ● webreader | · | ○ KOReader-ish | ● |
| tolino Gen B | ● `.kobo/` | · | · | · | ● webreader | · | ○ (verify) | ● |
| PocketBook | ● `Books/` | · | · | · | ● email/cloud | · | ○ KOReader | ● |
| Onyx Boox | · | ● | · | ● :8085 | ● send2boox | · | ● PushRead/KOReader | ○ OTG/SD |
| Android e-ink tail | · | ● | · | · | ● various | · | ● via reader apps | ○ SD |
| reMarkable | · | · | ● 10.11.99.1 | ○ 3rd-party | ● Connect ($, 50-doc free cap) | · | ○ KOReader port | · |
| Supernote | · | ● | · | ● :8089 | ● cloud | · | · | ○ microSD |
| Nook MSC models | ● `NOOK/` | · | · | · | · | · | · | ● |
| Nook MTP models | · | ● (broken even in Calibre) | · | · | · | · | · | · |
| Sony PRS / legacy | ● | · | · | · | · | · | · | ● |
| Apple Books | · | · | · | · | ○ iCloud (flaky) | ● open -a / AirDrop | · | · |
| Kindle apps | · | · | · | · | ● STK only | ● STK app | · | · |
| Play Books | · | · | · | · | ● upload | · | · | · |
| KOReader (any host) | ○ | · | · | · | ○ WebDAV | · | ● OPDS | ● |

---

## 5. Gap analysis: Method Reader today vs the map

Support when this gap analysis was written (verified then against the Swift
app's code; the engine's `rust/crates/sp-device/` and `rust/crates/sp-engine/src/export/` carry all of it now):

- **Kindle MSC** via volume signature; AZW3 (ebook-convert), KFX (when the
  Calibre+plugin+Previewer toolchain exists), engine MOBI as fallback.
  **Since 2026-10-10** the AZW3 is Method Reader's own (`rust/crates/sp-azw3/`, a port of
  Calibre's KF8 writer, beta) and is made on every machine with no Calibre;
  Calibre is needed for KFX alone, and the MOBI is asked for by name.
- **Kobo** via `KOBOeReader` volume, EPUB with KEPUB choice on the send
  block; **tolino** via `Books/` folder copy; **reMarkable** via the USB web
  interface; **Apple Books** handoff; **Send-to-Kindle** export affordances
  (the email route was deliberately rejected: mailto: carries no attachment).

What the map says we are missing, tiered by leverage:

**Now (small, high value):**

1. **reMarkable upload-root bug:** `GET /documents/` before `POST /upload`
   (upload otherwise lands in the tablet's last-browsed folder). Plus
   interface-level detection (`10.11.99.0/29` on any interface = cable
   present, so "web interface off" becomes a distinct message) and a 100 MB
   guard. File: `rust/crates/sp-device/src/remarkable.rs`.
2. **Apple Books justification/fonts — DONE 2026-07-30:** emit
   `<meta property="ibooks:specified-fonts">true</meta>` in the EPUB OPF.
   One engine line; without it Books force-justifies ragged-right screenplay
   text the moment a user has Justify on. Shipped in `rust/crates/sp-epub/src/build.rs`
   (with the `ibooks:` prefix declared on `<package>` so epubcheck stays
   quiet); registry #6b.
3. **PocketBook:** volume signature (root `system/` + `Books/`), copy EPUB3
   into `Books/`. Reuses the tolino code path nearly verbatim; zero
   conversion; the best value-per-line in the map.
4. **Generic "copy to folder/volume" destination (T8):** Calibre's
   `folder_device` lesson. One affordance covers every legacy MSC brand, SD
   sneakernet, and future unknowns; strictly higher leverage than per-brand
   signatures.

**Next (moderate effort, clear demand):**

5. **Nook MSC:** volume `NOOK`, era folder rule (`NOOK/Books` 2021+,
   `NOOK/My Files` 2013-2019), expectation-setting copy about B&N's flaky
   sideload indexing.
6. **Sony PRS:** two folder layouts + the index caveat in UI copy. Largest
   legacy community, pure MSC, perfectly aligned with the offline-first
   ethos.
7. **MTP-era Kindle guidance:** detect VID `0x1949` non-MSC via IOKit and
   show a guided handoff (Amazon Kindle USB File Manager / OpenMTP) instead
   of silence. Every Kindle sold since Oct 2024 is invisible to our volume
   scan today. (The handoff is still the cheap first step; §7 is the real
   route.)
8. **AirDrop button** for iPhone -> Apple Books (offline, native,
   `NSSharingService`).
9. **KEPUB generation** via bundled `kepubify` for Kobo-branded devices only
   (never tolino; both suites pin this), opt-in, with the
   hyphenation/justification tradeoff noted.

**Later (bigger bets, decide deliberately):**

10. **Local HTTP one-shot + QR, then OPDS 1.2 on the same listener (T7):**
    the single move that reaches KOReader everywhere, the Android reader
    ecosystem, Thorium/Yomu, and Boox's PushRead, all LAN-offline with no
    entitlement cost if Bonjour stays off. This is the "everything else"
    route.
11. **BooxDrop upload (T4):** URLSession multipart to `:8085`, /24 probe or
    pasted URL. Covers the only growing non-Kindle ecosystem without
    touching MTP.
12. **Supernote Browse & Access (T4):** same shape as BooxDrop at `:8089`,
    user-supplied IP.
13. **libmtp linkage (T2):** the only true USB route for the whole Android
    e-ink world + MTP Kindles/Nooks. One native dependency to notarize;
    decide only if 10-12 prove insufficient. **Superseded 2026-09-30 by
    §7:** a pure-Rust library removes the native dependency, and MTP is
    the only offline route to KFX on a new Kindle, which 10-12 cannot
    reach at all. That moves it out of "only if 10-12 fail".

**Registry corrections to carry into docs/formatting-options-log.md:**

- Kindle invariant, precise form: sideloaded EPUB is unread (not merely
  unindexed). The second half this bullet used to carry — "ET ignores
  `widows`/`orphans`/`page-break-inside`, so scene-break integrity requires
  file splits, not CSS" — was the stale Appendix B talking, and is retired:
  KFX honors the keeps and the split minimums (§2.1, §6, registry #5a and
  #8b). File splits remain the right advice for kepub e-ink and for
  KF8/AZW3, NOT for KFX.
- Kobo: class-qualify every selector (Kobo injects bare `div`/`span`);
  vertical em margins only at 1-2; base font-size in px/pt, never %.
- reMarkable: EPUB is converted on ingest; user typography applies to EPUB
  only; empirically verify whether cue-indent horizontal % survives the
  device's own margin setting (open question flagged by research).

**Verification gaps flagged by the sweep** (worth a hands-on check before
building on them): Gen-B tolino macOS volume label (assumed `KOBOeReader`);
PocketBook native OPDS (probably absent); whether Kindle for Mac accepts
drag-in like the Windows app; PW5/Kindle-11 staying MSC on future firmware;
Amazon's EPUB conversion target (KFX vs AZW3) per title.

---

## 6. Fragmentation support matrix (researched 2026-07-30)

What each rendering family does with the break, keep and split-minimum
properties: the table to check any fragmentation decision against. §2.1
states the Kindle column in prose; the Method-Reader-side counterpart is the
registry's break entries (#5, #5a, #8b, #8c, #16, #17). "(tested)" means
measured on device — jhowell's reporting or our own passes; everything
else is a citation or, where marked, an inference.

The KF8/AZW3 column is about the FORMAT and its renderer, so it holds for
Method Reader's own AZW3 (`rust/crates/sp-azw3/`, 2026-10-10) exactly as it did for
Calibre's: the stylesheet travels into the AZW3 verbatim, keeps included,
and the KF8 renderer then ignores the keeps it always ignored (registry
#8b). What the writer cannot change is what this column says; which file
the ladder hands over is formats.ts's question.

Two things the cells are too narrow to carry. **Apple Books honors only
the OLD column spelling**, and drops both spellings when they share one
declaration block (BlitzTricks) — which is why our stylesheet emits
`-webkit-column-break-inside` in a rule of its own rather than beside the
modern property. And **kepub's NO is for the modern spelling**: its
renderer paginates with multicol, so the old spelling is the one form
that plausibly reaches it, but that is an inference nobody has tested and
t=346874 records only that kepub ignores break CSS.

| Property | KFX/ET | KF8/AZW3 | MOBI 6 | Kobo epub (RMSDK) | Kobo kepub e-ink | tolino | Apple Books |
|---|---|---|---|---|---|---|---|
| `break-inside: avoid` | YES (tested); dies book-wide if html/body carries a `background-color` | NO for text blocks (images only) | NO | NO | NO (modern spelling; old spelling untested) | = Kobo column by generation | via the column spelling only (BlitzTricks) |
| `break-before/after: always` | YES | YES | `<mbp:pagebreak/>` only | YES | NO (split files) | YES (Gen A) | YES |
| `break-before/after: avoid` | YES, fw-dependent (tested) | NO | NO | probably YES (inferred from RMSDK honoring book CSS; untested) | NO | likely YES (RMSDK, untested) | NO (WebKit lacks it) |
| `widows`/`orphans` | YES, reported from fw 5.12.3 | NO | NO | YES (t=328903; reported, not verified here) | unverified (patch-lore says its WebKit reads them) | = RMSDK | likely (WebKit, untested) |
| New XHTML file = page break | YES | YES | YES | YES | YES (the only reliable break) | YES | YES |

Sources: KDP Text Guidelines (reflowable) help topic GH4DRT75GWWAGBTU;
Kindle Previewer release notes 3.35 and 3.36; MobileRead t=330798 (avoid is
KFX-only; the `background-color` wrapper trap that kills every keep in the
book), t=328903 (RMSDK widows/orphans), t=346874 (kepub ignores break CSS,
split files instead); kobolabs/epub-spec; BlitzTricks, the Blitz
boilerplate's iBooks notes (Books needs the column spelling, alone in its
own block); clagnut.com/blog/2426 (WebKit lacks `break-after: avoid`). The
2026.2 guidelines PDF's Appendix B contradicts Amazon's own live pages and
lost. Not in that list: the **fw 5.12.3** widows/orphans threshold, which
comes from jhowell's device-test reporting rather than any Amazon artifact
— treat the exact version as approximate; the dated artifacts are Previewer
3.35 and 3.36.

---

## 7. MTP Kindles from the Tauri window (researched 2026-09-30)

**Since then (2026-10-12):** this was built as `rust/crates/sp-mtp` (the
`method-reader-mtp` helper) and the MTP route in `rust/crates/sp-device`, beta
and switched off until it has run on a real Kindle. The window is native
Rust now with the engine linked in, so there is no sidecar; the plan below
is kept as written.

Research only. Nothing below has been built or run here, and nobody on the
project has a 2024-or-newer Kindle to run it on.

**Why it matters.** Every Kindle sold since Oct 2024 is MTP-only (§2.1), so
the engine's volume scan never sees it and the README sends those owners
to email. Email cannot carry KFX (§2.1: KFX is USB-only), so today a new
Kindle can never get the Enhanced Typesetting output. MTP is the only
offline route to it, and the only one where the script never leaves the
machine. It is not a Mac-only gap either: on Windows an MTP Kindle shows
in Explorer with no drive letter, so a plain file copy cannot reach it
there. Linux was not investigated.

**The library: `mtp-rs`** (github.com/vdavid/mtp-rs, crates.io `mtp-rs`).

- MIT OR Apache-2.0. Pure Rust over `nusb` on Mac and Linux, with no
  libmtp, no libusb and no FFI. On Windows it drives the system's own WPD
  COM stack behind the same API, with no driver install.
- Active: v0.32.0 on 2026-08-28, 42 releases since 2026-02-20. A
  ready-made CLI ships beside it (`mtp-rs-cli`).
- Its tested-devices table lists the **Kindle Paperwhite 12 (2024)** as
  "Full support". That came from PR #2 (merged 2026-04-01), which sends
  `OpenSession` with transaction id 0 as the PTP spec says; the PR notes
  Kindles reject it otherwise, where Android phones tolerated the old
  behavior. The contributor confirmed it on a real Paperwhite on
  2026-04-17. The report does not say which host OS.
- Its Windows path was verified on a Pixel 9 Pro XL, not on a Kindle.

**The Mac catch: `ptpcamerad`.** macOS's camera-import daemon claims
MTP/PTP devices the moment they connect and holds them exclusively, so
every other app is locked out. No library gets around this; it is the
same wall for libmtp. `mtp-rs` reports it as `Error::is_exclusive_access()`,
and IORegistry's `UsbExclusiveOwner` names the holding process. Other
holders: Android File Transfer's agent, and any open Calibre or OpenMTP
(Calibre's forum: one program per MTP device at a time on Mac and Linux).

Shipped precedent: **Cmdr** (github.com/vdavid/cmdr), a Tauri file manager
by the same author. It runs `launchctl disable user/<uid>/com.apple.ptpcamerad`
plus `pkill -9 ptpcamerad` when an MTP device connects, `launchctl enable`
when it disconnects, and `enable` unconditionally at startup to recover
from a crash. No password prompt, and a toast tells the user it paused the
daemon. Cmdr is source-available, NOT open source: take the idea, not the
code. Method Reader could hold the pause for the length of one copy only, and
restore it in a `finally` and again at engine start. The cost to say in
the UI: while paused, Photos and Image Capture cannot import from a camera
or phone. This is Method Reader touching a system service, however briefly, so
it is a product decision, not an implementation detail.

**Where it goes.** Not in the Tauri shell: `desktop/` holds no logic, and
the engine owns every route (desktop/README.md). The fit is a small Rust
helper binary on `mtp-rs`, shipped as a second sidecar beside the engine
and called from `src/device/` the way `src/export/` calls Calibre. Roughly
two verbs: list MTP devices as JSON (vendor, product, serial, model, or
who holds exclusive access), and put a file at a device path. One more
signed binary in the bundle and no dylibs, built for Apple silicon only, as
the engine is.

**Kindle behavior to copy, from GPL-3 tools** (behavior reference;
Method Reader is MIT OR Apache-2.0 outside sp-azw3, so code from these must not
be copied anywhere else; a clean reading of their behavior is enough):

- **sync2kindle** (github.com/rupor-github/sync2kindle, Go, Kindle-only
  sideloading). Added Mac MTP on 2026-08-01, tested by its author on a
  Paperwhite 12 under macOS 26.6. Books go under `documents/` (its default
  is `documents/mybooks`). For EBOK-type books it extracts cover thumbnails
  into `system/thumbnails`, and it copies `.apnx` and `.sdr` alongside.
  Its Mac driver deliberately never calls `LIBMTP_Release_Device` because
  "explicit libmtp release can leave some Kindles unusable until
  re-plugged". That is libmtp behavior; whether closing an `mtp-rs`
  session does the same is untested and worth watching for.
- **Calibre's Kindle MTP driver**: 7.25 imports KFX from MTP Kindles, 8.7
  generates APNX page-number files for them, 8.10 fixed where those land.
  APNX is MOBI/AZW3 only, never KFX (§2.1).

**Rejected.** `libmtp-rs`/`libmtp-sys` (unmaintained since 2020, and it
would put LGPL libmtp and libusb dylibs in the bundle). `mtpkit` (one
release, LGPL, no public repository). `winmtp` (Windows-only WPD, which
`mtp-rs` already uses). `swift-mtp` (MIT, a clean libmtp wrapper, but
Swift) and `SwiftMTP-dev` (Swift, pre-alpha, one device ever transferred
a file). Amazon's USB File Manager and OpenMTP stay as the handoff
fallback in §5 item 7.

**Unproven.**

- The whole route on a real 2024+ Kindle from a Mac, with Method Reader's files.
- That sideloaded KFX over MTP gets Enhanced Typesetting and indexes. The
  format-not-route rule (registry §8b) predicts yes, and sync2kindle lists
  `.kfx` as supported over MTP, but our KFX verdict was measured on a
  mass-storage Kindle.
- The Windows path on a Kindle.
- Cmdr's `ptpcamerad` pause, which we have only read.

**Cheapest first test** when a 2024+ Kindle is at hand: install
`mtp-rs-cli`, pause `ptpcamerad` by hand, push a Method Reader KFX into
`documents/`, and check it opens with Enhanced Typesetting and appears in
the library. That answers the first two unproven items before any code is
written.
