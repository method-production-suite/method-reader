# Fountain input: partial support

PDF is Method Reader's primary path. `.fountain` input is **partially supported**:
the render pipeline runs in full and 18 of the 20 formatting options apply
normally, but two do not, because they are consumed in
`rust/crates/sp-fountain/src/serialize.rs`, upstream of the `.fountain` itself, where the
pipeline's durable artifact begins. One piece of Fountain syntax also renders
differently than another tool would render it.

| What | On `.fountain` input |
| --- | --- |
| `contdMode` | **Not applied.** `(CONT'D)` is already written into the cue text. Asking to strip it emits a warning on the CLI and in `--json` rather than silently doing nothing; re-convert the PDF to change it. |
| `rejoinSplitDialogue` | **Not applicable.** It repairs speeches that a PDF's pagination split across pages. Fountain has no pages, so there is nothing to rejoin. |
| Forced sluglines (`.BLACK`) | **Rendered as mini-slugs.** Method Reader writes its own secondary sluglines with Fountain's dot-force, so it reads a dot-forced heading back the same way: a bold uppercase line inside the current scene, with no section break and no TOC entry. A dot-forced line that also opens like a real slugline (`.INT. …`, `.EST. …`) still becomes a full scene heading. |

The forced-slugline row is a cost, not a feature. Fountain has one forcing
character for headings and Method Reader needs it for the secondary sluglines a PDF
gives it, so a hand-written `.THE BRIDGE` loses its place in the table of
contents. If you want a scene, write it as one.

The input guards are PDF-only too: Fountain input runs neither the scanned-PDF
check nor the not-a-screenplay check, and does not warn when a script has no
scene headings. Handing over Fountain is taken as the claim that it is a
screenplay.

None of this affects Method Reader's own use of the path. The app's preview window
re-renders a script it already converted, where the options were settled when
the PDF was read and the dot-forced lines are Method Reader's own. It matters when
you bring a `.fountain` from another tool.
