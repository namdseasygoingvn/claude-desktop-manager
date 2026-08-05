# Claude Desktop Manager — Specification

**Status:** decided. This file is the architecture and the decision record; the detail lives in
`plan/`.

| Document | Covers |
| --- | --- |
| [`01-platform-adapter.md`](plan/01-platform-adapter.md) | binary discovery, launch, running detection, termination |
| [`02-implementation-tauri.md`](plan/02-implementation-tauri.md) | crates, tray, activation policy, detached spawn, atomic writes |
| [`03-user-interface.md`](plan/03-user-interface.md) | every screen, flow, error state and string |
| [`04-failure-modes.md`](plan/04-failure-modes.md) | adversarial audit — defects, contradictions, required edits |
| [`05-mcp-server-management.md`](plan/05-mcp-server-management.md) | config schema and server CRUD (post-v1) |
| [`06-distribution-and-updates.md`](plan/06-distribution-and-updates.md) | signing, notarization, updater, CI |

> **This document has not been reconciled with the audit.** `plan/04-failure-modes.md` finds 9
> internal contradictions here and specifies 14 edits, none of which are applied below. Where
> the two disagree, the audit is correct. In particular: `is_running()` as described in
> [Running detection](#running-detection) cannot work — Claude Desktop never creates
> `SingletonLock`.

## Purpose

Create, name, and launch isolated Claude Desktop profiles on macOS and Windows. Each profile
is its own login, its own MCP servers, its own extensions, its own history.

The tool is GUI-first and requires no manual setup. A CLI exists for debugging only.

## Non-goals

- **No credential storage.** Profiles hold no tokens; Claude Desktop owns its own auth.
- **No rotation, quota polling, or automated switching.** One profile, launched deliberately.
- **No touching the installed app.** Flags only — no re-signing, no patched binaries, no
  bundle duplication. App updates can never break us.
- **No config seeding.** Every new profile starts with an empty MCP server list. Copying
  servers between profiles is the user's job for now.
- **No import of the existing install.** The unmanaged `Claude/` folder is never read,
  copied, or modified. It keeps launching from the Dock as it always has.

## Core mechanism

A profile is a **display name** plus a **data directory**. Claude Desktop already does all the
isolation work; Electron's `--user-data-dir` switch is the entire mechanism. This tool is
bookkeeping and a UI around one flag.

```
launch(profile) → spawn(binary, ["--user-data-dir=<profile.dir>"])
```

Claude Desktop sets no Electron single-instance lock (verified in the macOS `app.asar`: no
`requestSingleInstanceLock`, no `second-instance` handler), so isolation rests on Chromium's
per-data-dir lock. Distinct data directories therefore mean genuinely independent processes,
and **any number of profiles run concurrently**.

## Product surface

Two views over one model.

**Menu bar / tray** — the everyday surface. Lists profiles; clicking one launches it. Also
holds *New Profile…* and *Manage Profiles…*.

**Manager window** — create, rename, delete, and per-profile details. Opened from the tray.
Closing it hides the window rather than quitting the app.

Profiles are launched **only** from the manager. No `.app` bundles or `.lnk` shortcuts are
generated, so there is nothing on disk to keep in sync with a rename.

### macOS activation policy

The app runs as `ActivationPolicy::Accessory` (tray only, no Dock icon) and switches to
`Regular` while the manager window is open, then back on close. Windows has no equivalent;
the tray icon is the only persistent presence.

### Single instance of the manager

`tauri-plugin-single-instance`. Launching cdm twice focuses the existing instance rather than
starting a second tray icon.

## Architecture

Two layers, and only one of them knows what OS it's on.

**Core** (Rust) — registry, naming, validation, profile lifecycle, reconciliation. All logic
lives here, fully platform-agnostic.

**Platform adapter** (Rust) — the complete surface of OS difference, four functions:

| Function | Returns |
| --- | --- |
| `find_claude_binary()` | path to the executable |
| `launch(binary, data_dir)` | spawn a detached process, yield its pid |
| `profiles_root()` | directory that holds `Claude-*` folders |
| `is_running(data_dir)` | whether a live process holds that directory |

Nothing else branches on platform. A third OS is one more adapter file.

**Frontend** (web) — presentation only. Every action is a Tauri command into core; the
frontend holds no business logic and no filesystem access.

### Platform table

|  | macOS | Windows |
| --- | --- | --- |
| Binary | `/Applications/Claude.app/Contents/MacOS/Claude` | `%LOCALAPPDATA%\AnthropicClaude\claude.exe` |
| Override | `CDM_CLAUDE_BINARY` | `CDM_CLAUDE_BINARY` |
| Profiles root | `~/Library/Application Support/` | `%APPDATA%\` |
| Manager data | `~/Library/Application Support/ClaudeDesktopManager/` | `%APPDATA%\ClaudeDesktopManager\` |
| Chromium lock | `SingletonLock` (symlink) | `lockfile` |
| Trash | `NSFileManager.trashItem` | Recycle Bin |
| Terminate | `SIGTERM`, escalate to `SIGKILL` | `WM_CLOSE`, escalate to `taskkill /F` |

> The Windows row is carried over from `PLAN.md` and has **not** been verified on hardware.
> Confirm the install path and the default `%APPDATA%\Claude` data dir before the first
> Windows build.

**Launch spawns the executable inside the bundle directly**, not via `open -n -a Claude`.
`open` returns immediately and discards the child pid, which we need for *Quit & Rename* and
*Quit & Delete*. Direct spawn means the app is not launched through LaunchServices; for an
Electron app this affects only Dock activation niceties, not isolation.

## Naming and folders

The folder is derived from the name the user types, prefixed with `Claude-`, matching the
layout the user already maintains by hand (`Claude/`, `Claude-Work/`).

The UI **always** displays the name as typed. The derived folder name is never shown except
in debug output.

### Slug algorithm

```
slug(name):
  1. Unicode NFC normalize
  2. replace each run of characters outside [A-Za-z0-9._-] and space with "-"
  3. replace spaces with "-", collapse repeated "-"
  4. trim leading/trailing "-", "." and whitespace
  5. truncate to 32 characters, re-trim
  6. if empty  →  "profile"
```

Folder is `Claude-<slug>`. Examples:

| Typed | Folder | Shown in UI |
| --- | --- | --- |
| `Work` | `Claude-Work` | Work |
| `Work (EU)` | `Claude-Work-EU` | Work (EU) |
| `client/acme` | `Claude-client-acme` | client/acme |
| `工作` | `Claude-profile` | 工作 |
| `Work` (second one) | `Claude-Work-2` | Work |

**The `Claude-` prefix eliminates Windows reserved names for free.** `CON`, `PRN`, `AUX`,
`NUL`, `COM1`–`COM9`, `LPT1`–`LPT9` can never be produced, because the folder never *equals*
the slug. Steps 3 and 4 handle the other Windows rules — no trailing dot or space, no path
separators, no reserved characters.

The 32-character cap exists for Windows. Chromium creates deep paths inside a user-data-dir,
and a long profile folder eats into `MAX_PATH` for every one of them.

### Collisions

Compare the candidate folder **case-insensitively** against existing folders — APFS and NTFS
are both case-insensitive by default, so `Work` and `work` collide. On collision, append `-2`,
`-3`, … until free.

Two different names can therefore share a folder stem (`Work (EU)` and `Work-EU` both slug to
`Work-EU`), and two profiles may share a display name. Neither is an error: the registry is
keyed by a stable id, not by name or folder.

## Registry

One JSON file at `<manager data>/registry.json`, the single source of truth for display names.
Never stored inside a profile.

```jsonc
{
  "version": 1,
  "profiles": [
    {
      "id": "p_7f3a2c",        // stable, internal, survives rename
      "name": "Work (EU)",     // display name, free-form, may duplicate
      "dir": "Claude-Work-EU", // current folder, relative to profiles root
      "createdAt": "2026-08-05T09:12:00Z",
      "lastUsedAt": "2026-08-05T16:40:00Z"
    }
  ]
}
```

Writes are atomic: serialize to a temp file in the same directory, `fsync`, then `rename` over
the target.

### Marker file and reconciliation

Each profile folder contains `.cdm-profile`, holding its `id`. Chromium ignores unknown files
at the root of a user-data-dir.

On startup, core reconciles registry against disk:

- Entry whose `dir` is missing → scan `Claude-*` folders for a `.cdm-profile` with that `id`;
  if found, repair `dir` (this is how a crash mid-rename heals). If not found, mark the entry
  **orphaned** and surface it in the manager window rather than deleting it silently.
- Folder with a `.cdm-profile` id absent from the registry → re-adopt it, deriving the display
  name from the folder stem.
- Folder without a `.cdm-profile` → ignore entirely. This is what protects the user's existing
  `Claude/` and hand-made `Claude-Work/` folders from being adopted without consent.

Adopting a hand-made folder is offered explicitly in the manager window, never automatic.

## Operations

### Create

1. Slug the name, resolve collisions, compute the folder.
2. `mkdir` the folder.
3. Write `.cdm-profile`.
4. Write `claude_desktop_config.json` containing `{ "mcpServers": {} }`.
5. Append to the registry.

A valid empty config is written rather than no file at all, so *Edit Config* always has
something to open. This is not seeding — the server list is empty.

Failure at any step rolls back the folder.

### Launch

1. `find_claude_binary()`; if absent, error pointing at `CDM_CLAUDE_BINARY`.
2. Ensure the folder and config file still exist; recreate the config if the user deleted it.
3. Spawn detached with `--user-data-dir=<folder>`.
4. Record the pid in memory and stamp `lastUsedAt`.

### Rename

1. Compute the new slug. If it equals the current folder, update the registry only and stop.
2. Require the profile not be running. If it is, offer **Quit & Rename**.
3. `fs::rename` the folder — same volume, atomic.
4. Update the registry.

If the process dies between 3 and 4, startup reconciliation repairs `dir` from `.cdm-profile`.
On Windows the move fails outright while Claude holds files open, which makes the running
check load-bearing rather than a courtesy.

### Delete

1. Confirm, naming the folder and stating that the login session is lost.
2. Require the profile not be running; offer **Quit & Delete**.
3. Move the folder to **Trash / Recycle Bin**, not an unlink. Recoverable if the user
   confirms the wrong row. Fall back to recursive delete only if the trash operation fails,
   and say so in the dialog.
4. Remove the registry entry.

### Quit

Terminate by pid when cdm launched the profile this session. When it did not (cdm was
restarted), locate the process by scanning for one whose command line contains the profile's
`--user-data-dir`. Escalate per the platform table if the process does not exit within a
few seconds.

### Running detection

`is_running(data_dir)` combines a known-live child pid with the Chromium lock file
(`SingletonLock` / `lockfile`). Required internally by rename, delete, and quit.

Whether the tray and window *display* running state is deferred, but the mechanism exists,
so showing it is nearly free.

## Debug CLI

Not the product surface. Ships in the same binary behind a subcommand so the core can be
driven without the GUI, primarily to prove launch and isolation on a fresh OS.

| Command | Does |
| --- | --- |
| `cdm create <name>` | make the folder, register it |
| `cdm list` | id, name, folder, running |
| `cdm launch <name>` | launch it; `--wait` to stay attached and stream stderr |
| `cdm rename <old> <new>` | move the folder and update the registry |
| `cdm delete <name>` | confirm, trash the folder, unregister |
| `cdm doctor` | binary discovery, registry/disk reconciliation report |

## Build order

1. **Core + adapter + debug CLI, no GUI.** Proves `--user-data-dir` isolation and concurrent
   launch on macOS and Windows. This is the milestone that de-risks everything else.
2. **Tray**: list profiles, launch, quit.
3. **Manager window**: create, rename, delete, adopt orphans.
4. **Polish**: reconciliation edge cases, trash, *Quit &* flows, activation policy.

## Open items

- **MCP config CRUD** — deferred by decision. Today the profile's config is opaque to cdm and
  edited by hand via *Edit Config*. Revisit once the tool is in daily use.
- **Surfacing running state** in the tray and window.
- **Updating cdm itself** — no updater specified.
- **Windows verification** — install path, default data dir, lock file name.
- **Copying servers between profiles** — the first thing likely to be wanted once several
  profiles exist, and the natural first step into config CRUD.

## Changes from PLAN.md

| Area | PLAN.md | Now |
| --- | --- | --- |
| Form factor | Node.js CLI | Tauri GUI; CLI is debug-only |
| Folder naming | generated id, decoupled from name | `Claude-<slug>`, derived from name |
| Rename | registry-only edit, no move | moves the folder; requires not running |
| Removal | unregister; delete dir behind a flag | one action, confirm, moves to Trash |
| Config seeding | open question | decided: never seed |
