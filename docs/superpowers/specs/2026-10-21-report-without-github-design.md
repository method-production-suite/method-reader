# Reporting a problem without a GitHub account

Status: Decided: option 1 (the owner, 2026-10-21); built. Reports go by
pre-filled email to hello@methodproduction.com, with GitHub kept as a second
link. What was built differs from the plan below in two places, both
recorded under "As built" at the end.

## The problem

The window offers two ways to say something went wrong, and both open a pre-filled
GitHub issue: Help > Report a Bug, and Report on a failed conversion. A GitHub account
is the price of either. Most people Method Reader is for (screenwriters, assistants,
readers of scripts) don't have one and won't make one to report a bad conversion. So
the reports we most need never arrive.

Asking for a device, and saying whether a reader worked, already need no account: the
window posts a small counted form to the device-request service (sp-device-requests).
That service stores counts and the day, never a person.

## What must hold, whatever the route

- **No script leaves the Mac without the person choosing it**, and the first version
  sends no script text at all. A screenplay is confidential by default.
- **The person sees exactly what is sent** before it goes, in the window, editable.
- **Nothing is sent on its own.** A report is one click on a button that says what it
  does.
- **No account, no sign-in.**
- The window's "online" copy stays true: it checks for updates on its own and does
  everything else only when asked.

## What a report carries

Built only from things the window already knows. No text from the script is
included:

- what the person typed (required, up to 2000 characters, as the issue body allows
  today);
- the app's version and the OS label (`os_label`, as the issue already carries);
- for a failed conversion, the refusal's code and message, which are the app's own
  words (for example `scanned: no text`);
- optionally, with a checkbox that is off by default: the conversion's shape, as
  counts. That means pages, scene headings, cues, dialogue blocks, dual-dialogue
  exchanges, pictures, and the parser's warnings by code. It never includes a name,
  a word or a font name. Font names are left out because a studio's custom font can
  identify it.

A screenshot or the PDF itself is not in the first version. If a report needs the PDF,
the reply asks for it, and the person decides then.

## The decision for the owner: where a report goes

### 1. An email (recommended)

"Report a problem" opens the Mac's mail app with a new message to the suite's address.
The subject is "Method Reader <version>: <code or 'a problem'>", and the body holds
the fields above, editable. The window also shows the address and a Copy button,
because some people have no mail app set up.

- For: no new service and no stored data on our side. The person can be answered,
  and the reply can ask for the PDF. It works offline, because the mail sits in the
  outbox. The engine already finds the Mac's mail app for the Send to Kindle route
  (`export/route_facts.rs`), so the window can say when none is set up.
- Against: it shows the person's email address to us (they can see that before
  sending). It needs an inbox someone reads.
- Needs: a real inbox behind the suite address. MOrg owns that.

### 2. A form to the device-request service

The window posts `{ kind: "problem", ... }` to sp-device-requests, which stores it in
D1. Someone reads them with wrangler.

- For: one click, with no mail app and no address shown.
- Against:
  - It is the first free text the service would store, so it needs a retention rule,
    spam handling and a way to read reports.
  - There is no way to answer the person, so it can't ask for the PDF.
  - It breaks the service's rule of storing counts only.
  - The privacy copy on the site would need a new paragraph.

### 3. Keep GitHub only

That changes nothing, and it leaves out the people the item is about.

**Recommended: option 1**, with GitHub kept as a second link ("or open an issue on
GitHub") for people who have an account. Option 2 can come later if email turns out to
lose reports.

## Built on option 1, in one PR

The engine owns the words; the window only draws them.

- **sp-app `model/feedback.rs`:** a `ProblemReport` holding the fields above, with:
  - `mailto_url()`, which keeps the URL within a length every mail app takes: cut the
    typed text, never the versions, as `new_issue_url` does today;
  - `issue_url()`, the current issue link, kept beside it.
- **The window:** the two report buttons open a small sheet. It holds the text box,
  the off-by-default counts checkbox with a "Show what's sent" line, then Send by
  email, Copy the address, and "or open an issue on GitHub".
  - Everything is accessible through AccessKit.
  - It uses the existing wording rules ("one word, one job").
- **The counts:** the engine adds a `shape` object of integers and fixed codes to the
  convert `--json` answer (pages, scene headings, cues and so on, and the parser's
  warning codes). The report copies only that object. A test pins that no string
  from the parse can reach it. The addition changes the frozen convert answers, so it
  goes through `cargo xtask parity --accept` with a PARITY.md entry.
- **Tests:** the mailto encoding (UTF-8, line breaks, the cut), the sheet through
  AccessKit, and a test that a report with the box unticked carries no counts.
- **Copy:** the site and README lines about reporting a problem, through MOrg for the
  site.

## Open questions

1. Which address: `hello@methodproduction.com` or a separate `reader@` one. MOrg
   knows what's real and who reads it.
2. The subject line: is the version enough to sort reports, or should it carry
   the platform too?

## As built (2026-10-21)

- **The counts come from a verb of their own, not the convert answer.**
  `method-reader shape <file.fountain> [--json]` tokenizes the script's
  `.fountain` and answers integers only: `pages`, `sceneHeadings`, `cues`,
  `dialogueBlocks`, `dualExchanges`, `pictures`, `transitions`. Two
  reasons. Adding a `shape` object to the convert answer would have moved
  every frozen convert row in the parity harness; a new verb moves only
  the four rows of the main help, which gains its line. And the script
  someone reports after a bad conversion still has its `.fountain`, which
  is what the book was made from, so the verb can count it at the moment
  the report is written. The window asks for it when the report sheet
  opens on a converted script; a failed PDF has no `.fountain`, so its
  report offers no counts.
- **Pages** are one more than the `= pg N` markers the `.fountain`
  carries, so the title page is not counted, and a page holding nothing but
  page furniture has no marker. With no marker at all the count is left
  out rather than guessed.
- **No warning codes.** The parser's warnings are sentences, not codes,
  and a library folder's `source.json` records only where the script came
  from, so the counts carry none.
- **No mail app.** The engine's route facts can name the app that handles
  `mailto:` links, but on a Mac there always is one (Mail, set up or not),
  so they cannot say "no mail app". The sheet says so when opening the
  message fails, which is what happens on Windows and Linux with nothing to
  open it, and the address and its Copy button are always on the sheet.
- **The address** is hello@methodproduction.com (open question 1). The
  subject is "Method Reader <version>: <code>", or "...: a problem", and
  the suite's inbox filters on the prefix "Method Reader " (open question
  2: the version is enough; the platform is in the body).
- **The URL** is held to 2000 bytes, cut in what the person typed and never
  in the versions or counts, and "Show what's sent" shows the body exactly
  as it will be sent, cut and all.
