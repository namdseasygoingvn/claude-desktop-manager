use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::naming::is_safe_dir;
use super::persist;
use super::profile::MARKER_FILE;
use super::types::{CdmError, Registry, Result, REGISTRY_VERSION};
use crate::platform;

const REGISTRY_FILE: &str = "registry.json";

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
    persist::write_json(&dir, REGISTRY_FILE, registry, "the registry")
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
    persist::quarantine(path, reason).map_err(|err| match err {
        CdmError::Io(detail) => CdmError::RegistryCorrupt(detail),
        other => other,
    })
}
