use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tempfile::NamedTempFile;

use super::naming::is_safe_dir;
use super::profile::MARKER_FILE;
use super::types::{CdmError, Registry, Result, REGISTRY_VERSION};
use crate::platform;

const REGISTRY_FILE: &str = "registry.json";
const PERSIST_ATTEMPTS: u32 = 5;
const PERSIST_BACKOFF_MS: u64 = 20;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind")]
pub enum Discrepancy {
    MissingFolder { id: String },
    UnregisteredFolder { dir: String, id: String },
    DuplicateId { id: String, dirs: Vec<String> },
}

pub fn path() -> Result<PathBuf> {
    Ok(platform::current().manager_data_dir()?.join(REGISTRY_FILE))
}

pub fn load() -> Result<Registry> {
    let path = path()?;
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Registry::default()),
        Err(err) => {
            return Err(CdmError::Io(format!(
                "cannot read {}: {err}",
                path.display()
            )))
        }
    };

    match serde_json::from_slice::<Registry>(&bytes) {
        Ok(registry) if registry.version > REGISTRY_VERSION => Err(CdmError::RegistryCorrupt(
            format!(
                "registry.json is version {} but this build understands {REGISTRY_VERSION}",
                registry.version
            ),
        )),
        Ok(registry) => Ok(registry),
        Err(err) => {
            quarantine(&path, &err.to_string())?;
            Ok(Registry::default())
        }
    }
}

pub fn save(registry: &Registry) -> Result<()> {
    let dir = platform::current().manager_data_dir()?;
    fs::create_dir_all(&dir)
        .map_err(|err| CdmError::Io(format!("cannot create {}: {err}", dir.display())))?;

    // rename() is only atomic within a filesystem, so the temp file lives beside the target.
    let mut tmp = NamedTempFile::new_in(&dir)
        .map_err(|err| CdmError::Io(format!("cannot create a temp file in {}: {err}", dir.display())))?;
    {
        let mut writer = BufWriter::new(tmp.as_file_mut());
        serde_json::to_writer_pretty(&mut writer, registry)
            .map_err(|err| CdmError::Io(format!("cannot serialize the registry: {err}")))?;
        writer
            .write_all(b"\n")
            .map_err(|err| CdmError::Io(format!("cannot write the registry: {err}")))?;
        writer
            .flush()
            .map_err(|err| CdmError::Io(format!("cannot write the registry: {err}")))?;
    }
    tmp.as_file()
        .sync_all()
        .map_err(|err| CdmError::Io(format!("cannot flush the registry to disk: {err}")))?;

    persist_with_retry(tmp, &dir.join(REGISTRY_FILE))?;
    sync_parent_dir(&dir);
    Ok(())
}

pub fn reconcile(registry: &mut Registry) -> Result<Vec<Discrepancy>> {
    let root = platform::current().profiles_root()?;
    let markers = scan_markers(&root)?;
    let registered: HashSet<String> = registry.profiles.iter().map(|p| p.id.clone()).collect();
    let mut found = Vec::new();

    for profile in registry.profiles.iter_mut() {
        match markers.get(&profile.id).map(Vec::as_slice) {
            Some(dirs) if dirs.len() > 1 => found.push(Discrepancy::DuplicateId {
                id: profile.id.clone(),
                dirs: dirs.to_vec(),
            }),
            Some([dir]) => {
                if profile.dir != *dir {
                    profile.dir = dir.clone();
                }
            }
            _ => {
                if !folder_exists(&root, &profile.dir) {
                    found.push(Discrepancy::MissingFolder {
                        id: profile.id.clone(),
                    });
                }
            }
        }
    }

    for (id, dirs) in &markers {
        if registered.contains(id) {
            continue;
        }
        for dir in dirs {
            found.push(Discrepancy::UnregisteredFolder {
                dir: dir.clone(),
                id: id.clone(),
            });
        }
    }

    Ok(found)
}

/// A registry `dir` is user-editable text; it is never joined onto the profiles root unvalidated.
fn folder_exists(root: &Path, dir: &str) -> bool {
    is_safe_dir(dir) && root.join(dir).is_dir()
}

/// Every entry in the profiles root, not only `Claude-*`: adopted and Finder-renamed folders
/// need not match the prefix, and a folder invisible to the scan orphans its entry forever.
fn scan_markers(root: &Path) -> Result<BTreeMap<String, Vec<String>>> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(err) => {
            return Err(CdmError::Io(format!(
                "cannot read {}: {err}",
                root.display()
            )))
        }
    };

    let mut markers: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(dir) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Some(id) = read_marker(&path) else {
            continue;
        };
        markers.entry(id).or_default().push(dir);
    }

    // Directory order is not stable across runs; a caller must never see a different answer twice.
    for dirs in markers.values_mut() {
        dirs.sort();
    }
    Ok(markers)
}

fn read_marker(dir: &Path) -> Option<String> {
    let id = fs::read_to_string(dir.join(MARKER_FILE)).ok()?.trim().to_string();
    if id.is_empty() {
        None
    } else {
        Some(id)
    }
}

fn quarantine(path: &Path, reason: &str) -> Result<()> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0);
    let backup = path.with_file_name(format!("{REGISTRY_FILE}.corrupt-{stamp}"));

    fs::rename(path, &backup).map_err(|err| {
        CdmError::RegistryCorrupt(format!(
            "{} is unparseable ({reason}) and could not be moved aside: {err}",
            path.display()
        ))
    })?;

    log::warn!(
        "{} was unparseable ({reason}); moved to {}",
        path.display(),
        backup.display()
    );
    Ok(())
}

fn persist_with_retry(mut tmp: NamedTempFile, path: &Path) -> Result<()> {
    let mut attempts_left = PERSIST_ATTEMPTS;
    let mut backoff = PERSIST_BACKOFF_MS;

    loop {
        match tmp.persist(path) {
            Ok(_) => return Ok(()),
            Err(err) => {
                attempts_left -= 1;
                if attempts_left == 0 {
                    return Err(CdmError::Io(format!(
                        "cannot replace {}: {}",
                        path.display(),
                        err.error
                    )));
                }
                // Windows only: an editor, backup tool or AV scanner holding the target open
                // makes the replace fail transiently with ERROR_SHARING_VIOLATION.
                tmp = err.file;
                std::thread::sleep(Duration::from_millis(backoff));
                backoff *= 2;
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn sync_parent_dir(dir: &Path) {
    if let Ok(handle) = File::open(dir) {
        let _ = handle.sync_all();
    }
}

/// Windows cannot fsync a directory; NTFS journals the rename's metadata instead.
#[cfg(target_os = "windows")]
fn sync_parent_dir(_dir: &Path) {}
