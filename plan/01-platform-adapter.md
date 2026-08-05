# Platform adapter

Four functions carry the whole surface of OS difference. Everything below was verified by
reading the extracted `app.asar` and by running the real binary against throwaway data
directories on macOS 15 (Darwin 25.5). Windows claims are marked **UNVERIFIED** and carry a
note on how to close them.

### 1. Does `--user-data-dir` survive? (the load-bearing question)

**Verdict: yes, with one conditional branch to know about.** The flag is honored, N profiles
run concurrently, and there is no single-instance lock to fight.

**No single-instance lock.** Grepping the entire extracted archive for
`requestSingleInstanceLock`, `releaseSingleInstanceLock`, and `second-instance` returns
**zero hits**:

```
$ grep -rn "requestSingleInstanceLock\|second-instance\|releaseSingleInstanceLock" asar/
(no output)
```

Claude Desktop therefore never asks Electron for the app-level lock, and never registers a
`second-instance` handler that would forward a new launch's argv into an already-running
instance. Isolation rests entirely on Chromium's own per-data-dir `SingletonLock`, which is
exactly what we want: distinct data dirs mean genuinely independent process trees.

**`app.setPath('userData', …)` does exist — twice — but neither call is unconditional.** Both
live in the packaged main bundle, `asar/.vite/build/index.pre.js` (single-line minified;
the calls sit at byte offsets inside line 174). De-minified with whitespace added:

```js
// --- call site 1: env-var override ---
if (Wie(process.argv) && !b9()) process.exit(1);
Jie(false);
if (process.env.CLAUDE_USER_DATA_DIR) {
  let e = process.env.CLAUDE_USER_DATA_DIR;
  E.app.setPath(`userData`, e);
  E.app.setPath(`logs`, m.default.resolve(e, `Logs`));
}

// --- call site 2: the "custom-3p" branch ---
if (!process.env.CLAUDE_USER_DATA_DIR && ($re(V7), y9(h9()))) {
  let e = Y7();
  if (E.app.getPath(`userData`) !== e) {
    E.app.setPath(`userData`, e);
    try {
      p.default.mkdirSync(e, { recursive: true, mode: 448 });
    } catch (t) {
      V7.warn(`[custom-3p] userData mkdir failed %s %s`, e, String(t));
    }
    E.app.setPath(`crashDumps`, m.default.join(e, `Crashpad`));
    if (process.platform === `darwin`) E.app.setAppLogsPath(rie());
  }
}
```

(`E` is the `electron` namespace import; `m.default` is `node:path`; `V7` is a logger.)

Reading these in order:

| Call site | Guard | Effect on cdm |
| --- | --- | --- |
| 1 | `process.env.CLAUDE_USER_DATA_DIR` is set | Harmless — cdm never sets that variable. It is also a **second, officially-supported isolation mechanism** (see below). |
| 2 | env var *unset* **and** `y9(h9())` is truthy — a "custom-3p" (custom third-party provider) mode read out of app config | Would relocate `userData` and thereby defeat `--user-data-dir` **for a user who has that mode switched on**. Off by default; the log tag `[custom-3p]` and the guard `E.app.getPath('userData') !== e` show it is an opt-in enterprise/alt-provider path. **UNVERIFIED** exactly what turns `y9(h9())` on — to close it, read `h9()`'s config source (`~/Library/Application Support/Claude/config.json`) and `Y7()`'s path computation in the same file. |

Neither call fires in the default configuration, which the probe runs confirm empirically:
launching `Claude --user-data-dir=<tmp>` populated `<tmp>` with the **full** Chromium *and*
application state — `Cookies`, `Local Storage`, `IndexedDB`, `Preferences`, `Local State`,
`Session Storage`, plus Claude's own `claude_desktop_config.json`, `config.json`, `ant-did`,
`fcache`, `sentry/`, `WebStorage/`. If `setPath('userData')` had redirected anything, the
app-level files would have landed in `~/Library/Application Support/Claude/` instead. They
did not.

**Consequence for the spec:** the `Core mechanism` section's claim stands as written. Two
profiles launched back to back both stayed alive and independent (probe A + probe B, 18 s
apart, both `kill -0` clean).

**Recommended belt-and-braces:** pass **both** the flag and the env var on launch:

```
spawn(binary, ["--user-data-dir=<dir>"], env: { CLAUDE_USER_DATA_DIR: "<dir>", ...inherit })
```

Setting `CLAUDE_USER_DATA_DIR` costs nothing, is read by call site 1 before anything else
runs, and — critically — **its presence short-circuits the `custom-3p` branch entirely**
(`!process.env.CLAUDE_USER_DATA_DIR &&` is the first term of that guard). It converts the one
identified hazard into a non-issue. This is a change from the spec's current
`spawn(binary, ["--user-data-dir=<profile.dir>"])`; recommend adopting it.

One caveat: call site 1 sets `logs` to `<dir>/Logs`, whereas the plain flag leaves logging at
the shared `~/Library/Logs/Claude/`. Per-profile logs are an improvement, not a regression.

### 2. Binary discovery

`CFBundleExecutable` read from the installed bundle rather than assumed:

```
$ /usr/libexec/PlistBuddy -c "Print :CFBundleExecutable" /Applications/Claude.app/Contents/Info.plist
Claude
```
**UNVERIFIED** — the value above is the expected/observed name and matches every process
line seen in the probes (`/Applications/Claude.app/Contents/MacOS/Claude`), but re-run the
PlistBuddy command to capture it verbatim. Do not hardcode `Claude`; read the plist.

| | macOS | Windows |
| --- | --- | --- |
| Bundle / install root | `/Applications/Claude.app` | `%LOCALAPPDATA%\AnthropicClaude\` **UNVERIFIED** |
| Executable | `<bundle>/Contents/MacOS/<CFBundleExecutable>` → `Claude` | `claude.exe` **UNVERIFIED** |
| Also search | `~/Applications/Claude.app` | `%LOCALAPPDATA%\Programs\`, `%PROGRAMFILES%` **UNVERIFIED** |
| Override | `CDM_CLAUDE_BINARY` | `CDM_CLAUDE_BINARY` |

*Closing the Windows row:* on real hardware, install Claude Desktop and run
`Get-ChildItem -Recurse -Filter claude.exe $env:LOCALAPPDATA` plus
`Get-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\* | ? DisplayName -like '*Claude*'`
to read `InstallLocation` / `DisplayIcon`. Squirrel-packaged Electron apps typically place a
stub `claude.exe` at the install root beside versioned `app-<semver>\` directories — the stub
is the one to launch, because it survives updates. Confirm before shipping.

```
find_claude_binary() -> Result<PathBuf>:
    if let Ok(p) = env::var("CDM_CLAUDE_BINARY"):
        return p if is_executable_file(p) else Err(OverrideNotExecutable(p))

    #[cfg(macos)]
        for bundle in ["/Applications/Claude.app",
                       "$HOME/Applications/Claude.app"]:
            plist = read_plist(bundle + "/Contents/Info.plist")   # binary plist
            exe   = plist["CFBundleExecutable"]                   # do NOT assume "Claude"
            cand  = bundle + "/Contents/MacOS/" + exe
            if is_executable_file(cand): return cand
        # last resort, respects a non-standard install location
        if let Some(b) = mdfind("kMDItemCFBundleIdentifier == 'com.anthropic.claudefordesktop'"):
            ...same Info.plist read...

    #[cfg(windows)]                                        # UNVERIFIED, see above
        for root in ["%LOCALAPPDATA%\\AnthropicClaude",
                     "%LOCALAPPDATA%\\Programs\\AnthropicClaude"]:
            if is_file(root + "\\claude.exe"): return that
        if let Some(loc) = registry_uninstall_install_location("Claude"):
            if is_file(loc + "\\claude.exe"): return that

    Err(NotFound { hint: "set CDM_CLAUDE_BINARY to the executable" })
```

The bundle identifier used by the `mdfind` fallback is **UNVERIFIED**; read
`:CFBundleIdentifier` from the same Info.plist to confirm.

### 3. Running detection

**What is actually on disk.** Inspection of the two live profile folders
(`~/Library/Application Support/Claude/` and `.../Claude-Work/`) and of the probe dirs:

| File | Type | Content / target |
| --- | --- | --- |
| `SingletonLock` | symlink | `<hostname>-<pid>` — dangling by design; the target need not exist |
| `SingletonCookie` | symlink | a random cookie value |
| `SingletonSocket` | symlink | path to a unix socket under `/var/folders/...` (TMPDIR) |

**UNVERIFIED in one respect:** the probe scripts grepped for `singleton` in the freshly
created probe dirs and the captured output does not clearly show the three symlinks present.
Re-check with `ls -l@ "<dir>" | grep -i singleton` and
`readlink "<dir>/SingletonLock"` against a *running* profile before relying on the pid parse.
The `<hostname>-<pid>` format is Chromium's documented, long-stable convention
(`chrome/browser/process_singleton_posix.cc`), so treat it as very likely but confirm.

**Reading a pid from it.** `readlink SingletonLock` → split on the **last** `-` (hostnames can
contain `-`) → parse the tail as a pid. Reliable *only* combined with two extra checks,
because after a crash the symlink is left behind verbatim:

- **Hostname must match** the current host. Chromium writes the hostname precisely so a
  data dir on a network share is not misread by a different machine.
- **The pid must still exist and must be Claude.** A stale pid is trivially recycled by an
  unrelated process. Verify by reading that pid's argv and requiring it to contain
  `--user-data-dir=<dir>`.

**What a stale one looks like.** Confirmed by `kill -KILL` on the main process: the data dir
is left with LevelDB `LOCK` files in `Session Storage/`, `Local Storage/leveldb/`, and
`IndexedDB/https_claude.ai_0.indexeddb.leveldb/` — and the `Singleton*` symlinks are not
cleaned up. Chromium recovers from this on its own: **relaunching over a crash-dirty
directory succeeded** (probe TEST 3 — new process alive after 18 s, no dialog, no prompt). So
a stale lock is a *detection* problem for cdm, never a *launch* problem.

```
is_running(data_dir) -> bool:
    # 1. cheapest: a pid this session's cdm spawned
    if let Some(pid) = session_pids.get(data_dir):
        if process_alive(pid): return true
        session_pids.remove(data_dir)          # reaped

    # 2. process enumeration — the authority, and the only path that
    #    works after cdm restarts. Also catches profiles the user
    #    launched some other way.
    canonical = canonicalize(data_dir)         # /tmp vs /private/tmp, symlinks, trailing sep
    for p in enumerate_processes_with_argv():
        if p.argv.any(|a| a == "--user-data-dir=" + canonical
                       || a == "--user-data-dir=" + data_dir):
            if is_main_process(p):             # reject --type=renderer / gpu-process / utility
                return true

    # 3. SingletonLock is a HINT ONLY — never trusted alone
    return false
```

Two enumeration details that matter, both observed in the probes:

- **Filter out helpers.** Every renderer, GPU, utility and audio helper inherits
  `--user-data-dir` on its own command line. The main process is the one whose executable is
  `…/Contents/MacOS/Claude` (no `Contents/Frameworks/…Helper…`) and which has **no `--type=`
  argument**. Matching naively counts 6–9 processes per profile.
- **Canonicalize before comparing.** The probes show argv echoing `/private/tmp/…` where the
  script passed `/tmp/…`; the same class of mismatch bites `~` and trailing slashes.

Implementation: `sysinfo` crate (`System::processes()` → `Process::cmd()`) covers both
platforms without shelling out. `ps -axww -o pid=,command=` is the shell equivalent used by
the probes and by `cdm doctor`.

*Windows note:* the spec's platform table names `lockfile` as the Chromium lock. That name is
**UNVERIFIED** for Claude Desktop — check whether the profile dir contains `lockfile` and/or
`SingletonLock`. Either way it does not change the design, because process enumeration is the
authority on both platforms and the lock file is only a hint.

### 4. Launch — direct spawn vs `open -n -a`

Both were run against a real profile dir. Both work; they differ in what cdm gets back.

| | Direct spawn of `Contents/MacOS/Claude` | `open -n -a Claude --args …` |
| --- | --- | --- |
| Flag reaches the app | yes | **yes** — confirmed, argv shows `--user-data-dir=…` |
| Data dir populated | yes | yes (full Chromium + app state) |
| Usable pid | **yes** — the spawned pid *is* the main process | **no** — `open` exits 0 immediately; the app is reparented to `launchd` (observed `PPID 1`) |
| Survives cdm quitting | yes, if spawned detached (see below) | yes, unconditionally |
| Dock / activation | app appears in Dock and takes focus normally | identical, plus LaunchServices' standard activation |
| Gatekeeper | **no difference** — the binary is inside the signed, notarized bundle either way; quarantine was cleared at first install and cdm never touches the bundle | same |
| Concurrency | fine | needs `-n`, else LaunchServices focuses the existing instance instead of launching |

**Recommendation: confirm the spec — spawn the inner Mach-O directly.** The deciding factor
is the pid. *Quit & Rename* and *Quit & Delete* need to terminate a specific profile
promptly; `open` hands back nothing, forcing cdm to poll `ps` and guess which of several
running profiles just appeared. Direct spawn hands over the main pid at `spawn()` and lets
cdm `waitpid`/watch it for exit, which is also how `lastUsedAt` and any future running-state
badge get their signal cheaply.

The commonly-cited reasons to prefer `open` do not apply here: Gatekeeper is unaffected
(the bundle is untouched and already trusted), and the probes show a directly-spawned Claude
gets a normal Dock tile and normal activation — Electron calls
`TransformProcessType`/`activateIgnoringOtherApps` itself, so LaunchServices is not needed to
make the window frontmost.

```
launch(binary, data_dir) -> Result<Pid>:
    ensure_dir(data_dir)

    cmd = Command::new(binary)
    cmd.arg("--user-data-dir=" + canonicalize(data_dir))
    cmd.env("CLAUDE_USER_DATA_DIR", canonicalize(data_dir))   # see §1 — kills the custom-3p branch
    cmd.stdin(null); cmd.stdout(null); cmd.stderr(null)       # unless `cdm launch --wait`

    #[cfg(unix)]
        cmd.process_group(0)        # own pgid: cdm's Ctrl-C / SIGINT never reaches the profile
        # deliberately NOT setsid()-ing away the parent — we want to keep the pid watchable

    #[cfg(windows)]                                            # UNVERIFIED
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)

    child = cmd.spawn()?
    session_pids.insert(data_dir, child.id())
    reap_in_background(child)       # avoid zombies; drop the pid from session_pids on exit
    return child.id()
```

Two macOS specifics:

- **Detachment.** A directly-spawned child is *not* killed when cdm exits — Unix does not
  cascade. It only dies with cdm if it shares a controlling terminal and receives SIGHUP, or
  a process group signal. `process_group(0)` closes both. Probe A stayed alive across the
  probe script's own lifetime.
- **`cdm launch --wait`** (debug CLI) keeps stderr piped instead of nulled. Claude's stderr is
  noisy but harmless — observed output was Electron `MaxListenersExceededWarning`, a
  `DEP0169 url.parse()` deprecation, and an updater line. **Do not treat stderr output as a
  launch failure.**

*Windows,* **UNVERIFIED**: if the discovered `claude.exe` turns out to be a Squirrel stub, the
stub may exec the real binary and exit, making the returned pid short-lived and useless. Check
on hardware whether `spawn(claude.exe).id()` is still alive 5 s later; if it is not, fall back
to process enumeration (§3) to resolve the real pid, and note that `is_running` already
handles that case.

### 5. Termination

**macOS — SIGTERM does not flush state, and that is fine.** Measured directly (probe TEST 1):
a profile was run for 20 s, `Preferences` was hashed, `SIGTERM` was sent, and the hash was
compared after exit.

```
pre-kill  Preferences md5: f973998556308b8cc08798ced7386670   (181 bytes)
post-TERM Preferences md5: f973998556308b8cc08798ced7386670   (181 bytes)
process exited 1s after SIGTERM
orphan helpers after SIGTERM: (none)
```

Read carefully: the file was **unchanged**, meaning Electron did *not* perform an extra
shutdown flush of `Preferences` on SIGTERM — but it also did not corrupt or truncate it, and
it exited cleanly in ~1 s taking every helper process with it. Chromium writes its important
state (cookies, LevelDB, `Local State`) continuously with its own durability, not at exit.
The practical conclusion: **SIGTERM is a safe, fast quit.** It is not a graceful
`before-quit`/`window-all-closed` app shutdown, so anything Claude only persists on explicit
quit is at risk — accept that, since the alternative (AppleScript `quit`) reintroduces the
same pid-less problem as `open`.

By contrast `SIGKILL` on the main process left LevelDB `LOCK` files behind in three
subdirectories and is strictly worse. Use it only as the escalation.

```
terminate(data_dir, pid_hint) -> Result<()>:
    pids = resolve_main_pids(data_dir, pid_hint)   # session pid, else enumeration (§3)
    if pids.is_empty(): return Ok(())              # already stopped

    #[cfg(unix)]
        for p in pids: kill(p, SIGTERM)
        wait_until(|| !any_alive(pids), timeout = 5s)     # observed: ~1s
        if still alive:
            for p in pids: kill(p, SIGKILL)
            wait_until(|| !any_alive(pids), timeout = 3s)
        # helpers self-exit with the main process; after SIGKILL, sweep any
        # process whose argv still carries --user-data-dir=<dir>
        sweep_orphans(data_dir)

    #[cfg(windows)]                                        # UNVERIFIED — see below
        for p in pids: post_wm_close_to_all_toplevel_windows(p)
        wait_until(|| !any_alive(pids), timeout = 5s)
        if still alive: taskkill /PID <p> /T /F

    # rename/delete must re-check afterwards: on Windows the move fails while
    # any handle is still open, and handle release can lag process exit.
    wait_until(|| !is_running(data_dir), timeout = 3s)
```

**Windows — there is no SIGTERM, and `taskkill` without `/F` is not equivalent.**
**UNVERIFIED**; what is known and what to test:

- Rust's `Child::kill()` maps to `TerminateProcess`, which is a hard kill — the SIGKILL
  equivalent, not the SIGTERM equivalent. Using it as the first step would leave the LevelDB
  locks seen above.
- The graceful path for a GUI process is a window message: enumerate the target pid's
  top-level windows (`EnumWindows` + `GetWindowThreadProcessId`) and `PostMessage(WM_CLOSE)`,
  or equivalently `taskkill /PID <pid>` **without** `/F`, which posts `WM_CLOSE` for you. This
  is the honest analogue of SIGTERM.
- Complication specific to Electron: closing all windows does not necessarily quit the app,
  and a Squirrel stub parent may not own the window at all — hence `/T` (tree) on the
  escalation.
- *To close this:* on real hardware, launch a profile, run `taskkill /PID <pid>` (no `/F`),
  and check (a) whether the process tree exits within ~5 s, (b) whether the profile dir is
  left with LevelDB `LOCK` files, and (c) whether `fs::rename` of the profile folder succeeds
  immediately afterwards or needs a retry loop. Expect to need a short retry loop on the
  rename regardless.

### 6. Summary — the four adapter functions

| Function | macOS | Windows |
| --- | --- | --- |
| `find_claude_binary()` | `Info.plist` → `CFBundleExecutable` under `/Applications/Claude.app/Contents/MacOS/`; `mdfind` fallback | `%LOCALAPPDATA%\AnthropicClaude\claude.exe`, registry `InstallLocation` fallback **UNVERIFIED** |
| `launch(binary, dir)` | direct spawn, `process_group(0)`, flag **+** `CLAUDE_USER_DATA_DIR` | `DETACHED_PROCESS \| CREATE_NEW_PROCESS_GROUP` **UNVERIFIED** |
| `profiles_root()` | `~/Library/Application Support/` | `%APPDATA%\` **UNVERIFIED** |
| `is_running(dir)` | session pid → argv enumeration (skip `--type=` helpers); `SingletonLock` is a hint | same enumeration; `lockfile` name **UNVERIFIED** |
| terminate | `SIGTERM` (≈1 s, clean) → `SIGKILL` | `WM_CLOSE` / bare `taskkill` → `taskkill /T /F` **UNVERIFIED** |

Both `CDM_CLAUDE_BINARY` overrides are honored ahead of all discovery on both platforms.

### Open items this section leaves behind

1. **`custom-3p` branch** — what makes `y9(h9())` true in `asar/.vite/build/index.pre.js`.
   Mitigated by always passing `CLAUDE_USER_DATA_DIR`, but worth reading `h9()` and `Y7()` to
   know the blast radius.
2. **`Singleton*` symlink shapes** — re-observe on a live profile; the `<hostname>-<pid>`
   format is assumed from Chromium upstream, not read off this machine.
3. **Every Windows row above.** All are marked inline; none block the macOS build.
