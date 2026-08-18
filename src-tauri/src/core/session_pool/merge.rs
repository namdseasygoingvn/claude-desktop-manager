use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct CopyOp {
    pub relative_path: PathBuf,
}

pub struct MergePlan {
    pub copies: Vec<CopyOp>,
    /// Source entries whose metadata or bytes could not be read; skipped, not fatal.
    pub unreadable: Vec<PathBuf>,
}

/// Infallible: a missing `source` or `dest` root is an empty tree, not an error.
pub fn plan(source: &Path, dest: &Path) -> MergePlan {
    let (source_files, source_unreadable) = walk(source);
    let (dest_files, dest_unreadable) = walk(dest);

    let copies = source_files
        .into_iter()
        .filter(|(rel, mtime)| match dest_files.get(rel) {
            Some(dest_mtime) => mtime > dest_mtime,
            None => true,
        })
        .map(|(relative_path, _)| CopyOp { relative_path })
        .collect();

    let mut unreadable = source_unreadable;
    unreadable.extend(dest_unreadable);

    MergePlan { copies, unreadable }
}

pub struct ApplyOutcome {
    pub copied: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
}

/// Best-effort: every op is attempted; a single failed copy never stops the rest.
pub fn apply(source: &Path, dest: &Path, plan: &MergePlan) -> ApplyOutcome {
    let mut copied = Vec::new();
    let mut failed = Vec::new();

    for op in &plan.copies {
        match copy_one(source, dest, &op.relative_path) {
            Ok(()) => copied.push(op.relative_path.clone()),
            Err(e) => failed.push((op.relative_path.clone(), e.to_string())),
        }
    }

    ApplyOutcome { copied, failed }
}

fn copy_one(source: &Path, dest: &Path, relative_path: &Path) -> std::io::Result<()> {
    let to = dest.join(relative_path);
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source.join(relative_path), &to)?;
    Ok(())
}

fn walk(root: &Path) -> (BTreeMap<PathBuf, SystemTime>, Vec<PathBuf>) {
    let mut files = BTreeMap::new();
    let mut unreadable = Vec::new();
    walk_dir(root, Path::new(""), &mut files, &mut unreadable);
    (files, unreadable)
}

fn walk_dir(
    dir: &Path,
    prefix: &Path,
    files: &mut BTreeMap<PathBuf, SystemTime>,
    unreadable: &mut Vec<PathBuf>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }

        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        let rel = prefix.join(&name);
        if file_type.is_dir() {
            walk_dir(&entry.path(), &rel, files, unreadable);
        } else if file_type.is_file() {
            match fs::metadata(entry.path()).and_then(|m| m.modified()) {
                Ok(mtime) => {
                    files.insert(rel, mtime);
                }
                Err(_) => unreadable.push(rel),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::time::Duration;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    fn set_mtime(path: &Path, time: SystemTime) {
        let file = File::options().write(true).open(path).unwrap();
        file.set_times(fs::FileTimes::new().set_modified(time)).unwrap();
    }

    #[test]
    fn a_new_source_file_absent_from_dest_is_copied_over() {
        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        write(&source.path().join("a.json"), "new");
        write(&dest.path().join("b.json"), "existing");

        let merge_plan = plan(source.path(), dest.path());
        assert_eq!(merge_plan.copies.len(), 1);
        assert_eq!(merge_plan.copies[0].relative_path, PathBuf::from("a.json"));

        let outcome = apply(source.path(), dest.path(), &merge_plan);
        assert_eq!(outcome.copied, vec![PathBuf::from("a.json")]);
        assert!(outcome.failed.is_empty());
        assert_eq!(fs::read_to_string(dest.path().join("a.json")).unwrap(), "new");
        assert_eq!(fs::read_to_string(dest.path().join("b.json")).unwrap(), "existing");
    }

    #[test]
    fn same_path_with_equal_mtimes_keeps_the_dest_file_untouched() {
        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        write(&source.path().join("local_x.json"), "from source");
        write(&dest.path().join("local_x.json"), "from dest");
        let tied = SystemTime::now();
        set_mtime(&source.path().join("local_x.json"), tied);
        set_mtime(&dest.path().join("local_x.json"), tied);

        let merge_plan = plan(source.path(), dest.path());
        assert!(merge_plan.copies.is_empty());

        let outcome = apply(source.path(), dest.path(), &merge_plan);
        assert!(outcome.copied.is_empty());
        assert_eq!(fs::read_to_string(dest.path().join("local_x.json")).unwrap(), "from dest");
    }

    #[test]
    fn a_strictly_newer_source_file_overwrites_the_older_dest_file() {
        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        write(&source.path().join("local_x.json"), "newer");
        write(&dest.path().join("local_x.json"), "older");
        let base = SystemTime::now();
        set_mtime(&dest.path().join("local_x.json"), base);
        set_mtime(&source.path().join("local_x.json"), base + Duration::from_secs(5));

        let merge_plan = plan(source.path(), dest.path());
        assert_eq!(merge_plan.copies.len(), 1);

        apply(source.path(), dest.path(), &merge_plan);
        assert_eq!(fs::read_to_string(dest.path().join("local_x.json")).unwrap(), "newer");
    }

    #[test]
    fn a_deeply_nested_new_file_is_planned_with_its_full_relative_path_and_copied() {
        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        let nested = PathBuf::from("account-uuid").join("sub-uuid").join("local_new.json");
        write(&source.path().join(&nested), "session");

        let merge_plan = plan(source.path(), dest.path());
        assert_eq!(merge_plan.copies.len(), 1);
        assert_eq!(merge_plan.copies[0].relative_path, nested);

        let outcome = apply(source.path(), dest.path(), &merge_plan);
        assert_eq!(outcome.copied, vec![nested.clone()]);
        assert_eq!(fs::read_to_string(dest.path().join(&nested)).unwrap(), "session");
    }

    #[test]
    fn dot_noise_is_skipped_at_every_depth_not_just_the_top() {
        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        write(&source.path().join(".DS_Store"), "junk");
        write(&source.path().join("sub/.DS_Store"), "junk");
        write(&source.path().join("sub/local_x.json"), "session");

        let merge_plan = plan(source.path(), dest.path());
        assert_eq!(merge_plan.copies.len(), 1);
        assert_eq!(merge_plan.copies[0].relative_path, PathBuf::from("sub/local_x.json"));
    }

    #[test]
    fn a_tombstone_and_a_live_file_at_different_paths_are_both_kept_with_no_special_casing() {
        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        write(&source.path().join("deleted_abc"), "");
        write(&dest.path().join("local_abc.json"), "still here");

        let merge_plan = plan(source.path(), dest.path());
        assert_eq!(merge_plan.copies.len(), 1);
        assert_eq!(merge_plan.copies[0].relative_path, PathBuf::from("deleted_abc"));

        apply(source.path(), dest.path(), &merge_plan);
        assert!(dest.path().join("deleted_abc").exists());
        assert!(dest.path().join("local_abc.json").exists());
    }

    #[test]
    fn a_missing_source_root_plans_as_an_empty_tree_not_an_error() {
        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        write(&dest.path().join("local_x.json"), "kept");
        let missing_source = source.path().join("never-created");

        let merge_plan = plan(&missing_source, dest.path());
        assert!(merge_plan.copies.is_empty());
        assert!(merge_plan.unreadable.is_empty());
    }

    #[test]
    fn a_missing_dest_root_still_plans_every_source_file_as_a_copy() {
        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        write(&source.path().join("local_x.json"), "fresh join");
        let missing_dest = dest.path().join("never-created");

        let merge_plan = plan(source.path(), &missing_dest);
        assert_eq!(merge_plan.copies.len(), 1);

        let outcome = apply(source.path(), &missing_dest, &merge_plan);
        assert!(outcome.failed.is_empty());
        assert_eq!(fs::read_to_string(missing_dest.join("local_x.json")).unwrap(), "fresh join");
    }

    #[test]
    #[cfg(unix)]
    fn an_unreadable_source_file_is_skipped_and_the_rest_of_the_tree_still_merges() {
        use std::os::unix::fs::PermissionsExt;

        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        write(&source.path().join("sub/secret.json"), "locked");
        write(&source.path().join("local_ok.json"), "readable");
        fs::set_permissions(source.path().join("sub"), fs::Permissions::from_mode(0o400)).unwrap();

        let merge_plan = plan(source.path(), dest.path());

        fs::set_permissions(source.path().join("sub"), fs::Permissions::from_mode(0o700)).unwrap();

        assert_eq!(merge_plan.unreadable, vec![PathBuf::from("sub/secret.json")]);
        assert_eq!(merge_plan.copies.len(), 1);
        assert_eq!(merge_plan.copies[0].relative_path, PathBuf::from("local_ok.json"));
    }

    #[test]
    fn apply_continues_past_a_failed_copy_and_still_copies_the_rest() {
        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        write(&source.path().join("local_ok.json"), "fine");

        let missing_op = CopyOp { relative_path: PathBuf::from("local_gone.json") };
        let ok_op = CopyOp { relative_path: PathBuf::from("local_ok.json") };
        let merge_plan = MergePlan { copies: vec![missing_op, ok_op], unreadable: Vec::new() };

        let outcome = apply(source.path(), dest.path(), &merge_plan);
        assert_eq!(outcome.copied, vec![PathBuf::from("local_ok.json")]);
        assert_eq!(outcome.failed.len(), 1);
        assert_eq!(outcome.failed[0].0, PathBuf::from("local_gone.json"));
    }
}
