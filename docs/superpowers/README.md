# Plans and designs

How each piece of Method Reader was designed (`specs/`) and planned
(`plans/`), and the notes handed between sessions (`handoffs/`), kept for
the reasoning behind each piece. The code and its tests are the truth. Where
a document and the code disagree, the code is right and the document is
history.

## Which are history

- **Everything dated before 2026-10-11** describes the TypeScript engine, the
  Tauri window or the SwiftUI Mac app. All three were deleted when the Rust
  workspace took over (plan: `plans/2026-10-11-pure-rust-port.md`;
  `docs/retired-coverage.md` records what their checks covered). Paths in
  those documents, such as `src/`, `tests/*.ts`, `desktop/` or
  `tools/*.ts`, no longer exist. The decisions in them still hold unless a
  later document or the code says otherwise.
- **From 2026-10-11 on**, each document describes Rust code that exists. A
  plan is finished once its pull request merges, and its design then
  describes the code as it was built, not as it may have changed since.

Nothing here is moved or renamed: the code, the tests and the repo's other
docs link to these files by path.
