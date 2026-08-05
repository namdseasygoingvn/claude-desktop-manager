# MCP server management (post-v1)

**Status: designed, not built.** v1 treats `claude_desktop_config.json` as opaque: the manager
window offers *Edit Config*, which opens the profile's config in the system editor and nothing
else. Everything below is the phase that follows, specified far enough to implement without a
second design pass. Nothing here changes the *Create* behaviour — a new profile's config is
still exactly `{ "mcpServers": {} }`, never seeded.

The whole feature is a set of Tauri commands over one file per profile:
`<profiles root>/<profile.dir>/claude_desktop_config.json`.

### The file cdm is editing

Verified against Claude Desktop **1.25927.0** (macOS, `app.asar` inspected read-only) and against
the two real configs on the development machine (`Claude/`, `Claude-Work/`), plus current MCP
documentation.

The app parses the file with a zod schema, and **that schema is `strip`, not `passthrough`**.
Claude Desktop therefore drops top-level keys it does not know the moment it rewrites the file.
The full top-level schema:

| Key | Type | Notes |
| --- | --- | --- |
| `mcpServers` | object, name → server entry | the only key cdm ever edits |
| `globalShortcut` | string | written by the app's own shortcut setting; **not in official docs** |
| `claudeAiUrl` | string | dev builds only |
| `features` | object of booleans | `isDxtEnabled`, `isDxtDirectoryEnabled`, `isLocalDevMcpEnabled`, `isUvSystemPythonEnabled`, `isMidnightOwlEnabled`, `isChicagoEnabled`, `isChicagoBatchOnly`, plus a boolean catch-all |
| `isHardwareAccelerationDisabled` | bool | |
| `isHardwareAccelerationAutoDisabled` | bool | |
| `isHardwareAccelerationAutoDisableNoticePending` | bool | |
| `isCoworkSdkDebuggingEnabled` | bool | |
| `isUsingBuiltInNodeForMcp` | bool | app-managed Node for MCP servers |
| `isDxtAutoUpdatesEnabled` | bool | |
| `dxtMaxTotalSizeMB` | number | |
| `deploymentMode` | `"3p"` \| `"1p"` | |
| `awaitingSignIn` | bool | |
| `managedConfig` | object | mirror of MDM / managed-preferences values |
| `coworkUserFilesPath` | string | present in both local configs |
| `coworkUserFilesPathUncRedirectedFrom` | string | Windows UNC redirect |
| `preferences` | large object | ~150 keys, app-owned; treat as opaque |
| `lastRemoteBootstrapPartial` | object | telemetry bootstrap cache |
| `lastRemoteBootstrapHost` | string | " |
| `lastRemoteDisableEssentialTelemetry` | bool | " |
| `lastRemoteDisableEssentialTelemetryHost` | string | " |

None of these except `mcpServers` are cdm's business. They are listed so the implementation can
prove it round-trips them (see below), not so it can offer UI for them.

#### `mcpServers` entry shape

The entry schema, verified in the binary, is exactly four keys and is also `strip`:

```jsonc
{
  "mcpServers": {
    "<server name>": {
      "command": "string",              // REQUIRED
      "args": ["string", ...],          // optional
      "env": { "KEY": "value" },        // optional, string → string only
      "extensionId": "string"           // optional, app-owned — see below
    }
  }
}
```

Consequences cdm must respect:

- **`command` is required.** An entry without it fails validation, and Claude Desktop's response
  is to drop that entry, keep the rest, and show a "Some MCP servers could not be loaded" dialog
  naming the offending keys.
- **`env` values must be strings.** Numbers and booleans are a validation failure, not a coercion.
- **`extensionId` is written by the app**, marking a server that came from an installed extension
  (`.mcpb`/`.dxt`). The app strips `extensionId`-bearing entries before persisting the server list
  from its own UI. **cdm must treat any entry carrying `extensionId` as read-only**: never copy it
  to another profile (the target does not have the extension installed), never rename it, and show
  it greyed with an "from extension" tag.
- **There is no `type`, `url`, `transport`, `headers`, `cwd`, or `timeout` key.** `Claude/`'s real
  config contains `"type": "stdio"` on its one server; that key is not in the schema and is
  silently discarded by the parser. cdm must not write it, and must not treat its absence as an
  error when reading.

#### Remote / HTTP / SSE servers

**`claude_desktop_config.json` cannot express a remote server in this version.** `command` is
required, and there is no URL field. Remote MCP is reached through the Custom Connectors UI, whose
state does not live in this file. The SSE and Streamable-HTTP client transports *are* bundled in
the app, and the managed (MDM) channel has a `managedMcpServers` array that does carry
`transport: http | sse | stdio`, `url`, `headers`, and `oauth` — but that channel is
`/Library/Managed Preferences/…plist` on macOS and the Policies registry hive on Windows, not the
per-profile config file, and it is out of cdm's scope entirely.

So: **cdm's MCP UI covers stdio servers only.** If a future Claude Desktop adds a URL-shaped entry,
cdm's round-trip rules (below) mean it survives untouched and merely shows as "unrecognised
transport — edit by hand".

#### How Claude Desktop itself writes the file

Relevant because cdm shares the file with a live app:

- `JSON.stringify(config, null, 2)` — two-space indent, key order as parsed, trailing newline absent.
- Written with `writeFile` after `chmod 0600`; **not** atomic, no temp-and-rename. Observed:
  `Claude-Work/claude_desktop_config.json` is `0600` (last written by the app),
  `Claude/claude_desktop_config.json` is `0644` (last written by hand).
- Written only on specific events: file missing at startup, global shortcut changed, hardware
  acceleration toggled, or the MCP server list changed from the app's own settings UI. It is *not*
  rewritten on every launch.
- The config is read **once at startup** and cached in the main process. Edits made while a profile
  is running do not take effect until that profile restarts.

cdm matches the app's conventions: two-space indent, LF, mode `0600` on write.

#### How the entry is actually launched

Useful for validation, and for explaining failures to the user:

- The server is spawned via cross-spawn with
  `env = { <safelist from Claude Desktop's own env>, ...entry.env, PATH: <computed> }`.
  The safelist is `HOME, LOGNAME, PATH, SHELL, TERM, USER` on POSIX and
  `APPDATA, HOMEDRIVE, HOMEPATH, LOCALAPPDATA, PATH, PROCESSOR_ARCHITECTURE, SYSTEMDRIVE,
  SYSTEMROOT, TEMP, USERNAME, USERPROFILE, PROGRAMFILES` on Windows. **A server does not inherit
  the user's full shell environment.**
- `PATH` is not the shell's `PATH`. Claude Desktop computes a union of the extracted login-shell
  PATH, `process.env.PATH`, and a glob-expanded candidate list:
  `~/.nvm/versions/node/*/bin`, `/usr/local/bin`, `/opt/homebrew/bin`, `/opt/local/bin`,
  `~/.cargo/bin`, `~/go/bin`, `~/.bun/bin`, `~/.deno/bin`, `~/Library/pnpm`, `~/.local/bin`,
  `~/bin`, `~/.volta/bin`, `~/.local/share/mise/shims`, `~/.asdf/shims`, `~/.pyenv/shims`,
  `~/.rbenv/shims`, `~/.orbstack/bin`, `~/.nix-profile/bin`, `/usr/bin`, and others.
- **`${PATH}` inside `env.PATH` is expanded** to that computed list. `"PATH": "/my/tools:${PATH}"`
  works. This is undocumented but verified in the shipped bundle, and cdm's validator must not
  flag `${PATH}` as a broken literal.

*(Everything in the two subsections above is read out of the shipped `app.asar` for one version.
It is behaviour, not contract — treat it as a strong hint for diagnostics, never as a rule cdm
enforces.)*

### Round-trip safety

cdm's controlling rule: **an edit changes only the bytes the user asked to change.** Claude
Desktop can add a top-level key in any release, and cdm must never be the reason a key disappears.

Concretely:

1. **Parse to `serde_json::Value`**, never to a fully typed struct. `Value::Object` is backed by
   `serde_json::Map`, and cdm enables the crate's **`preserve_order` feature**, which backs that
   map with `indexmap::IndexMap` — insertion order is retained on both read and write. Key order
   is therefore preserved for every key cdm does not touch, and new keys append at the end rather
   than being sorted into the middle of the user's file.
2. **Typed views are read-only projections.** The `McpServer { command, args, env, extension_id }`
   struct exists for the UI layer and for validation. It is produced *from* the `Value`, and is
   never the thing that gets serialised back. Where a struct must round-trip, it carries
   `#[serde(flatten)] extra: serde_json::Map<String, Value>` so unknown per-entry keys survive.
3. **Mutate in place.** Adding a server inserts one key into the `mcpServers` object. Editing a
   server replaces only the fields the form changed; a field the user left alone keeps its
   existing `Value`, including any unknown sibling key such as the stray `"type": "stdio"` in the
   author's own config. Removing a server removes one key.
4. **`mcpServers` is created if missing, never removed.** If the profile's config has no
   `mcpServers` key at all — the state `Claude-Work/` is actually in today — the first add
   inserts it. Removing the last server leaves `"mcpServers": {}` behind, matching what *Create*
   writes.
5. **Write atomically**: serialise with a two-space pretty printer to a temp file *in the same
   directory*, `fsync`, `rename` over the target, then `chmod 0600` (`rename` preserves the temp
   file's mode, so set it before the rename on POSIX and after on Windows). This is the same
   discipline the registry already uses, and it is strictly safer than Claude Desktop's own
   in-place `writeFile`.
6. **Refuse to write a file cdm could not parse.** If `serde_json::from_str` fails, every write
   operation is rejected with "this profile's config is not valid JSON — fix it with Edit Config
   first". cdm never reformats, never repairs, never overwrites a broken file with a fresh one.
   The one exception already in the spec stands: *Launch* recreates the config if the file is
   **absent**.
7. **Read-modify-write is not transactional across processes.** cdm re-reads immediately before
   every write and compares mtime+size against the copy the UI was populated from; a mismatch
   aborts the write with "the config changed on disk — reload". This is a courtesy, not a lock:
   Claude Desktop does not lock the file either.
8. **Warn when the profile is running.** MCP edits to a live profile take effect only on restart.
   The dialog says so and offers *Quit & Restart* for that profile. Editing is not blocked —
   unlike rename and delete, nothing breaks.

### Operations

All are Tauri commands taking a profile id. All are per-profile; there is no global MCP state.

| Command | Effect |
| --- | --- |
| `mcp_list(profile)` | ordered list of entries: name, command, args, env key names, source (`config` / `extension`), enabled/disabled, validation findings |
| `mcp_add(profile, name, entry)` | insert; fails if `name` already exists |
| `mcp_edit(profile, name, patch)` | field-level patch of one entry, preserving untouched fields |
| `mcp_rename(profile, old, new)` | rekey in place, preserving position in the map; fails if `new` exists |
| `mcp_remove(profile, name)` | delete the key; confirmation dialog, no trash (it is three lines of JSON, and disable exists) |
| `mcp_set_enabled(profile, name, bool)` | see below |
| `mcp_copy(source, target, names[], options)` | see *Copying* |

`mcp_list` never hides an entry it does not understand. An entry missing `command`, or carrying a
shape cdm has no UI for, is listed with its raw JSON shown read-only and a "not editable here"
badge. Deleting such an entry is still allowed.

#### Enable / disable

**Claude Desktop has no supported disable flag in this file.** There is no `disabled` or `enabled`
key in the entry schema, and the app's own Connectors toggle removes the entry from `mcpServers`
outright when it persists. (`disabledMcpServers` exists in the app, but it is a *Claude Code*
concept read from `~/.claude.json`'s per-project block — a different file, a different feature,
and not something cdm should write.)

So cdm needs its own sidecar, and it must not put anything Claude does not understand into the
config Claude reads — an unknown key would be dropped by the app's `strip` parser anyway, so the
sidecar is the only option that actually works.

**The sidecar lives in the manager's data directory, not in the profile:**

```
<manager data>/disabled-servers/<profile id>.json
```

```jsonc
{
  "version": 1,
  "servers": {
    "unityMCP": {
      "entry": { "command": "/opt/homebrew/bin/uvx", "args": ["--from", "mcpforunityserver", "…"] },
      "disabledAt": "2026-08-05T16:40:00Z"
    }
  }
}
```

Rationale for the location: the profile folder is Chromium's `--user-data-dir`, and while it
tolerates unknown files at its root (that is how `.cdm-profile` works), keeping the sidecar out of
it means *Delete* stays a single trash operation, a hand-copied profile folder carries no stale
cdm state, and a user inspecting their profile sees only Claude's own files. Keyed by profile
**id**, so a rename does not orphan it.

Semantics:

- **Disable** = move the entry out of `mcpServers` into the sidecar, verbatim (including `env`).
- **Enable** = move it back, at the end of `mcpServers`; if a live entry with that name has
  appeared in the meantime, the user is asked to keep the live one, keep the disabled one, or
  rename.
- The UI shows disabled servers in the same list, greyed, with an "off" toggle. They are otherwise
  fully editable — editing a disabled server writes the sidecar, not the config.
- **`env` is stored in the sidecar.** This is the one place cdm holds a secret at rest, and it is
  unavoidable: the alternative is silently destroying the user's credentials when they flip a
  toggle. It is not *storage of credentials cdm obtained*; it is temporary custody of a value the
  user already had in a plaintext file, put back on enable. The sidecar file is written `0600`.
  The confirmation on first use of Disable says exactly this.
- Reconciliation: a sidecar entry whose profile id is not in the registry is deleted at startup;
  a sidecar that has been hand-deleted just means nothing is disabled.
- Extension-sourced entries (`extensionId`) cannot be disabled through cdm — that is the
  extension's own toggle inside Claude Desktop.

### Copying servers between profiles

The reason this phase exists. Flow:

1. Pick a **source profile** and a **target profile** (the target defaults to the profile the user
   opened the panel from).
2. Pick a **subset of the source's servers** — checkboxes, multi-select, disabled servers included
   (they copy as disabled).
3. Choose how `env` travels (below).
4. Review a per-name conflict list, then copy.

The operation is a read of the source config, a merge into the target `Value`, and one atomic
write of the target. **The source is opened read-only and is never written.** Copying is not a
move. Extension-sourced entries are excluded from selection entirely.

#### Conflicts

Resolved per-name, defaulting to **Skip**, with a "apply to all" affordance:

| Choice | Behaviour |
| --- | --- |
| **Skip** | leave the target's existing entry untouched (default) |
| **Overwrite** | replace the target's entry wholesale, including deleting keys the source lacks |
| **Rename** | insert under `<name>-2`, incrementing until free; the suffix rule matches the folder-collision rule already used for profile names |

Name comparison is exact and case-**sensitive**: these are JSON object keys, not filenames.

#### `env` and the "no credential storage" non-goal

`env` is where API keys live in practice. The spec's non-goal is **"No credential storage. Profiles
hold no tokens; Claude Desktop owns its own auth."** — that is about *login sessions*, and it means
cdm does not run an auth flow, does not hold a keychain, and does not become a place secrets are
kept. A server's `env` is a different animal: it is plaintext in a file the user already owns, and
cdm is not its custodian.

The tension resolves by **never persisting secrets on cdm's own terms, and never moving them
without the user saying so**:

- **Default: copy structure, strip values.** `command` and `args` copy verbatim. `env` copies its
  **keys with empty-string values**, and the copied server lands **disabled** with a "needs
  credentials" badge, so an incomplete server never gets launched and fails confusingly. The user
  fills the values in the target and enables it.
- **Opt-in: copy values too.** A single checkbox, *"Also copy environment values (may include API
  keys)"*, unchecked every time — never remembered, never a preference. Checking it shows the
  affected variable names, `SOMETHING_API_KEY (from unityMCP)`, so the user sees precisely what is
  about to be duplicated. Values pass straight from source file to target file; cdm holds them in
  memory for the duration of the operation and never logs, caches, or writes them anywhere else.
- **Heuristic marking, not blocking.** Variable names matching `(?i)(KEY|TOKEN|SECRET|PASSWORD|
  CREDENTIAL|AUTH)` are flagged in the review list. Non-matching values are copied under the same
  rule as everything else — cdm does not pretend it can tell a secret from a setting.
- **No third mode.** cdm does not offer a "shared secrets store", a keychain integration, or a
  vault. That would be the non-goal, for real.

The debug CLI mirrors this: `cdm mcp copy <src> <dst> --server <name>` strips values by default and
requires an explicit `--with-env-values` to carry them.

### Validation

cdm validates statically, never by executing anything. Findings are advisory: they annotate rows
and warn on save, they do not block a write. The user is allowed to author a config cdm thinks is
wrong.

**Checked:**

- **JSON validity** of the whole file — the only *blocking* check, because cdm will not write over
  a file it cannot parse.
- `mcpServers` is an object, and every value under it is an object.
- **`command` present and a non-empty string.** This is the one that matters: Claude Desktop drops
  entries that fail it, and the user's only clue is a dialog at startup.
- `args`, if present, is an array of strings.
- `env`, if present, is an object whose values are all strings — the most common hand-edit mistake
  is a bare number or `true`.
- **Unknown keys inside an entry** (`type`, `url`, `cwd`, `disabled`, …) are surfaced as an info
  note: "Claude Desktop ignores this key." Not an error, and never removed.
- **`command` resolution**, advisory only. Absolute path → does the file exist and is it
  executable? Bare name → search cdm's own `PATH` *plus* the candidate directory list Claude
  Desktop itself uses. A miss shows "cdm could not find `npx` — Claude Desktop searches more
  directories than cdm does, so this may still work" rather than an error. `${PATH}` inside
  `env.PATH` is recognised and not flagged.
- **Duplicate names across the copy operation** — a target-side conflict check, described above.
- **Reserved-prefix names.** Claude Desktop refuses server names colliding with its internal
  servers (`claude_in_chrome`, `claude_browser`, `claude_preview`, `computer_use`, `plugins`,
  `skills`, `mcp_registry`, `scheduled_tasks`, `cowork`, `dispatch`, `remote_devices`, and
  others; matching is on the normalised name's prefix). cdm warns on add/rename/copy into one of
  these because the entry will be silently dropped at runtime. *This list is read from the shipped
  bundle and is version-specific — treat it as a warning source, never as a hard rejection.*

**Deliberately not checked:**

- Whether the server actually starts, speaks MCP, or responds to `initialize`. cdm spawns nothing.
- Whether an `env` value is a *valid* credential, or a credential at all.
- Whether `args` make sense for the command, or whether a package name exists on npm/PyPI.
- Whether a path inside `args` exists — servers legitimately take paths that are created later.
- Anything about `preferences`, `features`, or the other top-level keys. cdm's rule there is
  preserve-and-ignore.
- Schema conformance against a pinned Claude Desktop version. The validator encodes what the app
  requires *today*; when the app's schema grows, cdm's unknown-key handling means an unrecognised
  entry degrades to "shown, preserved, not editable here" rather than to data loss.

### UI

MCP servers become a section of the per-profile detail pane in the manager window — not a new
top-level view, and nothing appears in the tray. The tray stays a launcher.

The profile detail pane gains a server list under the existing actions. *Edit Config* stays exactly
where it is; the table is an easier path to the same file, never the only one.

```
┌─ Claude Desktop Manager ───────────────────────────────────────────────────┐
│                                                                            │
│  Profiles              │  Work (EU)                                        │
│  ────────────────────  │  ───────────────────────────────────────────────  │
│  Personal              │  Folder   Claude-Work-EU                          │
│ ▶Work (EU)             │  Created  5 Aug 2026     Last used  16:40 today   │
│  client/acme           │                                                   │
│  Research              │  [ Launch ]  [ Rename… ]  [ Delete… ]             │
│                        │                                                   │
│                        │  MCP servers (3)          [ + Add ] [ Copy from…] │
│                        │  ┌──────────────────────────────────────────────┐ │
│                        │  │ ●  unityMCP        uvx --from mcpforunity…   │ │
│                        │  │ ●  filesystem      npx -y @modelcontextp…    │ │
│                        │  │ ○  github          npx -y @modelcontextp… ⚠  │ │
│                        │  │    postgres        from extension  (locked)  │ │
│                        │  └──────────────────────────────────────────────┘ │
│                        │  ⚠ github: command not found on cdm's PATH        │
│                        │                                                   │
│                        │  [ Edit Config ]        Changes apply on restart. │
│  [ + New Profile ]     │                                                   │
└────────────────────────────────────────────────────────────────────────────┘
```

`●` enabled, `○` disabled, no dot = extension-sourced and read-only. A row click opens the editor;
the toggle is on the dot. `⚠` marks any row with a validation finding, explained in the strip
below the list.

Add / edit is one modal, the same shape for both:

```
┌─ Edit server — “github” ──────────────────────────────────┐
│                                                           │
│  Name     [ github                                     ]  │
│                                                           │
│  Command  [ npx                                        ]  │
│           ⚠ not found on cdm’s PATH — Claude Desktop      │
│              searches more directories, so this may work  │
│                                                           │
│  Args     ┌─────────────────────────────────────────┐     │
│           │ -y                                      │     │
│           │ @modelcontextprotocol/server-github     │     │
│           └─────────────────────────────────────────┘     │
│           [ + Add argument ]                              │
│                                                           │
│  Environment                                              │
│           ┌── name ──────────┬── value ──────────────┐    │
│           │ GITHUB_TOKEN     │ ••••••••••••    [👁]  │    │
│           └──────────────────┴───────────────────────┘    │
│           [ + Add variable ]                              │
│                                                           │
│  Enabled  [x]                                             │
│                                                           │
│                              [ Cancel ]   [ Save ]        │
└───────────────────────────────────────────────────────────┘
```

`env` values render masked with a reveal toggle. Masking is presentation only — the value is
plaintext in the profile's config either way, and the UI says so in the tooltip rather than
implying protection cdm does not provide.

*Copy from…* is the second modal:

```
┌─ Copy MCP servers ────────────────────────────────────────────┐
│                                                               │
│  From  [ Personal            ▾ ]     To  Work (EU)            │
│                                                               │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │ [x] filesystem    npx -y @modelcontextprotocol/serv…    │  │
│  │ [x] github        npx …          env: GITHUB_TOKEN      │  │
│  │ [ ] sqlite        uvx mcp-server-sqlite                 │  │
│  │ [x] unityMCP      /opt/homebrew/bin/uvx  ⚠ name exists  │  │
│  └─────────────────────────────────────────────────────────┘  │
│                                                               │
│  [ ] Also copy environment values (may include API keys)      │
│      Without this, variable names copy with empty values and  │
│      the server arrives disabled until you fill them in.      │
│                                                               │
│  Conflicts                                                    │
│      unityMCP already exists in Work (EU)                     │
│      ( ) Skip   ( ) Overwrite   (•) Rename → unityMCP-2       │
│                                                               │
│                             [ Cancel ]   [ Copy 3 servers ]   │
└───────────────────────────────────────────────────────────────┘
```

The conflict block appears only when there is a conflict, and lists one row per colliding name.
Checking *Also copy environment values* expands an inline list of the variable names about to be
duplicated, secret-looking ones marked, before the copy is allowed to proceed.

### Debug CLI additions

Same subcommand binary, same debug-only status:

| Command | Does |
| --- | --- |
| `cdm mcp list <profile>` | names, command, enabled, findings |
| `cdm mcp add <profile> <name> -- <command> [args…]` | `--env K=V` repeatable |
| `cdm mcp rm <profile> <name>` | remove |
| `cdm mcp enable\|disable <profile> <name>` | sidecar move |
| `cdm mcp copy <src> <dst> [--server <name>]…` | `--with-env-values`, `--on-conflict skip\|overwrite\|rename` |
| `cdm mcp check <profile>` | validation report, exit non-zero on invalid JSON only |

### Build order

Slots after step 4 of the existing plan, as step 5, in this order — each step is shippable alone:

1. Read-only: parse, list, validate, render the table. *Edit Config* still does the writing.
2. Write path: atomic write with unknown-field preservation, then add / edit / remove / rename.
3. Enable / disable and the sidecar.
4. Copy between profiles, including the `env` policy and conflict handling.

### Open questions this section does not settle

- **Windows verification.** Every byte-level observation here comes from the macOS `app.asar` of
  one version. The config path (`%APPDATA%\Claude\claude_desktop_config.json`) and the schema are
  assumed identical on Windows and are **UNVERIFIED** on hardware, alongside the existing Windows
  row in the platform table.
- **Version drift.** The entry schema is `strip`, which means a *future* Claude Desktop that adds
  a remote-transport key would still discard it from any config an *older* Claude Desktop rewrote.
  cdm cannot prevent that, and should not try; it only guarantees cdm is never the one doing it.
- **Whether disable should instead be "comment out".** JSON has no comments, and Claude Desktop's
  parser would reject `//`. Rejected; the sidecar stands.
