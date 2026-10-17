# Security policy

## Reporting a vulnerability

**Please do not open a public issue for a security problem.**

Report it privately through GitHub:
[**Report a vulnerability**](https://github.com/method-production-suite/method-reader/security/advisories/new).
That opens a draft advisory only you and the maintainer can see.

Please include your operating system and its version (macOS, Windows or
Linux), which Method Reader (the app or the command line), the Method Reader version
(the version stamp at the foot of the app's window, or `method-reader --version`),
and, if a specific file
triggers it, how to build a PDF that reproduces the problem. **Don't attach a
confidential script.** If a real one is the only reproducer, say so and we'll
work out a way to narrow it down without you sending it.

Expect an acknowledgement within a week. Fixes ship in a normal tagged
release, and you'll be credited in the release notes unless you'd rather not
be.

## What's in scope

The interesting attack surface is **the PDF parser**, because that's the one
place Method Reader handles a file it didn't create. A screenplay arrives from a
producer, an agency, or a stranger, and Method Reader opens it. The PDF reader is
Method Reader's own, written in pure Rust (`rust/crates/sp-pdf`), and every crate
in the workspace forbids `unsafe` code.

- **Malicious PDF leading to code execution, or reading or writing files
  outside the output path.** This is the one that matters most.
- **Malicious PDF making the engine hang or exhaust memory** on a file a
  reasonable person would call small.
- **Anything that causes a script to leave the machine without you asking.**
  Converting makes no network requests. Method Reader uses no network except
  checking for updates, which you can turn off (on a Mac it fetches the
  release's `latest.json` from GitHub, carrying only the app's name and
  version). Otherwise it reaches the network only when you ask it to: uploading to a docked reMarkable over USB (plain HTTP
  to the tablet's own web interface on the USB link), opening a
  page in your browser, installing the KFX plugin from Calibre's plugin
  index, and installing an update you chose.
  The [README](README.md#your-script-stays-on-your-machine) lists each one.
  A way to make it do anything else is a real finding.
- **The update path.** The app installs an update only after checking its
  minisign signature, and the version that signature names, against a key
  built into the app (`rust/crates/sp-update`). It fetches over HTTPS with
  rustls on the ring crypto provider. The older Mac app, retired but
  still on some Macs, checks the Apple Developer ID signature and the version
  before it replaces itself with the current app. A way to get either to
  install something else is a real finding.
- **Signing weaknesses.** The Mac app and the Mac command-line converter
  are signed with the hardened runtime and no entitlements, and notarized.
  If you find a way around that, we want to know.

## What's out of scope

- Bugs in [Calibre](https://calibre-ebook.com) or Amazon's Kindle Previewer,
  which are optional and which Method Reader runs only if you installed them
  yourself. Report those upstream.
- What Amazon does with a document after you send it to your Kindle by email
  or through Send to Kindle. That's between you and Amazon.
- The Windows downloads being unsigned. That is known and stated on the
  download pages.
- Vulnerabilities in a dependency with no path to exploitation through
  Method Reader, though if you're unsure whether a path exists, ask.
- A PDF that converts badly, produces garbled output, or crashes on
  malformed-but-harmless input. Those are ordinary bugs; please
  [open an issue](https://github.com/method-production-suite/method-reader/issues/new/choose).

## Supported versions

The latest tagged release, only, on macOS, Windows and Linux. Method Reader is a
small project maintained by one person; there are no long-term support
branches.
