//! Facts parsed from MSIX package full names and the paths of running Claude processes.

use std::cmp::Ordering;
use std::path::Path;

pub(super) const COPY_CACHE_DIR: &str = "msix-app";

const FULL_NAME_PIECES: usize = 5;

fn name_pieces(package_full_name: &str) -> Option<Vec<&str>> {
    let pieces: Vec<&str> = package_full_name.split('_').collect();
    (pieces.len() >= FULL_NAME_PIECES).then_some(pieces)
}

pub(super) fn version_of(package_full_name: &str) -> Option<&str> {
    name_pieces(package_full_name).map(|pieces| pieces[1])
}

pub(super) fn arch_of(package_full_name: &str) -> Option<&str> {
    name_pieces(package_full_name).map(|pieces| pieces[2])
}

pub(super) fn family_of(package_full_name: &str) -> Option<String> {
    let pieces = name_pieces(package_full_name)?;
    Some(format!("{}_{}", pieces[0], pieces[pieces.len() - 1]))
}

pub(super) fn running_package(exe: &Path) -> Option<String> {
    copy_package(exe).or_else(|| super::launch_route::package_full_name(exe))
}

fn copy_package(exe: &Path) -> Option<String> {
    let text = exe.to_string_lossy();
    let pieces: Vec<&str> = text
        .split(['\\', '/'])
        .filter(|piece| !piece.is_empty())
        .collect();
    let index = pieces
        .iter()
        .position(|piece| piece.eq_ignore_ascii_case(COPY_CACHE_DIR))?;
    match &pieces[index + 1..] {
        [name, _, ..] => Some((*name).to_string()),
        _ => None,
    }
}

pub(super) fn is_newer(candidate: &str, installed: &str) -> bool {
    let candidate = numeric_parts(candidate);
    let installed = numeric_parts(installed);
    let len = candidate.len().max(installed.len());
    for index in 0..len {
        let left = candidate.get(index).copied().unwrap_or(0);
        let right = installed.get(index).copied().unwrap_or(0);
        match left.cmp(&right) {
            Ordering::Equal => {}
            other => return other == Ordering::Greater,
        }
    }
    false
}

fn numeric_parts(version: &str) -> Vec<u64> {
    version
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_NAME: &str = "Claude_2.19675.0.0_x64__pzs8sxrjxfjjc";
    const COPY: &str = r"C:\Users\x\AppData\Local\ClaudeDesktopManager\msix-app\Claude_2.19675.0.0_x64__pzs8sxrjxfjjc\claude.exe";
    const PAYLOAD: &str =
        r"C:\Program Files\WindowsApps\Claude_2.19675.0.0_x64__pzs8sxrjxfjjc\app\claude.exe";
    const ALIAS: &str = r"C:\Users\x\AppData\Local\Microsoft\WindowsApps\claude.exe";

    #[test]
    fn version_is_the_second_piece() {
        assert_eq!(version_of(FULL_NAME), Some("2.19675.0.0"));
    }

    #[test]
    fn arch_is_the_third_piece() {
        assert_eq!(arch_of(FULL_NAME), Some("x64"));
    }

    #[test]
    fn arm64_arch_is_returned() {
        assert_eq!(
            arch_of("Claude_2.19675.0.0_arm64__pzs8sxrjxfjjc"),
            Some("arm64")
        );
    }

    #[test]
    fn neutral_arch_is_returned_as_is() {
        assert_eq!(
            arch_of("Claude_2.19675.0.0_neutral__pzs8sxrjxfjjc"),
            Some("neutral")
        );
    }

    #[test]
    fn family_is_first_piece_plus_last_piece() {
        assert_eq!(family_of(FULL_NAME), Some("Claude_pzs8sxrjxfjjc".into()));
    }

    #[test]
    fn a_malformed_full_name_gives_none() {
        for bad in ["", "Claude", "Claude_2.1.0.0", "Claude_2.1.0.0_x64_pzs"] {
            assert_eq!(version_of(bad), None, "{bad}");
            assert_eq!(arch_of(bad), None, "{bad}");
            assert_eq!(family_of(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_copy_path_names_its_package() {
        assert_eq!(
            running_package(Path::new(COPY)),
            Some(FULL_NAME.to_string())
        );
    }

    #[test]
    fn a_forward_slash_copy_path_names_its_package() {
        let path = Path::new(
            "C:/Users/x/AppData/Local/ClaudeDesktopManager/msix-app/Claude_2.19675.0.0_x64__pzs8sxrjxfjjc/claude.exe",
        );
        assert_eq!(running_package(path), Some(FULL_NAME.to_string()));
    }

    #[test]
    fn a_mixed_case_copy_cache_dir_still_matches() {
        let path = Path::new(
            r"C:\Users\x\AppData\Local\ClaudeDesktopManager\MSIX-APP\Claude_2.19675.0.0_x64__pzs8sxrjxfjjc\claude.exe",
        );
        assert_eq!(running_package(path), Some(FULL_NAME.to_string()));
    }

    #[test]
    fn a_partial_staging_dir_name_is_returned_as_is() {
        let path = Path::new(
            r"C:\Users\x\AppData\Local\ClaudeDesktopManager\msix-app\Claude_2.19675.0.0_x64__pzs8sxrjxfjjc.partial\claude.exe",
        );
        assert_eq!(running_package(path), Some(format!("{FULL_NAME}.partial")));
    }

    #[test]
    fn a_copy_cache_dir_with_nothing_below_it_gives_none() {
        let path = Path::new(r"C:\Users\x\AppData\Local\ClaudeDesktopManager\msix-app");
        assert_eq!(running_package(path), None);
        let path = Path::new(r"C:\Users\x\AppData\Local\ClaudeDesktopManager\msix-app\Claude_x");
        assert_eq!(running_package(path), None);
    }

    #[test]
    fn the_payload_path_names_its_package() {
        assert_eq!(
            running_package(Path::new(PAYLOAD)),
            Some(FULL_NAME.to_string())
        );
    }

    #[test]
    fn the_alias_path_has_no_package() {
        assert_eq!(running_package(Path::new(ALIAS)), None);
    }

    #[test]
    fn a_macos_path_has_no_package() {
        assert_eq!(
            running_package(Path::new("/Applications/Claude.app/Contents/MacOS/Claude")),
            None
        );
    }

    #[test]
    fn three_parts_equal_four_parts_is_not_newer() {
        assert!(!is_newer("2.19675.0", "2.19675.0.0"));
        assert!(!is_newer("2.19675.0.0", "2.19675.0"));
    }

    #[test]
    fn a_greater_version_is_newer() {
        assert!(is_newer("2.19676.0", "2.19675.0.0"));
        assert!(is_newer("2.19675.0.1", "2.19675.0"));
    }

    #[test]
    fn an_equal_version_is_not_newer() {
        assert!(!is_newer("2.19675.0.0", "2.19675.0.0"));
    }

    #[test]
    fn a_smaller_version_is_not_newer() {
        assert!(!is_newer("2.19674.9", "2.19675.0.0"));
    }

    #[test]
    fn parts_compare_numerically_not_lexically() {
        assert!(is_newer("2.10", "2.9"));
        assert!(!is_newer("2.9", "2.10"));
    }

    #[test]
    fn non_numeric_parts_count_as_zero() {
        assert!(!is_newer("2.x.0", "2.0.0"));
        assert!(is_newer("2.1", "2.x"));
        assert!(!is_newer("", ""));
    }
}
