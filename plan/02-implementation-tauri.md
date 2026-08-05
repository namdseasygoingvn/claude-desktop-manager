# Implementation notes (Tauri)

Everything below is verified against Tauri **2.11.5** (July 2026) and the current
crates.io / docs.rs / Microsoft Learn documentation. Anything not confirmed against a
primary source is marked **UNVERIFIED**.

### Target version and project layout

Tauri **v2** (`tauri = "2"`, currently 2.11.5). v1 is not an option: the tray, menu,
activation-policy and plugin APIs used here either did not exist or had different names
in v1, and v1 is EOL.

```
cdm/
├── package.json                 # dev-only: vite + typescript + @tauri-apps/api
├── index.html                   # profile list
├── src/
│   ├── main.ts                  # invoke() calls, DOM rendering
│   └── style.css
└── src-tauri/
    ├── Cargo.toml
    ├── build.rs                 # tauri_build::build()
    ├── tauri.conf.json
    ├── Info.plist               # optional; merged by the CLI on macOS
    ├── icons/                   # icon.icns, icon.ico, icon.png, trayTemplate.png
    ├── capabilities/default.json
    └── src/
        ├── main.rs              # entry: CLI dispatch, else run()
        ├── lib.rs               # pub fn run() -> tauri::Builder wiring
        ├── core/
        │   ├── mod.rs
        │   ├── registry.rs      # registry.json, atomic write, ids
        │   ├── slug.rs          # NFC + slug algorithm + collision resolution
        │   ├── profile.rs       # create / rename / delete / launch orchestration
        │   └── reconcile.rs     # registry <-> disk, .cdm-profile markers
        ├── platform/
        │   ├── mod.rs           # trait/fn surface: find_claude_binary, launch,
        │   │                    #   profiles_root, is_running  (+ cfg re-export)
        │   ├── macos.rs         # #[cfg(target_os = "macos")]
        │   └── windows.rs       # #[cfg(target_os = "windows")]
        ├── commands.rs          # #[tauri::command] wrappers — thin, no logic
        ├── tray.rs              # build_tray(), rebuild_tray_menu()
        └── cli.rs               # clap parser + headless dispatch
```

`main.rs` stays tiny; `lib.rs::run()` holds the builder so the same crate is usable from
integration tests. `core/` never `use`s `tauri::` — it takes and returns plain types, so the
debug CLI (build order step 1) links against it with no GUI in the graph.

**Frontend: no framework.** The whole surface is one list plus four dialogs (New, Rename,
Delete-confirm, Adopt-orphan). React/Svelte/Solid each add a dependency tree, a build
config and a reactivity model to render `<li>` elements from a `Vec<Profile>` that only
changes when the user clicks something. Recommendation:

**Vite + TypeScript, vanilla (`create-tauri-app` "Vanilla / TypeScript" template).** One dev
dependency tree, no runtime framework, and typed `invoke<Profile[]>("list_profiles")`
against the command surface — the types are the only thing worth a build step here.

There is a strictly smaller option: set `app.withGlobalTauri: true` in `tauri.conf.json`,
point `build.frontendDist` at a plain static folder, and drop npm entirely, calling
`window.__TAURI__.core.invoke(...)`. Confirmed present in the v2 config schema
(`AppConfig.withGlobalTauri`, default `false`). It costs you all TypeScript types on the IPC
boundary and injects the full JS API surface into the webview. Take it only if the npm
toolchain is itself the thing you are trying to avoid.

Either way there is **no state library, no router, no CSS framework**. The frontend calls
`invoke`, renders, and re-`invoke`s after mutations.

### Plugins and crates

Tray icon is **not** a plugin in v2 — it is a Cargo feature on the `tauri` crate:

```toml
tauri = { version = "2", features = ["tray-icon", "image-png"] }
```

`image-png` is needed for `Image::from_bytes` on a PNG tray icon; use `image-ico` instead if
you ship `.ico`. (Both confirmed in the `tauri` crate's `[features]` table.)

| Crate | Version (Aug 2026) | Why |
| --- | --- | --- |
| `tauri` | 2.11.5 | core; features `tray-icon`, `image-png` |
| `tauri-build` | 2.x (build-dep) | codegen in `build.rs` |
| `tauri-plugin-single-instance` | 2.4.3 | second launch focuses the manager instead of a second tray icon |
| `tauri-plugin-dialog` | 2.7.2 | delete confirm, "Quit &" prompts, binary-not-found error; used from **Rust** (`blocking_show_with_result`) |
| `tauri-plugin-opener` | 2.5.4 | *Edit Config* (`open_path`) and *Reveal in Finder/Explorer* (`reveal_item_in_dir`) |
| `tauri-plugin-log` | 2.9.0 | optional; file + stderr logging for `cdm doctor` |

**Deliberately not used:**

- **`tauri-plugin-fs`** — the spec says the frontend has no filesystem access. Adding the fs
  plugin would create exactly the capability the architecture forbids. Omit it.
- **`tauri-plugin-shell`** — see *Detached child processes*; it kills its children on exit.
- **`tauri-plugin-cli`** — it parses argv *inside* a running Tauri app and surfaces the match
  to JS. It cannot give you a headless run, which is the whole point of the debug CLI.

Non-Tauri crates:

| Crate | Version | Purpose | Notes |
| --- | --- | --- | --- |
| `serde` | 1.0.229 | `derive` feature; registry + IPC types | |
| `serde_json` | 1.0.151 | `registry.json`, `claude_desktop_config.json` | use `to_writer_pretty` |
| `tempfile` | 3.27.0 | `NamedTempFile::new_in(dir)` + `persist()` for atomic registry writes | see caveat below — `persist` does **not** fsync |
| `trash` | 5.2.6 (May 2026) | Delete → Trash / Recycle Bin | `trash::delete(path)`. macOS uses the system trash API, Windows uses the shell file operation. Maintained. |
| `sysinfo` | 0.39.6 | find a Claude process by its `--user-data-dir` argument | `Process::cmd() -> &[OsString]`. **Windows caveat, from its own docs: you may need elevated privileges to read another process's command line.** |
| `unicode-normalization` | 0.1.25 | step 1 of the slug algorithm (NFC) | `UnicodeNormalization::nfc()`; unicode-rs, maintained |
| `clap` | 4.6.5 | debug CLI subcommands | `derive` feature |
| `thiserror` | 2.0.19 | core error enum; `impl Serialize` for the IPC boundary | |
| `windows-sys` | 0.61.2 | `#[cfg(windows)]` only: `AttachConsole`, `ATTACH_PARENT_PROCESS` | features `Win32_System_Console`, `Win32_Foundation` |

None of the above are unmaintained. `trash` carries one documented hazard that does **not**
apply to us: its crate docs warn about potential UB on **Linux and FreeBSD** from
non-threadsafe `getmntent`. macOS and Windows are unaffected.

`sysinfo`'s API changed shape in recent releases; the 0.39 spelling is
`RefreshKind::nothing()` / `ProcessRefreshKind::nothing().with_cmd(..)`, not the older
`::new()`. Refresh only what you need:

```rust
use sysinfo::{ProcessRefreshKind, RefreshKind, System, UpdateKind};

let mut sys = System::new_with_specifics(
    RefreshKind::nothing()
        .with_processes(ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always)),
);
sys.refresh_processes_specifics(
    sysinfo::ProcessesToUpdate::All,
    true,
    ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
);
```

**UNVERIFIED:** the exact argument list of `refresh_processes_specifics` in 0.39.6 (the
`ProcessesToUpdate` / `remove_dead_processes` parameters). Confirm against
`docs.rs/sysinfo/0.39.6/sysinfo/struct.System.html` before writing it. The `System::new_with_specifics`,
`RefreshKind::nothing`, `ProcessRefreshKind::nothing`, `with_cmd`, and `Process::cmd`
names are all confirmed.

Application-defined commands (those registered through `invoke_handler`) do **not** need
capability entries — Tauri's docs state that by default all commands registered by the app
are callable from all its windows. `capabilities/default.json` therefore only needs
`"core:default"` plus permissions for any plugin command you call *from JS*. Call the dialog
and opener plugins from Rust and that list stays empty. `build.removeUnusedCommands` exists
in the v2 config schema and is worth enabling to strip unreferenced plugin commands.

### Tray menu, and rebuilding it when the profile list changes

The tray is built in `setup`, and the menu is **replaced wholesale** on every registry
mutation. Do not try to mutate items in place — `Menu` is a handle to a native object and
partial edits are where this goes wrong.

Confirmed API:

- `TrayIconBuilder::with_id(id)` / `::new()`, `.icon(Image)`, `.menu(&M)`,
  `.show_menu_on_left_click(bool)`, `.on_menu_event(|app, event| ..)`,
  `.on_tray_icon_event(|tray, event| ..)`, `.build(manager) -> Result<TrayIcon<R>>`
- `TrayIcon::set_menu<M: ContextMenu + 'static>(&self, menu: Option<M>) -> Result<()>` —
  takes the menu **by value**, so the tray owns it and you do not have to keep it alive.
- `AppHandle::tray_by_id(&self, id) -> Option<TrayIcon<R>>` — how you get the tray back later.
- `MenuBuilder::new(manager)`, `.text(id, label)`, `.item(&dyn IsMenuItem)`, `.items(&[..])`,
  `.separator()`, `.quit()`, `.build() -> Result<Menu<R>>`
- `MenuItemBuilder::with_id(id, label)`, `.enabled(bool)`, `.accelerator(..)`, `.build(manager)`
- `MenuEvent` has a public `id: MenuId` field and an `id()` accessor.

```rust
// src-tauri/src/tray.rs
use tauri::menu::MenuBuilder;
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Runtime};

pub const TRAY_ID: &str = "cdm-tray";

pub fn build_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<TrayIcon<R>> {
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().unwrap().clone())
        .icon_as_template(true) // macOS: renders correctly in light/dark menu bar
        .menu(&menu_for(app)?)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "new" => open_manager(app, ManagerRoute::New),
            "manage" => open_manager(app, ManagerRoute::List),
            "quit" => app.exit(0),
            other => {
                if let Some(id) = other.strip_prefix("launch:") {
                    let _ = crate::core::profile::launch(app, id);
                }
            }
        })
        .build(app)
}

fn menu_for<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<tauri::menu::Menu<R>> {
    let profiles = crate::core::registry::list(app);
    let mut b = MenuBuilder::new(app);
    if profiles.is_empty() {
        b = b.text("noprofiles", "No profiles").separator();
    } else {
        for p in &profiles {
            b = b.text(format!("launch:{}", p.id), &p.name);
        }
        b = b.separator();
    }
    b.text("new", "New Profile…")
        .text("manage", "Manage Profiles…")
        .separator()
        .quit()
        .build()
}

/// Call after every create / rename / delete / adopt.
pub fn rebuild_tray_menu<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    // muda panics if menus are touched off the main thread on macOS.
    let _ = app.clone().run_on_main_thread(move || {
        if let Some(tray) = app.tray_by_id(TRAY_ID) {
            if let Ok(menu) = menu_for(&app) {
                let _ = tray.set_menu(Some(menu));
            }
        }
    });
}
```

**The part people get wrong** is the thread. `muda`'s own crate docs: *"On macOS, menus can
only be used from the main thread, and most functionality will panic if you try to use it
from any other thread."* Tauri runs **non-`async` commands on the main thread** and
**`async` commands on the async runtime's thread pool**. So:

- If `create_profile` / `rename_profile` / `delete_profile` are plain `fn` commands, calling
  `rebuild_tray_menu` directly is fine.
- The moment one becomes `async fn`, or you rebuild from a spawned task, watcher, or the
  single-instance callback, a direct rebuild **panics on macOS**.

Wrapping in `AppHandle::run_on_main_thread(FnOnce() + Send + 'static)` — confirmed present on
`AppHandle` — is correct in both cases, so always go through the wrapper above and stop
thinking about it.

Encode the profile id, not the display name, in the menu item id (`launch:p_7f3a2c`).
Display names may duplicate; ids are the registry key.

### macOS activation policy

Confirmed on `AppHandle` (macOS-only cfg, 2.11.5):

```rust
#[cfg(target_os = "macos")]
pub fn set_activation_policy(&self, activation_policy: ActivationPolicy) -> Result<()>
#[cfg(target_os = "macos")]
pub fn set_dock_visibility(&self, visible: bool) -> Result<()>
#[cfg(target_os = "macos")]
pub fn show(&self) -> Result<()>   // shows the app without focusing
#[cfg(target_os = "macos")]
pub fn hide(&self) -> Result<()>
```

Note the distinction that trips people up: `App::set_activation_policy(&mut self, ..)` (no
`Result`) is the **setup-time** builder-ish call; `AppHandle::set_activation_policy(&self, ..)
-> Result<()>` is the **runtime** one. Only the `AppHandle` form can flip the policy while
the app is running, which is what the spec requires.

Start as `Accessory` in `setup`, go `Regular` when the manager window is shown, go back on
hide:

```rust
// lib.rs — setup
#[cfg(target_os = "macos")]
app.handle().set_activation_policy(tauri::ActivationPolicy::Accessory)?;
```

```rust
// call sites: show_manager() and the hide-on-close handler
#[cfg(target_os = "macos")]
fn set_policy(app: &tauri::AppHandle, regular: bool) {
    let _ = app.set_activation_policy(if regular {
        tauri::ActivationPolicy::Regular
    } else {
        tauri::ActivationPolicy::Accessory
    });
}
```

Order matters when showing: switch to `Regular` **before** `window.show()` +
`set_focus()`, otherwise the window can come up behind the frontmost app. Switch back to
`Accessory` **after** the window is hidden.

`bundle.macOS.infoPlist` is a real key in the v2 config schema, and the Tauri CLI also merges
a `src-tauri/Info.plist` if present. Setting `LSUIElement: true` there makes the app start
Dock-less before Rust runs, avoiding a one-frame Dock icon flash at launch — the runtime
`set_activation_policy` calls still work on top of it.

**UNVERIFIED:** whether setting `LSUIElement: true` prevents a later
`set_activation_policy(Regular)` from putting the icon in the Dock. Test on hardware; if it
conflicts, drop `LSUIElement` and accept the launch flash, since the runtime API is the
load-bearing one.

Windows: no equivalent, and none needed — the tray icon is the only persistent presence.
Guard every call with `#[cfg(target_os = "macos")]`; the methods do not exist on other
targets, so an un-cfg'd call is a compile error, not a no-op.

### Window hide-on-close

The event is `WindowEvent::CloseRequested { api: CloseRequestApi }` (confirmed variant, and
`CloseRequestApi::prevent_close(&self)` is confirmed). Register it on
`Builder::on_window_event`:

```rust
.on_window_event(|window, event| {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window.hide();
        #[cfg(target_os = "macos")]
        {
            let _ = window.app_handle()
                .set_activation_policy(tauri::ActivationPolicy::Accessory);
        }
    }
})
```

`WindowEvent` and its `CloseRequested` variant are both `#[non_exhaustive]`, so you must use
`if let` / a `_ =>` arm — a total `match` will not compile.

Two related pieces:

- Configure the manager window with `"visible": false` in `tauri.conf.json` so cdm starts as
  tray-only and never flashes a window on login.
- Because the window is only hidden, the app never runs out of windows, so the usual
  "app quits when the last window closes" path is not hit. If you ever also want to survive a
  programmatic exit, `RunEvent::ExitRequested { code, api }` gives you `ExitRequestApi` —
  guard on `code.is_none()` (user-initiated) and `api.prevent_exit()`. Only *Quit* in the tray
  menu should actually exit.

### Detached child processes

**Recommendation: `std::process::Command` directly. Do not use `tauri-plugin-shell`.**

This is not a style preference. The shell plugin's source registers every spawned child in a
`ChildStore` and, in its `on_event` handler, does this on `RunEvent::Exit`:

```rust
if let RunEvent::Exit = event {
    let children = { /* drain the child store */ };
    for child in children.into_values() {
        let _ = child.kill();
    }
}
```

Quitting cdm would kill every running Claude Desktop profile. It also hard-codes
`stdout/stdin/stderr` to `Stdio::piped()` and `creation_flags(CREATE_NO_WINDOW)` on Windows,
and exposes no `pre_exec`, no `process_group`, and no custom creation flags. There is no
configuration that makes it correct for this use.

`std::process::Command` gives the pid via `Child::id()` and does **not** kill on drop on
either platform.

```rust
use std::process::{Command, Stdio};

pub fn launch(binary: &Path, data_dir: &Path, attached: bool) -> io::Result<u32> {
    let mut cmd = Command::new(binary);
    cmd.arg(format!("--user-data-dir={}", data_dir.display()));

    if attached {
        // `cdm launch --wait`: stream Claude's stderr into the CLI.
        cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    } else {
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0); // stable since Rust 1.64
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }

    let child = cmd.spawn()?;
    let pid = child.id();
    reap_in_background(pid, child); // see below
    Ok(pid)
}
```

Why each piece:

- **Unix `process_group(0)`** (`CommandExt::process_group(&mut self, pgroup: i32)`, stable
  1.64) is `setpgid(0, 0)` in the child. It puts Claude in its own process group so a
  `Ctrl-C` in the terminal running `cdm launch` does not take the profile down with it.
  Prefer it over `unsafe pre_exec(|| libc::setsid())`: `pre_exec` closures must be
  async-signal-safe and force the slow fork/exec path, and a full `setsid()` buys nothing
  here — a Unix child does not die when its parent exits, so no extra work is needed for
  survival itself.
- **Windows `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`.** `DETACHED_PROCESS` (0x00000008)
  means the child does not inherit cdm's console; `CREATE_NEW_PROCESS_GROUP` (0x00000200)
  roots a new group so console Ctrl events do not propagate. Values confirmed on Microsoft
  Learn's *Process Creation Flags*.
- **`CREATE_NO_WINDOW` — do not add it here.** Microsoft Learn, verbatim: *"This flag is
  ignored if the application is not a console application, or if it is used with either
  **CREATE_NEW_CONSOLE** or **DETACHED_PROCESS**."* Claude Desktop is a GUI app and we
  already pass `DETACHED_PROCESS`, so it is doubly a no-op. `CREATE_NO_WINDOW` (0x08000000)
  **is** the right flag for the *console* helpers cdm shells out to — notably
  `taskkill /F /PID <pid>` in the escalation path — where without it a console window flashes.
  Use it there and only there.
- Also note `DETACHED_PROCESS` **cannot** be combined with `CREATE_NEW_CONSOLE`; Microsoft
  Learn states this explicitly on both entries.

**Reaping.** On Unix an un-`wait`ed `Child` becomes a zombie for as long as cdm lives, which
would corrupt pid-based `is_running`. Spawn one thread per launch that owns the `Child`:

```rust
fn reap_in_background(pid: u32, mut child: std::process::Child) {
    std::thread::spawn(move || {
        let _ = child.wait();          // reaps on Unix, closes the handle on Windows
        crate::core::running::forget(pid);
    });
}
```

This doubles as the "profile exited" signal for the in-memory pid map, at the cost of one
parked thread per running profile — acceptable at the scale of a handful of profiles.

**Windows job-object caveat.** If cdm is itself launched inside a Job Object configured with
`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, children inherit the job and die with cdm.
`CREATE_BREAKAWAY_FROM_JOB` (0x01000000) escapes it, but only if the job sets
`JOB_OBJECT_LIMIT_BREAKAWAY_OK` — otherwise `CreateProcess` fails outright. Do not add it
unconditionally. **UNVERIFIED:** whether the Windows shells cdm will realistically be started
from (Explorer, Start Menu, Terminal) place processes in such a job. Verify on hardware
during the Windows milestone; if they do, add the flag with a fallback retry without it.

macOS is unaffected: spawning the executable inside `Claude.app/Contents/MacOS/` directly
(as the spec requires, for the pid) creates an ordinary child that outlives cdm.

### Atomic JSON write

`tempfile::NamedTempFile::persist()` atomically replaces the target — but its own docs are
explicit: *"neither the file contents nor the containing directory are synchronized, so the
update may not yet have reached the disk when persist returns."* You must fsync yourself.

```rust
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use tempfile::NamedTempFile;

pub fn write_json_atomic<T: serde::Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let dir = path.parent().expect("registry path has a parent");
    fs::create_dir_all(dir)?;

    // Temp file MUST live in the same directory: rename is only atomic within a filesystem.
    let mut tmp = NamedTempFile::new_in(dir)?;
    {
        let mut w = BufWriter::new(tmp.as_file_mut());
        serde_json::to_writer_pretty(&mut w, value)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        w.write_all(b"\n")?;
        w.flush()?;
    }
    tmp.as_file().sync_all()?;                 // 1. contents durable
    tmp.persist(path).map_err(|e| e.error)?;   // 2. atomic replace
    sync_parent_dir(dir);                      // 3. the rename itself durable
    Ok(())
}

#[cfg(unix)]
fn sync_parent_dir(dir: &Path) {
    if let Ok(d) = File::open(dir) {
        let _ = d.sync_all();
    }
}

#[cfg(windows)]
fn sync_parent_dir(_dir: &Path) {
    // No-op: see below.
}
```

The three steps are all load-bearing. Skipping (1) and you can end up with a correctly-named
file full of zeros after a power loss. Skipping (3) on Unix and the rename itself may not have
reached disk.

**What differs on Windows:**

1. **You cannot fsync a directory.** `File::open(dir)` fails on Windows without
   `FILE_FLAG_BACKUP_SEMANTICS` (reachable via
   `std::os::windows::fs::OpenOptionsExt::custom_flags`), and even with a valid directory
   handle `FlushFileBuffers` is not supported on directory handles. Make step 3 a
   `#[cfg]`-gated no-op and rely on NTFS metadata journaling for the rename. Step 1 is
   *more* important on Windows precisely because of this — NTFS journals the rename's metadata
   but not your file's data, so an unsynced write can land as a correctly-named empty file.
2. **`rename` semantics differ.** `std::fs::rename` maps to `MoveFileExW` **or**
   `SetFileInformationByHandle` on Windows (verbatim from the std docs); POSIX-like
   replace-if-exists behaviour requires Windows 10 1607+ *and* filesystem support for
   `FileRenameInfoEx`. On older paths, `from` can be anything but `to` must not be a
   directory. For `registry.json` — always a file, replacing a file — this is fine.
3. **Replace can fail because someone has the target open.** If an editor, backup tool, or
   AV scanner holds `registry.json` open without `FILE_SHARE_DELETE`, the replace returns
   `ERROR_ACCESS_DENIED` / `ERROR_SHARING_VIOLATION`. On Unix this never happens. Wrap the
   `persist` step in a short bounded retry (e.g. 5 attempts, 20/40/80/160/320 ms) before
   surfacing an error. `NamedTempFile::persist` returns `PersistError` containing the original
   `NamedTempFile`, so retrying is cheap — you do not have to re-serialize.

Do **not** use `persist_noclobber`: its own docs say it is not guaranteed atomic on all
platforms, and we specifically want replace-if-exists.

Read side: on startup, if `registry.json` fails to parse, do not overwrite it. Rename it to
`registry.json.corrupt-<timestamp>` and start from an empty registry — reconciliation will
re-adopt every folder carrying a `.cdm-profile`, so almost nothing is lost, and the user
still has the file.

### CLI subcommand in a GUI binary

The Tauri app is entered from `main.rs`. Dispatch on argv **before** touching `tauri::Builder`
— there is no supported way to "cancel" a Tauri run once started, and starting it would create
a tray icon for a `cdm list` invocation.

```rust
// src-tauri/src/main.rs
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // A subcommand is present iff argv[1] exists and is not an OS-injected flag.
    // macOS passes -psn_0_xxxxx when launched via LaunchServices; ignore it.
    let is_cli = args.len() > 1 && !args[1].starts_with("-psn_");

    if is_cli {
        #[cfg(windows)]
        cdm_lib::cli::attach_parent_console();
        std::process::exit(cdm_lib::cli::run(&args));
    }

    cdm_lib::run();
}
```

Note `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` — the standard
Tauri template line. Debug builds stay on the **console** subsystem, so `cargo run -- list`
prints normally during development and the console problem below only bites release builds.

**The Windows subsystem problem.** A `/SUBSYSTEM:WINDOWS` binary is not given a console. Run
`cdm.exe list` from `cmd` or PowerShell and two things happen: the shell returns to the prompt
immediately instead of waiting, and every `println!` goes to an invalid handle — output
vanishes. Microsoft Learn's `AttachConsole` page states it directly: *"the standard handles
retrieved with GetStdHandle will likely be invalid on startup until AttachConsole is called."*

Fix — attach to the parent's console at the top of the CLI path:

```rust
// src-tauri/src/cli.rs
#[cfg(windows)]
pub fn attach_parent_console() {
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    // ATTACH_PARENT_PROCESS is (DWORD)-1.
    // Fails with ERROR_INVALID_HANDLE if the parent has no console
    // (e.g. launched from Explorer) — that case has nowhere to print anyway.
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}
```

`AttachConsole` and `ATTACH_PARENT_PROCESS` are both confirmed present in
`windows_sys::Win32::System::Console`. Error codes, from Microsoft Learn: `ERROR_ACCESS_DENIED`
if already attached, `ERROR_INVALID_HANDLE` if the target has no console,
`ERROR_INVALID_PARAMETER` if the process does not exist. Ignoring the return value is correct
here — every failure mode means "no console to write to".

**UNVERIFIED:** whether `AttachConsole` alone is enough to make Rust's `println!` work, or
whether you must additionally `CreateFileW("CONOUT$", ..)` and `SetStdHandle` to rebind
stdout/stderr. Microsoft's wording implies `GetStdHandle` becomes valid after attaching, and
Rust's Windows stdout queries `GetStdHandle` rather than caching at startup, but this is the
one thing in this section that genuinely needs a hardware test. If output is still swallowed,
add the `CreateFileW("CONOUT$") + SetStdHandle(STD_OUTPUT_HANDLE, ..)` pair immediately after
the `AttachConsole` call, before any printing. Budget an hour for this during the Windows
milestone.

Two consequences to accept, not fix:

- The shell prompt returns before cdm finishes, so CLI output interleaves with the next
  prompt. This is inherent to a GUI-subsystem binary and is why the CLI is debug-only. The
  documented workaround is `start /wait cdm.exe list`.
- `cdm launch --wait` streaming Claude's stderr works, but the user is looking at output
  under a returned prompt.

**Alternatives considered and rejected:**

- **Console subsystem + `FreeConsole()` at startup** — a console window flashes on every GUI
  launch. Unacceptable for a login-item tray app.
- **A second `cdm-cli.exe` built as a console subsystem binary** — clean output, correct
  shell blocking, no `AttachConsole` needed. Costs a second binary in the installer and a
  second `[[bin]]` target. This is the correct fallback if `AttachConsole` proves
  insufficient; it is not worth the extra artifact up front for a debug-only surface.
- **`tauri-plugin-cli`** — parses argv inside a *running* Tauri app. Wrong shape entirely.

**Single-instance interaction.** `tauri_plugin_single_instance::init` takes
`FnMut(&AppHandle<R>, Vec<String>, String)` (confirmed signature: app handle, argv, cwd) and
is registered inside the Tauri builder. Because the CLI path `exit`s before the builder ever
runs, `cdm list` while the GUI is running does **not** get routed into the single-instance
callback and does not steal focus. The callback's only job is to show and focus the manager
window (and flip the macOS activation policy to `Regular`).

macOS has no equivalent console problem — a `.app` bundle's inner executable is a normal
Mach-O and inherits the terminal's stdio when invoked as
`Claude-Desktop-Manager.app/Contents/MacOS/cdm list`. Ship a small symlink or shell shim on
`PATH` if you want a bare `cdm` command.
