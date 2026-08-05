# Claude Desktop Manager

`SPEC.md` is the design authority. Read it before changing anything structural.

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
