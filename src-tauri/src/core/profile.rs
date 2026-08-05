//! Profile lifecycle: create, list, launch, rename, delete, quit, adopt.

use std::collections::hash_map::RandomState;
use std::ffi::OsStr;
use std::fs;
use std::hash::{BuildHasher, Hash, Hasher};
use std::io::ErrorKind;
use std::path::{Component, Path};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::naming;
use super::registry;
use super::types::{CdmError, Profile, ProfileStatus, Registry, Result};
use crate::platform::{self, Platform};

pub const MARKER_FILE: &str = ".cdm-profile";
pub const CONFIG_FILE: &str = "claude_desktop_config.json";
pub const EMPTY_CONFIG: &str = "{\n  \"mcpServers\": {}\n}\n";
pub const UNMANAGED_DIR: &str = "Claude";

pub fn create(name: &str) -> Result<Profile> {
    let name = non_empty(name)?;
    let plat = platform::current();
    let root = plat.profiles_root()?;
    let mut reg = registry::load()?;

    let dir_name = resolve_dir(name, &root)?;
    let dir = root.join(&dir_name);
    create_dir_exclusive(&dir, &dir_name)?;

    let profile = Profile {
        id: mint_id(&reg),
        name: name.to_string(),
        dir: dir_name,
        created_at: now_rfc3339(),
        last_used_at: None,
    };

    let built = populate(&dir, &profile.id).and_then(|()| {
        reg.profiles.push(profile.clone());
        registry::save(&reg)
    });

    if let Err(e) = built {
        // Reachable only past a successful create_dir, so this can never remove a
        // directory that was already the user's.
        let _ = fs::remove_dir_all(&dir);
        return Err(e);
    }
    Ok(profile)
}

pub fn list() -> Result<Vec<ProfileStatus>> {
    let plat = platform::current();
    let root = plat.profiles_root()?;
    let reg = registry::load()?;

    Ok(reg
        .profiles
        .into_iter()
        .map(|profile| {
            let running_pid = plat.is_running(&root.join(&profile.dir)).unwrap_or(None);
            ProfileStatus {
                profile,
                running_pid,
            }
        })
        .collect())
}

pub fn launch(id: &str) -> Result<u32> {
    let plat = platform::current();
    let root = plat.profiles_root()?;
    let mut reg = registry::load()?;
    let idx = index_of(&reg, id)?;
    let dir = root.join(&reg.profiles[idx].dir);

    if !dir.is_dir() {
        return Err(CdmError::Other(format!(
            "profile folder missing: {}",
            dir.display()
        )));
    }

    let binary = plat.find_claude_binary()?;
    ensure_config(&dir)?;
    let pid = plat.launch(&binary, &dir)?;

    reg.profiles[idx].last_used_at = Some(now_rfc3339());
    // Already running: a failed timestamp write must not read back as a failed launch.
    let _ = registry::save(&reg);

    Ok(pid)
}

pub fn rename(id: &str, new_name: &str) -> Result<Profile> {
    let new_name = non_empty(new_name)?;
    let plat = platform::current();
    let root = plat.profiles_root()?;
    let mut reg = registry::load()?;
    let idx = index_of(&reg, id)?;
    let current_dir = reg.profiles[idx].dir.clone();

    if !folder_matches(&current_dir, new_name) {
        let from = root.join(&current_dir);
        if plat.is_running(&from)?.is_some() {
            return Err(CdmError::ProfileRunning(reg.profiles[idx].name.clone()));
        }

        let target = resolve_dir(new_name, &root)?;
        // No copy-and-delete fallback: a non-atomic move can leave two folders carrying one id,
        // which reconciliation cannot resolve. Cross-volume and busy-file failures surface as-is.
        fs::rename(&from, root.join(&target))
            .map_err(|e| CdmError::Io(format!("move {current_dir} to {target}: {e}")))?;
        reg.profiles[idx].dir = target;
    }

    reg.profiles[idx].name = new_name.to_string();
    registry::save(&reg)?;
    Ok(reg.profiles[idx].clone())
}

pub fn delete(id: &str) -> Result<()> {
    let plat = platform::current();
    let root = plat.profiles_root()?;
    let mut reg = registry::load()?;
    let idx = index_of(&reg, id)?;
    let dir = root.join(&reg.profiles[idx].dir);

    if plat.is_running(&dir)?.is_some() {
        return Err(CdmError::ProfileRunning(reg.profiles[idx].name.clone()));
    }

    // Trash before unregistering: a crash between the two leaves a folder reconciliation can
    // re-adopt, rather than a registry entry pointing at nothing.
    if dir.symlink_metadata().is_ok() {
        plat.trash(&dir)?;
    }

    reg.profiles.remove(idx);
    registry::save(&reg)
}

pub fn quit(id: &str) -> Result<()> {
    let plat = platform::current();
    let root = plat.profiles_root()?;
    let reg = registry::load()?;
    let idx = index_of(&reg, id)?;
    let dir = root.join(&reg.profiles[idx].dir);

    match plat.is_running(&dir)? {
        Some(pid) => plat.terminate(pid, &dir),
        None => Ok(()),
    }
}

pub fn adopt(dir_name: &str, display_name: &str) -> Result<Profile> {
    let name = non_empty(display_name)?;
    let dir_name = single_component(dir_name)?;
    if dir_name.eq_ignore_ascii_case(UNMANAGED_DIR) {
        return Err(CdmError::Other(format!(
            "{UNMANAGED_DIR} is the existing unmanaged install; cdm leaves it alone"
        )));
    }

    let plat = platform::current();
    let dir = plat.profiles_root()?.join(dir_name);
    if !dir.is_dir() {
        return Err(CdmError::Other(format!("no such folder: {}", dir.display())));
    }

    let marker = dir.join(MARKER_FILE);
    let mut reg = registry::load()?;
    let claimed = reg
        .profiles
        .iter()
        .any(|p| p.dir.eq_ignore_ascii_case(dir_name));
    if marker.exists() || claimed {
        return Err(CdmError::DirExists(dir_name.to_string()));
    }

    let profile = Profile {
        id: mint_id(&reg),
        name: name.to_string(),
        dir: dir_name.to_string(),
        created_at: now_rfc3339(),
        last_used_at: None,
    };

    write_file(&marker, &profile.id)?;
    reg.profiles.push(profile.clone());
    if let Err(e) = registry::save(&reg) {
        let _ = fs::remove_file(&marker);
        return Err(e);
    }
    Ok(profile)
}

/// The folder name is asserted to be a single path component before it is ever joined to the
/// profiles root, so no user-typed name can reach outside it.
fn resolve_dir(name: &str, root: &Path) -> Result<String> {
    let folder = naming::resolve_folder(name, root)?;
    single_component(&folder).map(str::to_string)
}

/// Never `create_dir_all`: it returns `Ok(())` for a folder that already exists, which would let
/// cdm write into — and then roll back, i.e. delete — a directory it did not create.
fn create_dir_exclusive(dir: &Path, dir_name: &str) -> Result<()> {
    match fs::create_dir(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {
            Err(CdmError::DirExists(dir_name.to_string()))
        }
        Err(e) => Err(CdmError::Io(format!("create {}: {e}", dir.display()))),
    }
}

fn populate(dir: &Path, id: &str) -> Result<()> {
    write_file(&dir.join(CONFIG_FILE), EMPTY_CONFIG)?;
    // Marker last: it is what makes a folder cdm's, so it is written only once the folder is whole.
    write_file(&dir.join(MARKER_FILE), id)
}

fn ensure_config(dir: &Path) -> Result<()> {
    let config = dir.join(CONFIG_FILE);
    if config.exists() {
        return Ok(());
    }
    write_file(&config, EMPTY_CONFIG)
}

fn write_file(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents).map_err(|e| CdmError::Io(format!("write {}: {e}", path.display())))
}

/// A folder is `<prefix>-<slug>`; comparing stems case-insensitively keeps a case-only or
/// punctuation-only rename a registry-only edit, which would otherwise collide with its own
/// folder and be handed a `-2` suffix.
fn folder_matches(folder: &str, name: &str) -> bool {
    let stem = folder.split_once('-').map_or(folder, |(_, stem)| stem);
    stem.to_lowercase() == naming::slug(name).to_lowercase()
}

fn single_component(dir_name: &str) -> Result<&str> {
    let name = dir_name.trim();
    let mut parts = Path::new(name).components();
    match (parts.next(), parts.next()) {
        (Some(Component::Normal(part)), None) if part == OsStr::new(name) => Ok(name),
        _ => Err(CdmError::Other(format!("not a folder name: {dir_name}"))),
    }
}

fn non_empty(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        Err(CdmError::NameEmpty)
    } else {
        Ok(name)
    }
}

fn index_of(reg: &Registry, id: &str) -> Result<usize> {
    reg.profiles
        .iter()
        .position(|p| p.id == id)
        .ok_or_else(|| CdmError::ProfileNotFound(id.to_string()))
}

pub(crate) fn mint_id(reg: &Registry) -> String {
    loop {
        let id = random_id();
        if !reg.profiles.iter().any(|p| p.id == id) {
            return id;
        }
    }
}

fn random_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let mut hasher = RandomState::new().build_hasher();
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
        .hash(&mut hasher);
    SEQ.fetch_add(1, Ordering::Relaxed).hash(&mut hasher);
    format!("p_{:06x}", hasher.finish() & 0xff_ffff)
}

pub(crate) fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
    let tod = secs.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

/// Hinnant's `civil_from_days`: days since the Unix epoch to a proleptic Gregorian date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = yoe + era * 400;
    (if month <= 2 { year + 1 } else { year }, month, day)
}
