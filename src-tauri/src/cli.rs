use std::io::Write;
use std::process::Command;

use crate::core::profile;
use crate::core::registry;
use crate::core::types::ProfileStatus;
use crate::platform;

const EXIT_OK: i32 = 0;
const EXIT_ERR: i32 = 1;
const EXIT_USAGE: i32 = 2;
const EXIT_NOT_FOUND: i32 = 3;
const EXIT_AMBIGUOUS: i32 = 4;
const EXIT_REFUSED: i32 = 5;

const MUTATING: [&str; 4] = ["create", "launch", "rename", "delete"];

const USAGE: &str = "\
cdm - debug CLI, not the product surface

  cdm create <name>                 make the folder, register it
  cdm list                          id, name, folder, running
  cdm launch <id-or-name> [--wait]  launch; --wait blocks until the profile exits
  cdm rename <id-or-name> <new>     move the folder, update the registry
  cdm delete <id-or-name> [--yes]   confirm, trash the folder, unregister
  cdm doctor                        binary discovery + reconciliation report

exit codes: 0 ok, 1 error, 2 usage, 3 not found, 4 ambiguous name, 5 refused";

struct Fail(i32, String);
type Rv<T> = std::result::Result<T, Fail>;
type R = Rv<()>;
type Row<'a> = (&'a str, &'a str, &'a str, bool);

fn fail(code: i32, msg: impl Into<String>) -> Fail {
    Fail(code, msg.into())
}

fn core_err(e: impl std::fmt::Display) -> Fail {
    fail(EXIT_ERR, e.to_string())
}

fn row(s: &ProfileStatus) -> Row<'_> {
    (&s.profile.id, &s.profile.name, &s.profile.dir, s.running_pid.is_some())
}

pub fn run(args: Vec<String>) -> i32 {
    let argv: Vec<&str> =
        args.iter().skip(1).map(String::as_str).filter(|a| !a.starts_with("-psn_")).collect();
    match dispatch(&argv) {
        Ok(()) => EXIT_OK,
        Err(Fail(code, msg)) => {
            eprintln!("cdm: {msg}");
            code
        }
    }
}

fn dispatch(argv: &[&str]) -> R {
    let Some((&cmd, rest)) = argv.split_first() else {
        return Err(fail(EXIT_USAGE, "no command; try `cdm help`"));
    };
    if matches!(cmd, "help" | "-h" | "--help") {
        println!("{USAGE}");
        return Ok(());
    }
    let (flags, pos): (Vec<&str>, Vec<&str>) =
        rest.iter().copied().partition(|a| a.starts_with("--"));
    let allowed: &[&str] = match cmd {
        "launch" => &["--wait"],
        "delete" => &["--yes"],
        _ => &[],
    };
    if let Some(f) = flags.iter().find(|f| !allowed.contains(*f)) {
        return Err(fail(EXIT_USAGE, format!("unknown flag {f} for `{cmd}`")));
    }
    if MUTATING.contains(&cmd) {
        if let Some(pid) = other_instance_pid() {
            return Err(fail(EXIT_REFUSED, refusal(pid)));
        }
    }
    match cmd {
        "create" => create(&pos),
        "list" => list(),
        "launch" => launch(&pos, flags.contains(&"--wait")),
        "rename" => rename(&pos),
        "delete" => delete(&pos, flags.contains(&"--yes")),
        "doctor" => doctor(),
        _ => Err(fail(EXIT_USAGE, format!("unknown command `{cmd}`"))),
    }
}

fn refusal(pid: u32) -> String {
    format!(
        "another cdm process is running (pid {pid}). It rewrites registry.json whole from \
         memory, so a CLI mutation would be silently lost (audit F9). Quit it, then retry."
    )
}

fn need(pos: &[&str], n: usize, form: &str) -> R {
    (pos.len() == n)
        .then_some(())
        .ok_or_else(|| fail(EXIT_USAGE, format!("usage: cdm {form}")))
}

fn create(pos: &[&str]) -> R {
    need(pos, 1, "create <name>")?;
    let p = profile::create(pos[0]).map_err(core_err)?;
    println!("{}\t{}", p.id, p.dir);
    Ok(())
}

fn list() -> R {
    let profiles = profile::list().map_err(core_err)?;
    let rows: Vec<Row> = profiles.iter().map(row).collect();
    let head = ("ID", "NAME", "FOLDER");
    let (mut a, mut b, mut c) = (head.0.len(), head.1.len(), head.2.len());
    for (id, name, dir, _) in &rows {
        a = a.max(id.len());
        b = b.max(name.len());
        c = c.max(dir.len());
    }
    println!("{:<a$}  {:<b$}  {:<c$}  RUNNING", head.0, head.1, head.2);
    for (id, name, dir, running) in &rows {
        let state = if *running { "yes" } else { "no" };
        println!("{id:<a$}  {name:<b$}  {dir:<c$}  {state}");
    }
    Ok(())
}

fn launch(pos: &[&str], wait: bool) -> R {
    need(pos, 1, "launch <id-or-name> [--wait]")?;
    let status = resolve(pos[0])?;
    let (id, _, dir, _) = row(&status);
    let pid = profile::launch(id).map_err(core_err)?;
    println!("{pid}\t{dir}");
    if wait {
        while profile::list()
            .map(|l| l.iter().any(|s| row(s).0 == id && row(s).3))
            .unwrap_or(false)
        {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        println!("exited\t{pid}");
    }
    Ok(())
}

fn rename(pos: &[&str]) -> R {
    need(pos, 2, "rename <id-or-name> <new-name>")?;
    let status = resolve(pos[0])?;
    let p = profile::rename(row(&status).0, pos[1]).map_err(core_err)?;
    println!("{}\t{}", p.id, p.dir);
    Ok(())
}

fn delete(pos: &[&str], yes: bool) -> R {
    need(pos, 1, "delete <id-or-name> [--yes]")?;
    let status = resolve(pos[0])?;
    let (id, name, dir, running) = row(&status);
    if running {
        return Err(fail(EXIT_REFUSED, format!("`{name}` ({dir}) is running; quit it first")));
    }
    let ask = format!("delete `{name}` -> {dir}? the login session is lost [y/N] ");
    if !yes && !confirm(&ask) {
        return Err(fail(EXIT_REFUSED, "aborted"));
    }
    profile::delete(id).map_err(core_err)?;
    println!("deleted\t{id}\t{dir}");
    Ok(())
}

fn doctor() -> R {
    match platform::current().find_claude_binary() {
        Ok(path) => println!("binary\t{}", path.display()),
        Err(e) => println!("binary\tNOT FOUND: {e} (set CDM_CLAUDE_BINARY)"),
    }
    match other_instance_pid() {
        Some(pid) => println!("reconcile\tskipped: cdm pid {pid} may write registry.json"),
        None => match registry::load().and_then(|mut r| registry::reconcile(&mut r)) {
            Ok(found) => println!("reconcile\t{found:?}"),
            Err(e) => println!("reconcile\tunavailable: {e}"),
        },
    }
    list()
}

fn resolve(key: &str) -> Rv<ProfileStatus> {
    let mut profiles = profile::list().map_err(core_err)?;
    if let Some(i) = profiles.iter().position(|s| row(s).0 == key) {
        return Ok(profiles.swap_remove(i));
    }
    let hits: Vec<usize> = (0..profiles.len())
        .filter(|&i| row(&profiles[i]).1 == key)
        .collect();
    match hits.as_slice() {
        [i] => Ok(profiles.swap_remove(*i)),
        [] => Err(fail(EXIT_NOT_FOUND, format!("no profile with id or name `{key}`"))),
        _ => {
            let ids: Vec<&str> = hits.iter().map(|&i| row(&profiles[i]).0).collect();
            let (n, ids) = (hits.len(), ids.join(", "));
            let msg = format!("`{key}` matches {n} profiles ({ids}); address it by id");
            Err(fail(EXIT_AMBIGUOUS, msg))
        }
    }
}

fn confirm(prompt: &str) -> bool {
    eprint!("{prompt}");
    let _ = std::io::stderr().flush();
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer).is_ok() && matches!(answer.trim(), "y" | "Y" | "yes")
}

#[cfg(unix)]
fn other_instance_pid() -> Option<u32> {
    let exe = std::env::current_exe().ok()?;
    let exe = exe.to_str()?;
    let out = Command::new("/bin/ps").args(["-Ao", "pid=,comm="]).output().ok()?;
    let me = std::process::id();
    String::from_utf8_lossy(&out.stdout).lines().find_map(|line| {
        let (pid, comm) = line.trim_start().split_once(' ')?;
        let pid: u32 = pid.parse().ok()?;
        (pid != me && comm.trim() == exe).then_some(pid)
    })
}

#[cfg(windows)]
fn other_instance_pid() -> Option<u32> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let exe = std::env::current_exe().ok()?;
    let name = exe.file_name()?.to_str()?.to_owned();
    let out = Command::new("tasklist")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["/FO", "CSV", "/NH", "/FI", &format!("IMAGENAME eq {name}")])
        .output()
        .ok()?;
    let me = std::process::id();
    String::from_utf8_lossy(&out.stdout).lines().find_map(|line| {
        let pid: u32 = line.split(',').nth(1)?.trim_matches('"').parse().ok()?;
        (pid != me).then_some(pid)
    })
}

#[cfg(windows)]
pub fn attach_parent_console() {
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    // Every failure means "no console to write to". UNVERIFIED (plan/02): if output is still
    // swallowed, CONOUT$ + SetStdHandle must follow this call before anything prints.
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}
