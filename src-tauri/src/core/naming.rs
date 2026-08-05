use std::collections::HashSet;
use std::path::{Component, Path};

use unicode_normalization::UnicodeNormalization;

use super::types::{CdmError, Result};

pub const FOLDER_PREFIX: &str = "Claude-";
pub const MAX_SLUG_LEN: usize = 32;
const FALLBACK_SLUG: &str = "profile";

pub fn slug(name: &str) -> String {
    let mapped: String = name
        .nfc()
        .map(|c| if is_slug_char(c) { c } else { '-' })
        .collect();

    let collapsed = collapse_dashes(&mapped);
    let capped: String = trim_edges(&collapsed).chars().take(MAX_SLUG_LEN).collect();
    let slug = trim_edges(&capped);

    if slug.is_empty() {
        FALLBACK_SLUG.to_string()
    } else {
        slug.to_string()
    }
}

/// The key both sides of every folder comparison are reduced to. APFS `readdir` returns the
/// bytes as stored — often NFD — so a raw comparison against an NFC candidate misses a folder
/// that `create_dir` will then refuse to create.
pub fn normalize_key(s: &str) -> String {
    s.nfc().collect::<String>().to_lowercase()
}

pub fn same_folder(a: &str, b: &str) -> bool {
    normalize_key(a) == normalize_key(b)
}

/// Guards the registry's `dir` field, which reaches both `--user-data-dir` and the delete path.
pub fn is_safe_dir(dir: &str) -> bool {
    let path = Path::new(dir);
    !dir.is_empty()
        && !dir.contains(['/', '\\', ':', '\0'])
        && path.components().count() == 1
        && matches!(path.components().next(), Some(Component::Normal(_)))
}

pub fn resolve_folder(name: &str, profiles_root: &Path) -> Result<String> {
    if name.trim().is_empty() {
        return Err(CdmError::NameEmpty);
    }

    let stem = slug(name);
    let occupied = occupied_keys(profiles_root)?;
    let mut suffix = 1u32;

    loop {
        let candidate = if suffix == 1 {
            format!("{FOLDER_PREFIX}{stem}")
        } else {
            format!("{FOLDER_PREFIX}{stem}-{suffix}")
        };

        if !occupied.contains(&normalize_key(&candidate)) {
            if !is_safe_dir(&candidate) || !is_generated_dir(&candidate) {
                return Err(CdmError::Other(format!(
                    "derived folder name is not a single path component: {candidate}"
                )));
            }
            return Ok(candidate);
        }

        suffix += 1;
    }
}

/// Every entry in the profiles root, not just directories, not just registered ones, not just
/// `Claude-*`: a name cdm has no registry entry for is still a name cdm must not occupy.
fn occupied_keys(profiles_root: &Path) -> Result<HashSet<String>> {
    let entries = match std::fs::read_dir(profiles_root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(HashSet::new()),
        Err(err) => {
            return Err(CdmError::Io(format!(
                "cannot read {}: {err}",
                profiles_root.display()
            )))
        }
    };

    Ok(entries
        .flatten()
        .map(|entry| normalize_key(&entry.file_name().to_string_lossy()))
        .collect())
}

fn is_generated_dir(dir: &str) -> bool {
    match dir.strip_prefix(FOLDER_PREFIX) {
        Some(stem) => !stem.is_empty() && stem.chars().all(is_slug_char),
        None => false,
    }
}

fn is_slug_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-'
}

fn collapse_dashes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '-' && out.ends_with('-') {
            continue;
        }
        out.push(c);
    }
    out
}

fn trim_edges(s: &str) -> &str {
    s.trim_matches(|c: char| c == '-' || c == '.' || c.is_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_table() {
        assert_eq!(slug("Work"), "Work");
        assert_eq!(slug("Work (EU)"), "Work-EU");
        assert_eq!(slug("client/acme"), "client-acme");
        assert_eq!(slug("工作"), "profile");
        assert_eq!(slug(""), "profile");
        assert_eq!(slug("   "), "profile");
        assert_eq!(slug("---"), "profile");
        assert_eq!(slug("...."), "profile");
        assert_eq!(slug("  spaced  out  "), "spaced-out");
        assert_eq!(slug("trailing."), "trailing");
        assert_eq!(slug("../.."), "profile");
        assert_eq!(slug("a.b_c-d"), "a.b_c-d");
        assert_eq!(slug("CON"), "CON");
        assert_eq!(slug(&"x".repeat(40)), "x".repeat(MAX_SLUG_LEN));
        assert_eq!(slug(&format!("{}-tail", "y".repeat(MAX_SLUG_LEN))), "y".repeat(MAX_SLUG_LEN));
    }

    #[test]
    fn slug_output_is_always_a_safe_component() {
        for name in ["../../etc/passwd", "C:\\Windows", "-flag", "a\0b", "..", "."] {
            let folder = format!("{FOLDER_PREFIX}{}", slug(name));
            assert!(is_safe_dir(&folder), "{folder}");
            assert!(is_generated_dir(&folder), "{folder}");
        }
    }

    #[test]
    fn unsafe_dirs_are_rejected() {
        assert!(is_safe_dir("Claude-Work"));
        assert!(is_safe_dir("MyClaude"));
        assert!(!is_safe_dir(""));
        assert!(!is_safe_dir("."));
        assert!(!is_safe_dir(".."));
        assert!(!is_safe_dir("../../../../Documents"));
        assert!(!is_safe_dir("a/b"));
        assert!(!is_safe_dir("a\\b"));
        assert!(!is_safe_dir("C:"));
    }

    #[test]
    fn collisions_are_resolved_against_the_filesystem_case_insensitively() {
        let root = tempfile::tempdir().unwrap();
        // Marker-less and unregistered — exactly the hand-made folder the user already owns.
        std::fs::create_dir(root.path().join("Claude-work")).unwrap();

        assert_eq!(resolve_folder("Work", root.path()).unwrap(), "Claude-Work-2");

        std::fs::create_dir(root.path().join("Claude-Work-2")).unwrap();
        assert_eq!(resolve_folder("Work", root.path()).unwrap(), "Claude-Work-3");
    }

    #[test]
    fn missing_profiles_root_is_not_a_collision() {
        let root = tempfile::tempdir().unwrap();
        let absent = root.path().join("does-not-exist");
        assert_eq!(resolve_folder("Work", &absent).unwrap(), "Claude-Work");
    }

    #[test]
    fn empty_name_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        assert!(matches!(
            resolve_folder("  ", root.path()),
            Err(CdmError::NameEmpty)
        ));
    }

    #[test]
    fn normalization_and_case_share_one_key() {
        assert!(same_folder("Claude-Work", "claude-work"));
        assert!(same_folder("Caf\u{e9}", "Cafe\u{301}"));
        assert!(!same_folder("Claude-Work", "Claude-Work-2"));
    }
}
