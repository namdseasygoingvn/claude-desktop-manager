# Distribution and updates

cdm ships as a **directly downloaded, signed desktop app on both platforms**. It is not a
store app on either. The rest of this section explains why that is forced rather than chosen,
and what it costs.

### macOS: entitlements for spawning Claude Desktop

The launch design — `posix_spawn` on `/Applications/Claude.app/Contents/MacOS/Claude` with
`--user-data-dir`, from a Developer-ID-signed app with Hardened Runtime enabled — **requires no
entitlements at all**. This is the answer the rest of the spec depends on, so it is worth being
precise about why.

**Hardened Runtime does not restrict process creation.** The Hardened Runtime entitlements are
a fixed, documented set covering JIT and writable-executable memory, `DYLD_*` environment
variables, library validation, debugging, Apple Events automation, and device/personal-data
access. There is no entitlement governing `fork`, `posix_spawn`, or `exec`, because process
creation was never restricted by Hardened Runtime in the first place. A hardened,
Developer-ID-signed binary can exec any other executable on the system exactly as an
unhardened one can.

**Library validation does not apply across `exec`.**
`com.apple.security.cs.disable-library-validation` controls which code may be loaded *into*
cdm's own address space — plugins, dylibs, and frameworks signed by a different Team ID. `exec`
replaces the process image entirely: the child is evaluated against *its own* signature and
*its own* entitlements, and Claude Desktop's Team ID is irrelevant to cdm's library-validation
state. cdm loads nothing from the Claude bundle, so this entitlement is not needed. **Do not
add it.** It weakens cdm's own signature guarantee and buys nothing.

Related: **entitlements are not inherited across `exec`.** The spawned Claude process gets the
entitlements in its own signature. cdm cannot grant, restrict, or leak capability to it.

**Required macOS bundle configuration:**

```jsonc
// tauri.conf.json
{
  "bundle": {
    "macOS": {
      "hardenedRuntime": true,      // required for notarization
      "minimumSystemVersion": "..." // set deliberately; see packaging below
    }
  }
}
```

No `Entitlements.plist` is needed. If one is added later for an unrelated reason, it must not
be a copy-paste of an Electron template — Electron apps carry `allow-jit` and
`allow-unsigned-executable-memory` because *they* run V8. cdm is a Rust binary with a WKWebView;
WKWebView's JIT lives in its own out-of-process JavaScript engine, not in cdm's address space.

#### Consequences of spawning the inner binary directly

The spec already commits to direct spawn (over `open -n -a Claude`) because `open` discards the
child pid. Three consequences follow, none of them entitlement-shaped:

1. **Gatekeeper first-launch is skipped.** The quarantine-and-notarization prompt ("… downloaded
   from the Internet, are you sure?") is presented by **LaunchServices**, not by the kernel.
   Executing the inner Mach-O directly bypasses that path. In practice this is a non-issue:
   Claude Desktop is the user's own installed app in `/Applications`, already launched at least
   once, so its `com.apple.quarantine` attribute has already been cleared. The signature itself
   is *still* verified — AMFI validates the code signature on every `exec` regardless of how the
   process was started, so a tampered or unsigned Claude binary fails to launch either way. What
   is skipped is only the one-time user consent dialog, not the security check.
   **UNVERIFIED:** whether `exec` of a *still-quarantined* app bundle is blocked outright by
   `syspolicyd` on macOS 14+/15+, or merely runs without a prompt. To close: set
   `com.apple.quarantine` on a scratch signed `.app` in `/Applications` and `posix_spawn` its
   inner binary. Cheap mitigation regardless: if `find_claude_binary()` sees a
   `com.apple.quarantine` xattr on the bundle, fall back to `open -n -a` for that one launch and
   accept the lost pid — the cmdline-scan fallback in *Running detection* already covers a
   missing pid.

2. **App Translocation does not apply.** Path randomization only affects quarantined apps run
   from outside `/Applications`. Not our case.

3. **TCC "responsible process" attribution.** This is the real one. When a process is spawned
   directly rather than through LaunchServices, macOS may attribute the child's TCC prompts to
   the *responsible* process — the parent — meaning a permission prompt raised by Claude Desktop
   (or by an MCP server it starts) could be shown against **cdm's** name and recorded against
   **cdm's** bundle identifier. Chromium and other launchers disclaim this with the private SPI
   `responsibility_spawnattrs_setdisclaim()` from `spawn_private.h`, applied to the
   `posix_spawnattr_t` before spawning.
   **UNVERIFIED** in this context — neither the attribution behaviour for a GUI-app child nor
   the SPI's current availability has been tested here. Treat as a polish item, not a blocker:
   worst case is a confusingly-labelled permission dialog, not a launch failure. To close: launch
   a profile, trigger a TCC-gated action inside it, and read the prompt's app name. Being a
   private SPI is acceptable only because cdm is never going to the App Store (below).

**Detachment is a separate requirement from entitlements** and is easy to conflate. The child
must survive cdm quitting or being replaced by an update: `setsid()` (or `POSIX_SPAWN_SETSID`),
no shared process group, no inherited controlling terminal, and stdio redirected away from
cdm's. Correspondingly, cdm must never terminate by process-group kill, which would take every
running profile down with it.

### macOS: App Sandbox is not viable — Mac App Store is off the table

State this plainly in the spec so it is never re-litigated.

cdm creates, renames, and trashes folders directly under `~/Library/Application Support/` that
belong to **another application**, and it execs that application's binary. App Sandbox confines
a process to `~/Library/Containers/<bundle-id>/Data`; access outside that container comes only
from user-selected files (`com.apple.security.files.user-selected.read-write`) plus
security-scoped bookmarks. Two independent blockers:

- **`~/Library` is not user-selectable from a sandboxed app.** The sandboxed open panel hides
  and refuses that path, so there is no user-consent route to a bookmark for the profiles root.
  `com.apple.security.temporary-exception.files.absolute-path.read-write` is the historical
  escape hatch and is not accepted for App Store review.
- **Child processes inherit the sandbox.** A sandboxed cdm cannot exec an arbitrary binary
  outside its own bundle into an unconfined process. Even if a LaunchServices route existed, the
  spec needs the pid and needs to pass `--user-data-dir`, which is exactly what that route
  gives up.

Consequences, all accepted:

| | Outcome |
| --- | --- |
| Mac App Store | Not possible. No sandbox, therefore no submission. |
| Distribution | Direct download only — signed, notarized `.dmg` from GitHub Releases. |
| Updates | Self-update via `tauri-plugin-updater` (which is also MAS-forbidden, so no loss). |
| Trust surface | Gatekeeper + notarization is the whole story; there is no store review badge. |

The Windows side has the mirror version of this: cdm is not an MSIX/Store app, for the same
reason — it writes another app's `%APPDATA%` folders and spawns that app.

### Updater: resolving the private-repo problem

`tauri-plugin-updater` v2 fetches a **static JSON manifest** over TLS and verifies the
downloaded artifact against a **minisign public key compiled into the binary**. The repo being
private breaks the *hosting* half of that, not the signing half.

**Manifest format** (the plugin validates the whole file before comparing versions, so every
platform entry present must be well-formed):

```json
{
  "version": "1.4.0",
  "notes": "…",
  "pub_date": "2026-08-05T12:00:00Z",
  "platforms": {
    "darwin-aarch64": { "signature": "<contents of the .sig file>", "url": "https://…" },
    "darwin-x86_64":  { "signature": "…", "url": "https://…" },
    "windows-x86_64": { "signature": "…", "url": "https://…" }
  }
}
```

Keys are `OS-ARCH`. `signature` is the **literal contents** of the generated `.sig` file, not a
path or a URL.

**Configuration:**

```jsonc
{
  "bundle": { "createUpdaterArtifacts": true },
  "plugins": {
    "updater": {
      "pubkey": "<contents of the generated public key — the key itself, not a path>",
      "endpoints": ["https://…/{{target}}/{{arch}}/{{current_version}}"]
    }
  }
}
```

Supported endpoint placeholders: `{{target}}`, `{{arch}}`, `{{current_version}}`. Endpoints are
tried in order; the plugin advances to the next only on a non-2XX response. TLS is enforced in
production. The frontend also needs the `updater:default` permission in its capability file.

**Keypair.** `tauri signer generate` produces a private key and its public key. The public key
goes in `tauri.conf.json` and ships in the binary; the private key is a CI secret
(`TAURI_SIGNING_PRIVATE_KEY`, with `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` if it was generated with
a passphrase). **Losing the private key ends the update channel** — every installed copy trusts
only that one key, and recovering means shipping a new manifest signed by a key nobody trusts.
Back it up outside CI, offline.

#### Where to host the manifest — decision

GitHub Releases on a private repo is not anonymously fetchable. Release assets need an
`Authorization: Bearer <token>` header against the API asset URL with
`Accept: application/octet-stream`. `tauri-plugin-updater` *can* send custom headers, but the
only way to get a token into a shipped desktop app is to embed one — which means publishing a
credential to everyone who downloads the app. **Rejected outright.**

| Option | Verdict |
| --- | --- |
| Embed a PAT in the app, hit the private repo's API | **No.** Ships a credential; revocation breaks every install. |
| Make `claude-desktop-manager` public | Works, but the repo is locked private by decision. Not ours to change. |
| **Separate public releases repo** (e.g. `namdseasygoingvn/cdm-releases`) holding only built artifacts and `latest.json` | **Chosen.** Source stays private, artifacts are anonymously fetchable, zero new infrastructure, stable URL. |
| Static host (Cloudflare R2/Pages, S3 + CDN) | Viable fallback if release-asset bandwidth or per-channel routing ever matters. Costs an account and a deploy step. |

Chosen shape: build in the private repo's CI, publish artifacts and `latest.json` to the public
releases repo, and point `endpoints` at
`https://github.com/namdseasygoingvn/cdm-releases/releases/latest/download/latest.json`. The
public repo carries no source, no issues, and a README that says what it is. Cross-repo publish
needs a token with write access to the releases repo — a fine-grained PAT or a GitHub App token
stored as a secret in the private repo (the default `GITHUB_TOKEN` is scoped to its own repo
only).

**UNVERIFIED:** whether `tauri-apps/tauri-action` can target a *different* repo for the release
in one step, or whether the workflow must build with `tauri-action` and then upload separately
with `gh release upload --repo …`. To close: read the action's documented inputs. The two-step
form definitely works, so assume it and simplify later.

#### Updating cdm while profiles are running

Safe, and worth stating because it looks alarming. Tauri's macOS updater downloads the new
bundle, replaces cdm's own `.app`, and re-execs cdm. Running Claude Desktop processes are a
**different bundle** (`/Applications/Claude.app`) in **detached sessions**, so nothing about
them is touched — no file in their `--user-data-dir` moves, and they are not in cdm's process
group (per the detachment requirement above), so they are not signalled.

The one real casualty is cdm's **in-memory pid table**, which does not survive the restart. The
spec already handles exactly this case for a plain restart: `is_running(data_dir)` falls back to
the Chromium lock file, and *Quit* falls back to scanning for a process whose command line
contains the profile's `--user-data-dir`. So after an update, every running profile is still
correctly detected and still quittable — via the slower path. No new mechanism is needed.

Two operational notes:

- Prefer prompting to update at a **quiet moment** rather than on launch, and never auto-restart
  without consent. Restart is cheap but confusing while profiles are open.
- The macOS update writes over cdm's own bundle, so cdm must be installed somewhere the user can
  write. Installed to `/Applications` by an admin user this is normally fine; a non-admin install
  or a locked-down `/Applications` will fail the replace. Surface that failure as "download an
  update manually", not a silent no-op.

### Windows packaging: NSIS, per-user

**Pick NSIS.** Tauri can emit both a WiX-based `.msi` and an NSIS `-setup.exe`; for this app
NSIS wins on the two things that matter.

| | NSIS (`-setup.exe`) | MSI (WiX) |
| --- | --- | --- |
| Per-user install | Yes — `bundle.windows.nsis.installMode: "currentUser"` | Per-machine; no per-user mode in Tauri's WiX bundle |
| Silent update | Runs silently; the updater's normal path for Tauri apps | Goes through `msiexec`, with an elevation prompt |
| Elevation at install | None, for `currentUser` | UAC prompt every install and every update |
| Enterprise/GPO deploy | Weak | Strong — the only real argument for MSI |
| Arm64 | Supported | Supported |

cdm is a personal-scope tray utility that stores everything in `%APPDATA%` and updates itself.
A per-machine installer that raises UAC on every silent update is actively wrong for that shape,
and there is no enterprise-deployment requirement to trade against. Configure:

```jsonc
{
  "bundle": {
    "targets": ["nsis"],
    "windows": { "nsis": { "installMode": "currentUser" } }
  }
}
```

Per-user install puts the program under `%LOCALAPPDATA%` and the profile data under `%APPDATA%`,
consistent with the platform table in *Architecture* — and consistent with Claude Desktop itself,
which installs per-user to `%LOCALAPPDATA%\AnthropicClaude`. A per-machine cdm managing per-user
Claude data would be an odd split.

**UNVERIFIED:** the exact `installMode` string set and whether a `"both"` value is offered. To
close: check `bundle.windows.nsis.installMode` in the Tauri v2 config reference. `currentUser`
and `perMachine` are the two that matter and the intent above is unambiguous either way.

#### Code signing on Windows

Unsigned means a SmartScreen "Windows protected your PC" interstitial on **every** install *and
every silent update*, which for a self-updating app is not survivable. Signing is mandatory.

| | OV | EV |
| --- | --- | --- |
| Identity vetting | Organisation validated | Extended validation, heavier paperwork |
| Key storage | Hardware token / HSM (CA/B Forum, since June 2023 — no longer an OV/EV difference) | Hardware token / HSM |
| SmartScreen reputation | Accrues over downloads and time; new certs warn initially | Historically granted immediate/expedited reputation |
| Cost | Lower | Notably higher |

Both OV and EV now require the private key on FIPS-140-2-Level-2 hardware, so the old "OV is
just a `.pfx` you can drop into CI" convenience is gone — either way CI signs through a cloud
HSM or a signing service. That collapses much of the practical gap and makes **Azure Trusted
Signing** the pragmatic recommendation: subscription-priced rather than per-certificate, issues
short-lived certs, and integrates with CI without shipping a hardware token around.

**UNVERIFIED:** whether EV still confers *immediate* SmartScreen reputation in 2026, and what
reputation profile Azure Trusted Signing certificates get. Microsoft does not document
SmartScreen's model precisely and it has changed. To close: check Microsoft's current Trusted
Signing docs before buying anything — this is a purchasing decision, not an architectural one,
and it can be deferred past the first internal builds.

Reputation is **per publisher identity**, so pick the signing identity once and keep it.
Changing certificates or switching CA resets accrued reputation.

### macOS packaging

Targets `app` and `dmg`. The `.dmg` is the download; the `.app` inside it is what gets signed
and notarized.

- **One `.dmg` per arch**, not a universal binary. `universal-apple-darwin` is a lipo of two
  real targets, so a universal build compiles the whole app twice inside a single job; as two
  jobs the same work overlaps and the manifest gets a distinct artifact per arch instead of
  `darwin-aarch64` and `darwin-x86_64` both pointing at one fat download. The cost is that
  users pick their chip on the download page.
- **Developer ID Application** certificate — the only identity that works for direct download.
  (`Apple Development` is for local runs; `3rd Party Mac Developer` is the App Store path that is
  closed to us.)
- **Hardened Runtime on** — `bundle.macOS.hardenedRuntime: true`. Notarization rejects builds
  without it. No entitlements file, per the section above.
- **Notarization** via `notarytool`, driven by Tauri's bundler when the credentials are present
  in the environment. Two credential shapes: Apple ID + app-specific password + team ID, or an
  App Store Connect API key. **Prefer the API key** in CI — it does not expire on password
  rotation and is not tied to a human's Apple ID with 2FA.
- **Stapling** so the app validates offline, on a machine that has never seen it. Tauri staples
  the `.app` after notarization.
  **UNVERIFIED:** whether Tauri also staples the enclosing `.dmg`. To close: run
  `xcrun stapler validate` against the built `.dmg` in CI and add an explicit
  `xcrun stapler staple` step if it fails. Add the validate step regardless — a silently
  unstapled artifact only surfaces as a user complaint weeks later.
- **`minimumSystemVersion`** — set it deliberately rather than inheriting a default, and verify
  the profile-launch path on the oldest version claimed.

### CI and versioning

One GitHub Actions workflow, triggered by a tag push, matrixed one job per target, using
`tauri-apps/tauri-action` to build and bundle. Both macOS arches run on `macos-latest` (Apple
Silicon runner, cross-compiling for Intel); Windows on `windows-latest`. The build cache is
keyed by target, since the two macOS jobs are otherwise indistinguishable to it.

Shape:

1. Tag push `v*` on the private repo triggers the workflow.
2. Matrix builds an Apple Silicon `.dmg`, an Intel `.dmg`, and a Windows NSIS `-setup.exe`,
   each with its `.sig` updater artifact (`createUpdaterArtifacts: true`).
3. Signing and notarization happen inside the build step, from secrets.
4. Artifacts upload to a **draft** release in the **public** releases repo.
5. A final job assembles `latest.json` from the `.sig` files, uploads it to the same release,
   then publishes. Publishing last matters: the manifest must never point at an asset that is
   not yet downloadable, and the plugin validates the whole manifest before comparing versions.

#### Required secrets

| Secret | Platform | Purpose |
| --- | --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | both | Signs updater artifacts. Its public half is compiled into the app. Back up offline. |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | both | Passphrase for the above, if set at generation. |
| `APPLE_CERTIFICATE` | macOS | Developer ID Application cert + key, base64-encoded `.p12`. |
| `APPLE_CERTIFICATE_PASSWORD` | macOS | Password for that `.p12`. |
| `APPLE_SIGNING_IDENTITY` | macOS | Identity to sign with, e.g. `Developer ID Application: … (TEAMID)`. |
| `KEYCHAIN_PASSWORD` | macOS | Password for the temporary CI keychain the cert is imported into. |
| `APPLE_TEAM_ID` | macOS | Team identifier, for notarization. |
| `APPLE_API_ISSUER` / `APPLE_API_KEY` / `APPLE_API_KEY_PATH` | macOS | App Store Connect API key for `notarytool`. **Preferred** over the Apple ID form. |
| `APPLE_ID` / `APPLE_PASSWORD` | macOS | Alternative notarization credentials: Apple ID + app-specific password. Use one shape or the other, not both. |
| Windows signing credentials | Windows | Shape depends on the signing route chosen (Azure Trusted Signing vs. a cloud-HSM CA). Fill in once that decision is made. |
| `RELEASES_REPO_TOKEN` | both | Write access to the public releases repo. The default `GITHUB_TOKEN` cannot reach another repo. |

**UNVERIFIED:** the exact spelling of the Apple-side variable names above, and whether
`APPLE_API_KEY_PATH` is required alongside `APPLE_API_KEY`. These are Tauri's documented
environment variables rather than invented ones, but they have changed across versions. To
close: check the Tauri v2 macOS code-signing page before writing the workflow — a typo here
fails the build loudly and immediately, so the risk is wasted CI minutes, not a shipped defect.

#### Versioning

- **Semantic versioning**, single source of truth in `tauri.conf.json`'s `version`. The Rust
  crate version and the manifest's `version` derive from it; do not maintain a second copy.
- **Tags are `v<version>`** — `v0.3.1` — and a tag push is the only thing that publishes. The
  workflow should fail if the tag does not match `tauri.conf.json`'s version rather than quietly
  shipping a mismatch.
- **Version comparison is the plugin's**, based on the manifest's `version` against the running
  app's. Do not hand-roll a check.
- **Where the version appears in the UI:** an *About* row in the manager window — version,
  commit hash, and a *Check for updates* action. The tray menu does **not** carry it; the tray is
  the everyday surface and stays short. When an update is available, the manager window shows it
  inline; a badge on the tray icon is optional and deferred.
- Pre-1.0 while the Windows platform row is still unverified. Ship `0.x`, and do not promise
  update-channel stability until the first release has actually updated a real install.

### Open items this section closes and leaves

Closes *Updating cdm itself* from **Open items**: updater plugin, minisign keypair, separate
public releases repo, manifest published last.

Leaves open: the Windows signing purchase decision, TCC responsibility attribution, and DMG
stapling — each flagged **UNVERIFIED** above with a way to close it.
