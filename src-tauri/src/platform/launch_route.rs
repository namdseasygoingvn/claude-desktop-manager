//! Classifies how a Claude binary path is launched.

use std::path::Path;

pub(super) const WINDOWS_APPS_DIR: &str = "WindowsApps";
const MICROSOFT_DIR: &str = "Microsoft";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchRoute {
    Spawn,
    ExecutionAlias,
    PackageCopy { package_full_name: String },
}

impl LaunchRoute {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Spawn => "spawn",
            Self::ExecutionAlias => "executionAlias",
            Self::PackageCopy { .. } => "packageCopy",
        }
    }
}

pub(super) fn classify(binary: &Path) -> LaunchRoute {
    if let Some(package_full_name) = package_full_name(binary) {
        return LaunchRoute::PackageCopy { package_full_name };
    }
    let pieces = pieces(binary);
    match pieces.as_slice() {
        [.., grandparent, parent, _]
            if parent.eq_ignore_ascii_case(WINDOWS_APPS_DIR)
                && grandparent.eq_ignore_ascii_case(MICROSOFT_DIR) =>
        {
            LaunchRoute::ExecutionAlias
        }
        _ => LaunchRoute::Spawn,
    }
}

pub(super) fn package_full_name(path: &Path) -> Option<String> {
    let pieces = pieces(path);
    let index = pieces
        .iter()
        .position(|piece| piece.eq_ignore_ascii_case(WINDOWS_APPS_DIR))?;
    match &pieces[index + 1..] {
        [name, _, ..] => Some(name.clone()),
        _ => None,
    }
}

fn pieces(path: &Path) -> Vec<String> {
    path.to_string_lossy()
        .split(['\\', '/'])
        .filter(|piece| !piece.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_NAME: &str = "Claude_1.14271.0.0_x64__pzs8sxrjxfjjc";
    const PAYLOAD: &str =
        r"C:\Program Files\WindowsApps\Claude_1.14271.0.0_x64__pzs8sxrjxfjjc\app\claude.exe";
    const ALIAS: &str = r"C:\Users\x\AppData\Local\Microsoft\WindowsApps\claude.exe";

    #[test]
    fn the_alias_path_has_no_package_full_name() {
        assert_eq!(package_full_name(Path::new(ALIAS)), None);
    }

    #[test]
    fn the_package_payload_path_yields_its_full_name() {
        assert_eq!(
            package_full_name(Path::new(PAYLOAD)),
            Some(FULL_NAME.to_string())
        );
    }

    #[test]
    fn a_mixed_case_windowsapps_component_still_matches() {
        let path = Path::new(
            r"C:\Program Files\windowsapps\Claude_1.14271.0.0_x64__pzs8sxrjxfjjc\app\claude.exe",
        );
        assert_eq!(package_full_name(path), Some(FULL_NAME.to_string()));
    }

    #[test]
    fn a_path_with_no_windowsapps_component_has_no_package_full_name() {
        let path = Path::new(r"C:\Program Files\AnthropicClaude\claude.exe");
        assert_eq!(package_full_name(path), None);
    }

    #[test]
    fn a_forward_slash_payload_path_still_yields_the_full_name() {
        let path = Path::new(
            "C:/Program Files/WindowsApps/Claude_1.14271.0.0_x64__pzs8sxrjxfjjc/app/claude.exe",
        );
        assert_eq!(package_full_name(path), Some(FULL_NAME.to_string()));
    }

    #[test]
    fn an_exe_directly_in_the_package_root_still_counts_as_package_store() {
        let path = Path::new(
            r"C:\Program Files\WindowsApps\Claude_1.14271.0.0_x64__pzs8sxrjxfjjc\claude.exe",
        );
        assert_eq!(package_full_name(path), Some(FULL_NAME.to_string()));
    }

    #[test]
    fn the_payload_classifies_as_package_copy() {
        assert_eq!(
            classify(Path::new(PAYLOAD)),
            LaunchRoute::PackageCopy {
                package_full_name: FULL_NAME.to_string()
            }
        );
    }

    #[test]
    fn the_alias_classifies_as_execution_alias() {
        assert_eq!(classify(Path::new(ALIAS)), LaunchRoute::ExecutionAlias);
    }

    #[test]
    fn a_mixed_case_alias_classifies_as_execution_alias() {
        let path = Path::new(r"c:\users\x\appdata\local\microsoft\windowsapps\claude.exe");
        assert_eq!(classify(path), LaunchRoute::ExecutionAlias);
    }

    #[test]
    fn a_windowsapps_dir_outside_microsoft_is_not_an_alias() {
        let path = Path::new(r"D:\WindowsApps\claude.exe");
        assert_eq!(classify(path), LaunchRoute::Spawn);
    }

    #[test]
    fn a_plain_windows_binary_classifies_as_spawn() {
        let path = Path::new(r"C:\Users\x\AppData\Local\AnthropicClaude\app-1.0.0\claude.exe");
        assert_eq!(classify(path), LaunchRoute::Spawn);
    }

    #[test]
    fn a_macos_binary_classifies_as_spawn() {
        let path = Path::new("/Applications/Claude.app/Contents/MacOS/Claude");
        assert_eq!(classify(path), LaunchRoute::Spawn);
    }

    #[test]
    fn a_user_folder_named_windowsapps_classifies_as_package_copy() {
        let path = Path::new(r"D:\WindowsApps\foo\claude.exe");
        assert_eq!(
            classify(path),
            LaunchRoute::PackageCopy {
                package_full_name: "foo".to_string()
            }
        );
    }

    #[test]
    fn a_verbatim_prefix_is_harmless() {
        let path = Path::new(
            r"\\?\C:\Program Files\WindowsApps\Claude_1.14271.0.0_x64__pzs8sxrjxfjjc\app\claude.exe",
        );
        assert_eq!(package_full_name(path), Some(FULL_NAME.to_string()));
    }

    #[test]
    fn labels_are_the_three_exact_strings() {
        assert_eq!(LaunchRoute::Spawn.label(), "spawn");
        assert_eq!(LaunchRoute::ExecutionAlias.label(), "executionAlias");
        assert_eq!(
            LaunchRoute::PackageCopy {
                package_full_name: FULL_NAME.to_string()
            }
            .label(),
            "packageCopy"
        );
    }
}
