# User interface

The governing constraint: **every flow completes with no terminal, no config file editing,
and no documentation.** Anything that would otherwise require one of those is either a button
or does not exist.

Two surfaces, one model. The tray is the everyday surface and is optimised for one click.
The manager window is where profiles are made and changed, and is optimised for being
understood by someone who has never seen it.

### Vocabulary

The UI has a small fixed vocabulary. These words, and no synonyms:

| Concept | Word used | Never say |
| --- | --- | --- |
| A profile | **profile** | instance, sandbox, workspace, container, environment |
| Its folder | *(not mentioned — see [Microcopy](#microcopy-principles))* | data directory, user-data-dir, profile path |
| The registry | **your profile list** | registry, database, index, JSON |
| Auth state | **signed in** / **signed out** | session, token, credentials, auth |
| Conversations | **chats** | history, conversations, threads |
| The app being launched | **Claude Desktop**, then **Claude** | the binary, the executable, the app bundle |
| Deletion target | **Trash** (macOS) / **Recycle Bin** (Windows) | trash can, bin, deleted items |

---

## 1. Tray menu

The tray icon is a template/monochrome glyph, present from launch to quit. Left-click (macOS)
or left/right-click (Windows) opens the menu. There is no tray popover UI, no custom-drawn
menu — a native menu, so it inherits every platform behaviour for free.

### Ordering

```
[ status row — only when something is wrong ]
──────────────
[ profile rows ]
──────────────
New Profile…
Manage Profiles…
──────────────
Quit Claude Desktop Manager        (macOS)   /   Exit   (Windows)
```

Profiles are sorted **alphabetically by display name**, locale-aware and case-insensitive,
ties broken by `createdAt` so two profiles sharing a name have a stable order.

> **DECIDED:** alphabetical, not most-recently-used. This menu is clicked many times a day and
> the target must not move under the cursor between clicks; MRU reorders the list every time
> it is used, which is exactly the wrong property for muscle memory.

> **DECIDED:** at more than 20 profiles the menu shows the first 20 and then a final row
> `More…`, which opens the manager. A tray menu taller than the screen is a broken menu.

### Running state

> **DECIDED: running state is shown in the tray**, as a leading bullet plus the word "Running"
> where the platform allows a secondary line, and always in the accessible label. Three reasons:
> the mechanism already exists (`is_running`), so this is nearly free; N profiles run
> concurrently, so "have I already got Work open?" is the single most common question this menu
> can answer; and without it, clicking an already-running profile looks like a dead click.

Clicking a running profile still invokes `launch()`. A spawn against a live data directory is
the OS's own "there is already one of these" path, and cdm treats it as a success either way —
it is never an error and never shows a dialog.

> **DECIDED:** re-launch rather than disable the row. A uniform click target ("every row
> launches") is simpler to learn than a row whose behaviour changes with state, and it avoids
> adding a fifth platform-adapter function for window focus. If Windows verification shows a
> second spawn against a locked directory produces a duplicate broken window rather than
> focusing the existing one, fall back to rendering running rows as disabled with the
> "Running" label — a one-line change, decided at that point.

### State: zero profiles (first run)

```
┌────────────────────────────────┐
│  No profiles yet               │   ← disabled
│ ────────────────────────────── │
│  New Profile…                  │
│  Manage Profiles…              │
│ ────────────────────────────── │
│  Quit Claude Desktop Manager   │
└────────────────────────────────┘
```

> **DECIDED:** on first run — the registry file does not exist — cdm opens the manager window
> immediately rather than sitting silently in the tray. A tray-only app with zero profiles is
> indistinguishable from a broken install, and the brief forbids sending the user to
> documentation to find out otherwise.

### State: N profiles, some running

```
┌────────────────────────────────┐
│  ● Work (EU)                   │   ← running
│    client/acme                 │
│  ● Personal                    │   ← running
│    Personal                    │   ← duplicate name, allowed
│ ────────────────────────────── │
│  New Profile…                  │
│  Manage Profiles…              │
│ ────────────────────────────── │
│  Quit Claude Desktop Manager   │
└────────────────────────────────┘
```

Two profiles named `Personal` appear as two identical rows. That is correct and is not
disambiguated with folder names, numbers, or dates.

> **DECIDED:** duplicate display names are not disambiguated in the tray. Any disambiguator
> would have to be the folder (forbidden) or an arbitrary counter (meaningless to the user).
> The manager window is where a user who has confused themselves goes to rename one.

### State: Claude binary not found

```
┌────────────────────────────────┐
│  ⚠  Claude Desktop not found   │   ← disabled, explanatory
│  Locate Claude Desktop…        │   ← the only enabled fix
│ ────────────────────────────── │
│    Work (EU)                   │   ← all dimmed / disabled
│    Personal                    │
│ ────────────────────────────── │
│  New Profile…                  │
│  Manage Profiles…              │
│ ────────────────────────────── │
│  Quit Claude Desktop Manager   │
└────────────────────────────────┘
```

Profiles remain listed — the user's profiles still exist and hiding them would read as data
loss — but are disabled, with the reason and the fix directly above them.

### State: registry unreadable

```
┌────────────────────────────────┐
│  ⚠  Profile list unavailable   │   ← disabled
│  Open Manager to Fix…          │
│ ────────────────────────────── │
│  Quit Claude Desktop Manager   │
└────────────────────────────────┘
```

`New Profile…` is **removed**, not disabled — creating a profile writes the registry, and cdm
must never write over a file it could not read and might still be able to recover.

### Quit

Quitting cdm does not quit running profiles; they are independent applications. There is no
confirmation, and no attempt to clean up.

> **DECIDED:** unconfirmed quit. Running Claude windows are the user's own apps with their own
> quit affordances; a "3 profiles are still running" nag would imply cdm owns them, which it
> deliberately does not.

---

## 2. Manager window

Master–detail. Profile list on the left, detail pane on the right, actions in the detail pane
next to the thing they act on. Closing the window hides it.

Default size 820 × 560, minimum 640 × 420, size and position remembered.

### macOS

Source-list sidebar, translucent, with the standard segmented footer control. The window uses
a unified toolbar with the window title only. Dialogs are **sheets** attached to this window.

```
┌───────────────────────────────────────────────────────────────────────────┐
│ ● ● ●              Claude Desktop Manager                                 │
├─────────────────────────┬─────────────────────────────────────────────────┤
│  PROFILES               │                                                 │
│                         │   Work (EU)                                     │
│  ● Work (EU)            │   Running · Last used today at 4:40 PM          │
│      Running            │                                                 │
│                         │   ┌───────────────────────────────────────┐     │
│    Personal             │   │                Launch                 │     │
│      Last used Tuesday  │   └───────────────────────────────────────┘     │
│                         │                                                 │
│    client/acme          │   ┌─────────┐ ┌──────────────────┐ ┌──────────┐ │
│      Never launched     │   │ Rename… │ │ Edit MCP Config… │ │ Reveal…  │ │
│                         │   └─────────┘ └──────────────────┘ └──────────┘ │
│                         │                                                 │
│                         │                                                 │
│                         │   ───────────────────────────────────────────   │
│                         │   Created 5 August 2026                         │
│                         │                                                 │
│                         │   Delete Profile…                               │
├─────────────────────────┤                                                 │
│  ⊞  ⊟  ⋯                │                                                 │
└─────────────────────────┴─────────────────────────────────────────────────┘
```

- `⊞` — New Profile. `⊟` — Delete (disabled with no selection). `⋯` — action menu containing
  *Add Existing Folder…*, *Rename…*, *Reveal in Finder*, *Copy Diagnostics*.
- `Delete Profile…` is plain destructive-red text at the bottom of the pane, away from
  everything else, never a prominent button.

### Windows

Title bar with system controls, a command bar under it, no sidebar translucency. Dialogs are
modal windows centred on the owner.

```
┌───────────────────────────────────────────────────────────────────────────┐
│  Claude Desktop Manager                                    ─   □   ✕      │
├───────────────────────────────────────────────────────────────────────────┤
│  ⊞ New Profile ▾   │                                                      │
├─────────────────────────┬─────────────────────────────────────────────────┤
│  Profiles               │                                                 │
│                         │   Work (EU)                                     │
│  ● Work (EU)            │   Running · Last used today at 4:40 PM          │
│      Running            │                                                 │
│                         │   ┌───────────────────────────────────────┐     │
│    Personal             │   │                Launch                 │     │
│      Last used Tuesday  │   └───────────────────────────────────────┘     │
│                         │                                                 │
│    client/acme          │   ┌─────────┐ ┌──────────────────┐ ┌──────────┐ │
│      Never launched     │   │ Rename… │ │ Edit MCP Config… │ │ Show…    │ │
│                         │   └─────────┘ └──────────────────┘ └──────────┘ │
│                         │                                                 │
│                         │   ───────────────────────────────────────────   │
│                         │   Created 5 August 2026                         │
│                         │                                                 │
│                         │   Delete Profile…                               │
│                         │                                                 │
└─────────────────────────┴─────────────────────────────────────────────────┘
```

- The `▾` on **New Profile** is a split-button dropdown containing *Add Existing Folder…*.
- Right-click on a list row gives a context menu on both platforms: *Launch*, *Rename…*,
  *Edit MCP Config…*, *Reveal in Finder* / *Show in File Explorer*, separator,
  *Delete Profile…*.

### Detail pane states

| Profile state | Header second line | Primary button | Notes |
| --- | --- | --- | --- |
| Never launched | "Never launched · not signed in yet" | `Launch` | plus hint: "Launch this profile and sign in to Claude. It won't affect any other profile." |
| Idle, used before | "Last used Tuesday at 9:14 AM" | `Launch` | |
| Running | "Running · last used today at 4:40 PM" | `Launch` | button stays enabled; see tray rationale |
| Launching | "Starting…" | `Launching…` (disabled, ~3 s) | reverts on timeout, no error |
| Orphaned | "Folder missing" | `Locate Folder…` | full flow in §3.7 |

> **DECIDED:** *Edit MCP Config…* is present but visually secondary, and labelled with MCP in
> it. MCP config CRUD is deferred, so this is the only hand-edit path; naming it explicitly
> stops it reading as a required setup step for a normal profile.

> **DECIDED:** single selection only, and a filter field appears above the list once there are
> more than 10 profiles. Multi-select would invite bulk delete, which is the one action that
> most deserves being done one at a time.

### Dialog button order — the platform difference

This is the thing most often got wrong, so it is specified once and obeyed everywhere.

**macOS** — buttons right-aligned in the sheet footer, affirmative action **rightmost**,
Cancel to its left:

```
                                              [ Cancel ]  [ Create ]
```

For **destructive** actions the destructive button is still rightmost, but **Cancel is the
default button** (pulsing/highlighted, activated by Return):

```
                                              [ Delete ]  [ Cancel ]
```

Note this reverses the *visual* order for destructive alerts: macOS alerts put the default
button at the right, so when Cancel is the default it moves right and Delete moves left. This
matches Finder's own "empty the Trash" alert.

**Windows** — buttons right-aligned in the dialog footer, affirmative action **leftmost**,
Cancel **rightmost**, always:

```
                                              [ Create ]  [ Cancel ]
                                              [ Delete ]  [ Cancel ]
```

The destructive button carries a red/critical accent. Initial focus is on **Cancel** for
destructive dialogs, so Return never destroys anything on either platform.

Copy shape also differs and is authored once, mapped twice: macOS alerts use **message text**
(bold, short, ends in a question mark for confirmations) plus **informative text**; Windows
task dialogs use **main instruction** plus **content**. The strings below are given as
*message / informative*, which map directly.

---

## 3. Flows

### 3.1 First run — the empty state

cdm launches, the tray icon appears, and the manager window opens unprompted.

```
┌───────────────────────────────────────────────────────────────────────────┐
│ ● ● ●              Claude Desktop Manager                                 │
├───────────────────────────────────────────────────────────────────────────┤
│                                                                           │
│                                                                           │
│                             No profiles yet                               │
│                                                                           │
│           A profile is a separate Claude Desktop — its own login,         │
│            its own MCP servers, its own chats. Nothing you already        │
│                  have in Claude Desktop is changed or moved.              │
│                                                                           │
│                       ┌───────────────────────┐                           │
│                       │     New Profile       │                           │
│                       └───────────────────────┘                           │
│                                                                           │
│                      Already have a folder? Add it…                       │
│                                                                           │
└───────────────────────────────────────────────────────────────────────────┘
```

Exact copy:

- Heading: "No profiles yet"
- Body: "A profile is a separate Claude Desktop — its own login, its own MCP servers, its own
  chats. Nothing you already have in Claude Desktop is changed or moved."
- Primary button: "New Profile"
- Secondary link, **shown only when adoptable folders were found**: "Already have a folder?
  Add it…"

The second sentence of the body exists because the first thing a user fears is that this tool
will touch their existing install. Saying so up front is cheaper than any amount of
reassurance later.

**Single obvious next action: New Profile.** It is the only prominent control on the screen.

### 3.2 Create a profile

1. User clicks `New Profile` (empty state, `⊞`, tray *New Profile…*, or ⌘N / Ctrl+N). From the
   tray this shows the manager window first, then the sheet.
2. Sheet / dialog:

```
   ┌─────────────────────────────────────────────────────┐
   │  New Profile                                        │
   │                                                     │
   │  Name    ┌───────────────────────────────────────┐  │
   │          │ Work                                  │  │
   │          └───────────────────────────────────────┘  │
   │                                                     │
   │  You'll sign in to Claude the first time you        │
   │  launch this profile.                               │
   │                                                     │
   │                            [ Cancel ]  [ Create ]   │
   └─────────────────────────────────────────────────────┘
```

   - Title: "New Profile"
   - Field label: "Name", placeholder: "Work"
   - Helper text: "You'll sign in to Claude the first time you launch this profile."
   - Buttons: macOS `[ Cancel ] [ Create ]`, Windows `[ Create ] [ Cancel ]`. Create is the
     default button; Return creates, Escape cancels.

3. `Create` is disabled while the field is empty or whitespace-only. There is no other
   validation message, ever — sanitization is silent and every non-empty string is a legal
   name.
4. If the typed name exactly matches an existing profile, an inline non-blocking note appears
   below the field: "You already have a profile named “Work”. That's allowed — they'll be
   separate." Create stays enabled.
5. On success the sheet closes, the new profile is selected in the list, and the detail pane
   shows the never-launched state with the `Launch` button focused.

> **DECIDED:** Create does not auto-launch. Setting up several profiles in one sitting is the
> common first-run pattern, and an Electron window stealing focus mid-setup is hostile. The
> `Launch` button being both primary and focused makes the next step unmissable without taking
> control away.

### 3.3 Launch a profile

**From the tray:** one click on the row. The menu closes. No dialog, no confirmation, no
progress window. The row's running bullet appears once `is_running` observes it.

**From the manager:** select the profile, click `Launch` (or press Return in the list).

- The button becomes `Launching…` and disables for up to 3 s, then reverts. This is feedback
  only — cdm does not wait on the child process and does not report timeout as failure.
- `lastUsedAt` is stamped, so the header's second line updates immediately.

> **DECIDED:** a transient disabled "Launching…" state rather than a spinner or a progress
> dialog. Claude Desktop takes seconds to draw its first window; a click with no acknowledgement
> reads as broken and produces double-launches. With reduced-motion enabled it is the same
> static label — there is no animation to suppress.

Failure paths are in §4.2.

### 3.4 Rename

Entry points: detail pane `Rename…`, context menu, ⌘R (macOS) / F2 (Windows).

**Not running:**

```
   ┌─────────────────────────────────────────────────────┐
   │  Rename Profile                                     │
   │                                                     │
   │  Name    ┌───────────────────────────────────────┐  │
   │          │ Work (EU)                             │  │   ← prefilled, selected
   │          └───────────────────────────────────────┘  │
   │                                                     │
   │                            [ Cancel ]  [ Rename ]   │
   └─────────────────────────────────────────────────────┘
```

Rename applies immediately; the list and tray update. Nothing is said about the folder.

**Running** — the dialog opens already showing the consequence, with the button relabelled:

```
   ┌─────────────────────────────────────────────────────┐
   │  Rename Profile                                     │
   │                                                     │
   │  Name    ┌───────────────────────────────────────┐  │
   │          │ Work (EU)                             │  │
   │          └───────────────────────────────────────┘  │
   │                                                     │
   │  ⚠  “Work (EU)” is running and has to quit before   │
   │     it can be renamed. Your chats are saved;        │
   │     anything in progress will stop.                 │
   │                                                     │
   │                     [ Cancel ]  [ Quit & Rename ]   │
   └─────────────────────────────────────────────────────┘
```

- Warning: "“Work (EU)” is running and has to quit before it can be renamed. Your chats are
  saved; anything in progress will stop."
- Button: "Quit & Rename"

> **DECIDED:** one dialog, not two. Showing the consequence inline before the user commits is
> both fewer clicks and more honest than a confirmation that ambushes them after they have
> already pressed Rename. If the profile starts running between the dialog opening and Rename
> being pressed, the same warning appears as a follow-up confirmation — that race is rare
> enough to deserve the clumsier treatment.

While quitting, the button becomes `Quitting…` and disables. If the process will not die, see
§4.9.

### 3.5 Delete

Entry points: detail pane `Delete Profile…`, context menu, `⊟`, ⌘⌫ (macOS) / Delete (Windows).

**Not running — macOS:**

```
   ┌─────────────────────────────────────────────────────┐
   │  Delete the profile “Work (EU)”?                    │
   │                                                     │
   │  You'll be signed out of Claude in this profile,    │
   │  and its chats, MCP servers and extensions go with  │
   │  it. Everything is moved to the Trash, so you can   │
   │  put it back until you empty the Trash.             │
   │                                                     │
   │                     [ Delete ]  [ Cancel ]          │
   └─────────────────────────────────────────────────────┘
                                       ↑ default
```

**Windows** — same copy with "Recycle Bin" for "Trash", and:

```
                                       [ Delete ]  [ Cancel ]
                                                     ↑ focused
```

Copy, authoritative:

- Message: "Delete the profile “Work (EU)”?"
- Informative (macOS): "You'll be signed out of Claude in this profile, and its chats, MCP
  servers and extensions go with it. Everything is moved to the Trash, so you can put it back
  until you empty the Trash."
- Informative (Windows): "…Everything is moved to the Recycle Bin, so you can restore it until
  you empty the Recycle Bin."

Every required element is present: the profile named as the user typed it, the specific loss
(the login), the collateral (chats, MCP servers, extensions), and the recoverability with its
expiry condition.

**Running:**

- Message: "Quit Claude and delete the profile “Work (EU)”?"
- Informative: "“Work (EU)” is running and has to quit first. You'll be signed out of Claude in
  this profile, and its chats, MCP servers and extensions go with it. Everything is moved to
  the Trash, so you can put it back until you empty the Trash."
- Button: "Quit & Delete"

**If the Trash operation fails**, a second dialog — this is the only case where a permanent
delete is possible, and it is always a separate, explicit decision:

- Message: "Couldn't move “Work (EU)” to the Trash."
- Informative: "It can still be deleted, but it won't be recoverable — there'll be nothing in
  the Trash to put back."
- Buttons: `[ Delete Permanently ] [ Cancel ]`, Cancel default/focused.

After a successful delete the selection moves to the next row (or the previous, if it was
last), and the tray updates. There is no undo affordance in cdm — the Trash is the undo, which
is precisely why the confirm says so.

### 3.6 Adopt an existing folder

Adoption is offered, never automatic.

**Discovery.** When `Claude-*` folders exist in the profiles root that have no `.cdm-profile`
and are not in the registry, the manager shows a dismissible banner above the list:

```
┌───────────────────────────────────────────────────────────────────────────┐
│  ℹ  2 folders here look like Claude profiles.   [ Review… ]         ✕     │
└───────────────────────────────────────────────────────────────────────────┘
```

- Banner: "2 folders here look like Claude profiles." / button "Review…"
- Dismissing hides it for good; the permanent entry point is *Add Existing Folder…* in the `⋯`
  menu (macOS) or the **New Profile ▾** split button (Windows).

**The sheet:**

```
   ┌───────────────────────────────────────────────────────────┐
   │  Add Existing Profiles                                    │
   │                                                           │
   │  These folders look like Claude Desktop profiles. Adding  │
   │  one lets cdm launch it. cdm puts one small marker file   │
   │  in each folder you add — nothing else is changed, and    │
   │  nothing is moved.                                        │
   │                                                           │
   │  ☑  Claude-Work        Name  ┌──────────────────────┐     │
   │                              │ Work                 │     │
   │                              └──────────────────────┘     │
   │  ☑  Claude-Personal    Name  ┌──────────────────────┐     │
   │                              │ Personal             │     │
   │                              └──────────────────────┘     │
   │                                                           │
   │                    [ Cancel ]  [ Add 2 Profiles ]         │
   └───────────────────────────────────────────────────────────┘
```

- Title: "Add Existing Profiles"
- Body: "These folders look like Claude Desktop profiles. Adding one lets cdm launch it. cdm
  puts one small marker file in each folder you add — nothing else is changed, and nothing is
  moved."
- Each row: the folder name (legitimately shown — the user is choosing between folders) and an
  editable Name field prefilled from the folder stem.
- Button label pluralises: "Add Profile" / "Add 2 Profiles". Disabled when nothing is checked.

> **DECIDED:** the bare `Claude/` folder is never listed as a candidate, on any surface,
> including *Add Existing Folder…*'s file picker, which rejects it with "That's your existing
> Claude Desktop. cdm leaves it alone on purpose — it keeps launching from the Dock as it
> always has." Adopting it would be the import of the existing install that the spec rules out.

> **DECIDED:** a candidate must contain at least one of `Local State`, `Preferences`, or
> `claude_desktop_config.json` to be offered, so an unrelated folder named `Claude-notes/` is
> never proposed as a profile.

> **DECIDED:** adoption never moves or renames the folder, even when the typed name would slug
> to something else. Adoption's whole promise is that nothing moves; and folder-vs-name
> divergence is already a legal state the registry handles by design.

### 3.7 Resolve an orphan

A registry entry whose folder cannot be found after reconciliation.

The list row is dimmed with a `Missing` badge, sorted in place (not to the bottom — it should
appear where the user expects to find it):

```
│    client/acme          │   client/acme                                   │
│      Missing        ⚠   │   Folder missing                                │
                          │                                                 │
                          │   cdm can't find this profile's folder in       │
                          │   Application Support. It may have been moved,  │
                          │   renamed, or deleted outside cdm. Its login    │
                          │   and chats live in that folder — cdm has no    │
                          │   copy of them.                                 │
                          │                                                 │
                          │   ┌────────────────────┐  ┌──────────────────┐  │
                          │   │  Locate Folder…    │  │ Remove from List │  │
                          │   └────────────────────┘  └──────────────────┘  │
```

- Header second line: "Folder missing"
- Body: "cdm can't find this profile's folder in Application Support. It may have been moved,
  renamed, or deleted outside cdm. Its login and chats live in that folder — cdm has no copy of
  them." (Windows: "…in the Roaming folder.")
- Primary: "Locate Folder…" — opens a directory picker starting at the profiles root.
- Secondary: "Remove from List".

**Locate outcomes:**

| Chosen folder | Result |
| --- | --- |
| Contains `.cdm-profile` with this id | Re-linked silently, row returns to normal. |
| Contains no `.cdm-profile` | cdm writes this profile's id into it and re-links. This is the "I renamed it in Finder" case, and the user just explicitly pointed at it. |
| Contains `.cdm-profile` with a different id | Refused: "That folder belongs to the profile “Personal”." One button, "OK". |
| Outside the profiles root | Refused: "Profiles have to live in Application Support. Move the folder there and try again." Buttons `[ Reveal Folder ] [ OK ]`. |

> **DECIDED:** Locate only accepts folders inside the profiles root, because the registry's
> `dir` is specified as *relative to profiles root* and there is no absolute-path form. The
> Reveal button makes the required move a two-drag operation rather than a instruction to read.

**Remove from List** confirm:

- Message: "Remove “client/acme” from your profile list?"
- Informative: "This only removes it from cdm. Nothing on your disk is deleted. If the folder
  turns up later you can add it back."
- Buttons: `[ Remove ] [ Cancel ]` (macOS, Cancel default) / `[ Remove ] [ Cancel ]` (Windows,
  Cancel focused).

---

## 4. Error states

Every error names the profile as the user typed it, says what happened in one sentence, and
offers exactly **one** recovery action. Secondary buttons are only ever `Cancel`, `OK`, or
`Copy Details`. No error codes appear in visible text; they go in the `Copy Details` payload.

### 4.1 Claude Desktop not installed / binary not found

Shown as a sheet the first time a launch is attempted, and as the tray status row thereafter.

- Message: "Can't find Claude Desktop."
- Informative: "cdm launches the copy of Claude Desktop already installed on this Mac, and
  couldn't find one." (Windows: "…on this PC…")
- Buttons: `[ Locate Claude Desktop… ]` primary, `[ Get Claude Desktop ]` secondary opening
  `https://claude.ai/download` in the default browser, `[ Cancel ]`.

`Locate Claude Desktop…` opens a file picker filtered to applications (macOS: `.app` bundles,
resolving to the executable inside; Windows: `.exe`). The chosen path is persisted.

> **DECIDED:** the picked path is written to cdm's own settings file in the manager data
> directory, and takes effect immediately without restart. `CDM_CLAUDE_BINARY` remains as the
> debug override and wins over the stored value when set. Telling a GUI user to set an
> environment variable is precisely the manual CLI setup the brief forbids.

If the picked file is not a Claude Desktop executable: "That doesn't look like Claude Desktop."
/ "Pick the Claude app in your Applications folder." — one button, "OK", picker reopens.

### 4.2 Launch failed

Spawn returned an error, or the child exited non-zero within 2 s.

- Message: "Couldn't launch “Work (EU)”."
- Informative: the OS error, one sentence, e.g. "Permission denied." — or, if nothing useful,
  "Claude Desktop closed straight away."
- Buttons: `[ Try Again ]` primary, `[ Copy Details ]`, `[ Cancel ]`.

If the failure is a missing binary, show §4.1 instead. If the failure is a missing folder, show
§4.7 instead. The generic dialog is the last resort, not the first.

### 4.3 Rename failed mid-move

Two sub-cases, and they are not equally interesting.

**Folder move failed, registry write would have succeeded.** cdm applies the name change and
says nothing.

> **DECIDED:** a failed folder move is not surfaced. The display name is the only thing the
> user sees or cares about; a folder whose name no longer matches its profile is a legal state
> that adoption already produces routinely. cdm records the mismatch for `doctor` and retries
> the move opportunistically the next time the profile is renamed. Reporting it would mean
> explaining folders to a user who has never been shown one, in order to describe a condition
> with no consequence.

**Registry write failed.** The name has not changed and nothing else can work either:

- Message: "Couldn't rename “Work”."
- Informative: "cdm couldn't save the change to your profile list."
- Buttons: `[ Try Again ]`, `[ Cancel ]`.

On any registry write failure cdm immediately re-reads and re-reconciles, so the visible state
always matches disk before the dialog appears. If the re-read also fails, escalate to §4.6.

### 4.4 Delete failed

**Trash unavailable / trash operation failed** — the permanent-delete dialog in §3.5.

**Permanent delete also failed** (files in use, permissions):

- Message: "Couldn't delete “Work (EU)”."
- Informative: "Some of its files are still in use. Make sure Claude isn't running for this
  profile, then try again."
- Buttons: `[ Quit & Delete ]` if still detected running, otherwise `[ Try Again ]`; plus
  `[ Cancel ]`.

**Partially deleted** (some files trashed, some not) — the registry entry is kept, the profile
appears as an orphan or as a normal profile depending on what survived, and reconciliation
decides. The user is told once:

- Message: "“Work (EU)” was only partly deleted."
- Informative: "Some of its files couldn't be removed. cdm has left the profile in your list so
  you can try again."
- Button: `[ OK ]`.

### 4.5 Disk full during create

Create rolls the folder back, then:

- Message: "Couldn't create “Work”."
- Informative: "There isn't enough free space on Macintosh HD. Free up some space and try
  again." (name the volume when it can be determined; otherwise "…on this disk.")
- Button: `[ OK ]`.

> **DECIDED:** the New Profile sheet stays open with the typed name intact rather than closing
> back to the list. Making someone retype a name they just typed is the most avoidable
> annoyance in the whole app.

Same treatment for any create failure — permissions, read-only volume — with the informative
line swapped for the actual reason.

### 4.6 Registry corrupt or hand-edited

cdm never overwrites a registry it could not parse.

The manager window opens (or comes forward) showing a full-pane error, not a dialog — there is
nothing else the window can usefully show:

```
┌───────────────────────────────────────────────────────────────────────────┐
│                                                                           │
│                     cdm can't read your profile list                      │
│                                                                           │
│        The file that stores your profile names is damaged. Your           │
│        profiles themselves are untouched — cdm just can't tell            │
│        which is which.                                                    │
│                                                                           │
│        Rebuilding finds your profiles again. Some names may come          │
│        back looking different, and you can rename them.                   │
│                                                                           │
│              ┌────────────────────┐   ┌────────────────────────┐          │
│              │   Rebuild List     │   │  Show the Damaged File │          │
│              └────────────────────┘   └────────────────────────┘          │
│                                                                           │
└───────────────────────────────────────────────────────────────────────────┘
```

- Heading: "cdm can't read your profile list"
- Body: "The file that stores your profile names is damaged. Your profiles themselves are
  untouched — cdm just can't tell which is which."
- Second paragraph: "Rebuilding finds your profiles again. Some names may come back looking
  different, and you can rename them."
- Primary: "Rebuild List" → renames the bad file to `registry.json.damaged-<timestamp>`, then
  reconstructs from disk: every `Claude-*` folder with a `.cdm-profile` becomes a profile named
  from its folder stem.
- Secondary: "Show the Damaged File" → reveals it in Finder / File Explorer.

The rebuild is the one place a folder stem legitimately becomes a display name; the copy warns
about it in advance in plain words rather than explaining folders.

**Unreadable for I/O or permission reasons** (not corrupt) is a different message with a
different fix:

- Heading: "cdm can't open your profile list"
- Body: "Something is stopping cdm from reading its own settings." + the OS error.
- Primary: "Try Again". Secondary: "Show the File".

### 4.7 Profile folder deleted behind cdm's back

Detected at startup (reconciliation) or at launch time. Both routes land in the orphan flow,
§3.7. If it is discovered at launch time, the manager comes forward with that profile selected
and the orphan detail pane showing — the click is answered with the explanation and the fix,
never with a bare error dialog.

A folder that exists but has lost its `.cdm-profile` marker or its config file is **not** an
error: launch rewrites both silently.

### 4.8 Trash unavailable

Covered by §3.5's fallback dialog. cdm does not pre-check trash availability, because the check
is not reliable and a warning about a failure that has not happened is noise.

### 4.9 A profile won't quit

After `SIGTERM` / `WM_CLOSE`, cdm escalates per the platform table. If the process is still
alive 5 s after escalation:

- Message: "“Work (EU)” isn't quitting."
- Informative: "cdm asked Claude to quit and it hasn't. You can force it to close, but anything
  in progress will be lost."
- Buttons: `[ Force Quit ] [ Cancel ]`, Cancel default/focused.

Cancelling abandons the rename or delete entirely and leaves the profile as it was.

### 4.10 Copy Details

Every error dialog with a non-obvious cause offers `Copy Details`, which puts a plain-text
block on the clipboard: cdm version, OS version, the operation, the profile's id, **folder
name**, resolved binary path, and the raw OS error. This is the diagnostic escape hatch that
replaces "run this command in Terminal".

---

## 5. Microcopy principles

### The folder never leaks

`Claude-Work-EU` is an implementation detail of a mechanism the user did not choose and cannot
change. Sanitization is silent; therefore the sanitized result must be invisible, or silence
becomes a lie the moment the user sees `工作` rendered as `Claude-profile`.

**The folder name must never appear in:**

- the tray menu, in any state
- the profile list or detail pane
- create, rename, or delete dialogs
- success or confirmation copy
- the empty state
- notifications

**The folder name may appear in:**

| Surface | Why it is allowed |
| --- | --- |
| `cdm doctor` and the debug CLI | Not the product surface; the audience is someone debugging. |
| `Copy Details` payloads | Diagnostic text, invisible unless deliberately fetched. |
| The Add Existing Profiles sheet | The user is choosing *between folders*; there is nothing else to identify them by. |
| The Locate Folder picker and its error messages | The user is operating the filesystem directly. |
| Reveal in Finder / Show in File Explorer | cdm is not displaying the name — the OS is, in its own window. |
| The rebuilt-registry warning | Only as the advance warning "some names may come back looking different", never as a literal folder string. |

Filesystem *locations* are a separate question from folder *names*. cdm may say "Application
Support" or "the Roaming folder" when the user must go there, and should — that is orientation,
not implementation detail. It still does not say `~/Library/Application Support/Claude-Work-EU`.

### Other rules

- **Name the profile in every destructive or failing message**, in typographic quotes, exactly
  as typed: `“Work (EU)”`. Never "this profile", never "the selected item".
- **No invalid names exist.** The only refusal is an empty field, and it is expressed by
  disabling the button, not by an error. Never write "invalid", "illegal", "not allowed" about
  a name.
- **One sentence for what happened, one for what to do.** If a third sentence is needed the
  design is wrong.
- **Say the loss, not the operation.** "You'll be signed out" beats "the data directory will be
  removed".
- **Say the recovery and its expiry.** "…until you empty the Trash" is load-bearing; without it
  "moved to the Trash" reads as permanent to some users and as forever-safe to others.
- **Never blame.** No "you deleted", no "the file you edited". §4.6 says "damaged", not
  "hand-edited", even when it plainly was.
- **Title case for buttons and menu items** on both platforms; **sentence case for all body
  text**. One shared string table, no per-platform casing fork.
- **Platform noun swap is the only per-platform string difference**: Trash/Recycle Bin,
  Finder/File Explorer, Mac/PC, Application Support/the Roaming folder, Quit/Exit. These are
  the only entries in the platform string map.
- **Never mention cdm's internals by name**: no "registry", "reconciliation", "orphan", "slug",
  "marker file" (except the one plain-words mention in the adopt sheet: "one small marker
  file"), "user-data-dir", "pid".

---

## 6. Keyboard and accessibility

### Tray

- Accessible label on the icon: "Claude Desktop Manager, 3 profiles, 1 running". Recomputed on
  every state change.
- Accessible label per row: `"<name>, running"` or `"<name>"`. The running bullet is decorative
  and hidden from assistive technology — the state is in the text, never in colour or glyph
  alone.
- Status rows carry their own labels: "Claude Desktop not found, unavailable".
- Reachable by keyboard through the platform's own path: macOS VoiceOver menu-extras
  (Ctrl+F8, arrows, Return); Windows Win+B to the notification area, arrows, Enter or the
  Menu key. cdm adds nothing and breaks nothing.

> **DECIDED:** no default global hotkey. Any default collides with something on someone's
> machine, and no settings surface for rebinding is specified. Revisit alongside a preferences
> window, if one ever exists.

### Manager window — tab order

Forward Tab order, macOS and Windows identical apart from the toolbar/command-bar position:

1. New Profile (`⊞` / command-bar button)
2. Filter field, when present (>10 profiles)
3. Profile list (one stop; arrows move within it)
4. Detail pane primary button (`Launch` / `Locate Folder…`)
5. `Rename…`
6. `Edit MCP Config…`
7. `Reveal…` / `Show…`
8. `Delete Profile…`
9. Banner action, when a banner is showing
10. wrap

Shift+Tab reverses. The list is a single tab stop with roving focus, per platform convention.

### Shortcuts

| Action | macOS | Windows |
| --- | --- | --- |
| New Profile | ⌘N | Ctrl+N |
| Launch selected | Return | Enter |
| Rename selected | ⌘R | F2 |
| Delete selected | ⌘⌫ | Delete |
| Edit MCP Config | ⌘E | Ctrl+E |
| Reveal folder | ⌘⇧R | Ctrl+Shift+R |
| Close (hide) window | ⌘W | Esc, Alt+F4 |
| Quit cdm | ⌘Q | — (tray only) |

> **DECIDED:** Return launches rather than renames, on both platforms. Launching is the
> frequent action and deserves the frequent key; rename gets F2 on Windows where that is the
> platform standard, and ⌘R on macOS where the Finder's Return-to-rename idiom does not carry
> into a master–detail app.

### Dialogs

- **Return** activates the default button. **Escape** always cancels, including macOS sheets,
  and always leaves state untouched.
- Destructive dialogs: default (macOS) / initially focused (Windows) button is **Cancel**.
  Return can never destroy a profile.
- Focus enters a dialog on its first text field if it has one (with existing text selected, so
  typing replaces), otherwise on the safe button.
- Focus is trapped in the dialog and restored to the invoking control on dismissal.
- macOS sheets announce as sheets; Windows dialogs are `WS_EX_DLGMODALFRAME` and announce as
  dialogs, with the message text as accessible name.

### Focus when the window is shown from the tray

1. The window is shown and raised; macOS switches activation policy to `Regular` first, so the
   window can actually take focus.
2. Focus goes to the **profile list**, with the previously selected row still selected. Screen
   readers therefore announce the window title and then the selected profile — the user knows
   where they are in two utterances.
3. If the list is empty, focus goes to the **New Profile** button.
4. If the tray item was *New Profile…*, the window is shown first and focus lands in the
   sheet's Name field.
5. Selection state survives hide/show. The window never re-opens with nothing selected when
   profiles exist.

### General

- All state is conveyed by text as well as colour or glyph: "Running", "Missing", "Never
  launched".
- List rows are ≥ 44 pt (macOS) / 40 px (Windows) tall for a two-line row; controls meet the
  platform minimum hit target.
- Respects OS text size, increased contrast, and reduced motion. There is no animation whose
  removal loses information; the launch feedback is a label change, not a spinner.
- No control is smaller than its label, no label is an icon alone. Every toolbar and footer
  icon button (`⊞`, `⊟`, `⋯`) has a tooltip and an accessible name: "New Profile", "Delete
  Profile", "More Actions".
