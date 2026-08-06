# Claude Desktop Manager

## Never bring up code signing

This app is for the author's own testing. It is not distributed, so signing, notarization,
Developer ID certificates, and Gatekeeper acceptance are all irrelevant here.

Do not suggest signing. Do not offer to add a signing step. Do not list it as an option, a
recommendation, a caveat, or a "real fix" next to a workaround. Unsigned releases are the
intended state, not a bug to route around.

When a Gatekeeper symptom does need explaining — "damaged and can't be opened", an arm64
build refusing to launch — say what fixes it locally (`xattr -dr com.apple.quarantine`,
`codesign --force --deep --sign -`) and stop there. No follow-up pitch.

## KISS — no god scripts, no god functions

One file, one responsibility. If describing a file needs the word "and", it is two files.
One function, one job. A function that orchestrates, does IO, and formats output is three
functions.

Applies to code you write. Do **not** refactor existing files to satisfy this unless asked.

## Single source of truth

Every fact is defined once and referenced everywhere else. Violations:

- **Constants and config** — a value lives in exactly one place and is imported. No magic
  number, path, or filename repeated across files; no value duplicated between code and config.
- **Logic** — no copy-pasted blocks. The *second* occurrence gets extracted, not the third.
- **Docs and comments** — prose never restates what code defines. No README listing flags the
  CLI already declares, no comment repeating a constant's value. It goes stale, and then it lies.
- **Types and schemas** — one definition per shape, derived across boundaries rather than
  hand-mirrored. The Rust core and the TypeScript frontend describe the same registry; that
  shape is defined once and generated, never typed out twice.

## File length

Scripts, build files, and one-off tooling: **200–250 lines max.** Past that, split.

Application source has no line cap — judge by cohesion instead. One reason to change per file.
