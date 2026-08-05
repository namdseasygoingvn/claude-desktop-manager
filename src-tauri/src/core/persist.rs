use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tempfile::NamedTempFile;

use super::types::{CdmError, Result};

const PERSIST_ATTEMPTS: u32 = 5;
const PERSIST_BACKOFF_MS: u64 = 20;

/// Atomic JSON write: a temp file beside the target, fsync'd, then renamed over it.
/// `subject` names the file in error messages ("the registry", "groups").
pub fn write_json<T: Serialize>(dir: &Path, file: &str, value: &T, subject: &str) -> Result<()> {
    fs::create_dir_all(dir)
        .map_err(|err| CdmError::Io(format!("cannot create {}: {err}", dir.display())))?;

    // rename() is only atomic within a filesystem, so the temp file lives beside the target.
    let mut tmp = NamedTempFile::new_in(dir)
        .map_err(|err| CdmError::Io(format!("cannot create a temp file in {}: {err}", dir.display())))?;
    {
        let mut writer = BufWriter::new(tmp.as_file_mut());
        serde_json::to_writer_pretty(&mut writer, value)
            .map_err(|err| CdmError::Io(format!("cannot serialize {subject}: {err}")))?;
        writer
            .write_all(b"\n")
            .map_err(|err| CdmError::Io(format!("cannot write {subject}: {err}")))?;
        writer
            .flush()
            .map_err(|err| CdmError::Io(format!("cannot flush {subject} to disk: {err}")))?;
    }
    tmp.as_file()
        .sync_all()
        .map_err(|err| CdmError::Io(format!("cannot flush {subject} to disk: {err}")))?;

    persist_with_retry(tmp, &dir.join(file))?;
    sync_parent_dir(dir);
    Ok(())
}

/// Move a file that failed to parse aside so the next save starts fresh instead of
/// overwriting the evidence. The caller surfaces the failure.
pub fn quarantine(path: &Path, reason: &str) -> Result<()> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0);
    let file = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("cdm-data");
    let backup = path.with_file_name(format!("{file}.corrupt-{stamp}"));

    fs::rename(path, &backup).map_err(|err| {
        CdmError::Io(format!(
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
