//! Resolves the installed Claude MSIX package and classifies package-store paths.

use std::os::windows::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

const PACKAGE_REPO_KEY: &str =
    r"HKCU\Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages";
const CLAUDE_PACKAGE_FILTER: &str = "Claude_*";
const CLAUDE_PACKAGE_PREFIX: &str = "Claude_";
const PACKAGE_ROOT_FOLDER_VALUE: &str = "PackageRootFolder";
const APP_SUBDIR: &str = "app";
const WINDOWS_APPS_DIR: &str = "WindowsApps";
const PROGRAMFILES_ENV: &str = "PROGRAMFILES";

/// The per-user package repository survives alias deletion and the version churn of every
/// update; the app-execution alias survives neither.
pub(super) fn payload_exe() -> Option<PathBuf> {
    let full_name = newest_claude_package()?;
    let exe = package_root(&full_name)?.join(APP_SUBDIR).join(super::win32::EXE_NAME);
    super::is_executable_file(&exe).then_some(exe)
}

fn newest_claude_package() -> Option<String> {
    newest(claude_packages(&query_claude_packages_text()?))
}

fn query_claude_packages_text() -> Option<String> {
    let output = Command::new(super::win32::REG)
        .args(["query", PACKAGE_REPO_KEY, "/k", "/f", CLAUDE_PACKAGE_FILTER])
        .stdin(Stdio::null())
        .creation_flags(super::win32::CREATE_NO_WINDOW)
        .output()
        .ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The summary line ("N match(es) found") is localized; the "HKEY" prefix on key lines is not.
fn claude_packages(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| line.trim_start().starts_with("HKEY"))
        .filter_map(|line| line.trim_end().rsplit('\\').next())
        .filter(|name| name.starts_with(CLAUDE_PACKAGE_PREFIX))
        .map(str::to_string)
        .collect()
}

fn newest(names: Vec<String>) -> Option<String> {
    names.into_iter().max_by_key(|name| super::win32::version_key(package_version(name)))
}

fn package_version(full_name: &str) -> &str {
    full_name.split('_').nth(1).unwrap_or("")
}

fn package_root(full_name: &str) -> Option<PathBuf> {
    let key = format!(r"{PACKAGE_REPO_KEY}\{full_name}");
    super::win32::reg_sz_query(&key, PACKAGE_ROOT_FOLDER_VALUE)
        .map(PathBuf::from)
        .or_else(|| {
            super::env_dir(PROGRAMFILES_ENV)
                .ok()
                .map(|root| root.join(WINDOWS_APPS_DIR).join(full_name))
        })
}

pub(super) fn is_in_package_store(path: &Path) -> bool {
    package_full_name(path).is_some()
}

/// The alias sits directly under a dir named WindowsApps and is launchable as-is; the protected
/// package payload is nested one level deeper, under the package's own folder.
pub(super) fn package_full_name(path: &Path) -> Option<String> {
    let components: Vec<Component> = path.components().collect();
    let index = components.iter().position(|c| {
        matches!(c, Component::Normal(name) if name.to_str().is_some_and(|s| s.eq_ignore_ascii_case(WINDOWS_APPS_DIR)))
    })?;
    if components.len() - index - 1 <= 1 {
        return None;
    }
    match components.get(index + 1) {
        Some(Component::Normal(name)) => name.to_str().map(str::to_string),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_QUERY_OUTPUT: &str = r"
HKEY_CURRENT_USER\Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages\Claude_1.9999.0.0_x64__pzs8sxrjxfjjc

HKEY_CURRENT_USER\Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages\Claude_1.14271.0.0_x64__pzs8sxrjxfjjc

Fin de la recherche : 2 correspondance(s) trouvée(s).
";

    #[test]
    fn a_realistic_query_output_yields_the_claude_package_full_names() {
        assert_eq!(
            claude_packages(SAMPLE_QUERY_OUTPUT),
            vec![
                "Claude_1.9999.0.0_x64__pzs8sxrjxfjjc".to_string(),
                "Claude_1.14271.0.0_x64__pzs8sxrjxfjjc".to_string(),
            ]
        );
    }

    #[test]
    fn the_newest_package_is_chosen_numerically_not_lexicographically() {
        let names = claude_packages(SAMPLE_QUERY_OUTPUT);
        assert_eq!(
            newest(names),
            Some("Claude_1.14271.0.0_x64__pzs8sxrjxfjjc".to_string())
        );
    }

    #[test]
    fn the_alias_path_has_no_package_full_name() {
        let path = Path::new(r"C:\Users\x\AppData\Local\Microsoft\WindowsApps\claude.exe");
        assert_eq!(package_full_name(path), None);
        assert!(!is_in_package_store(path));
    }

    #[test]
    fn the_package_payload_path_yields_its_full_name() {
        let path = Path::new(
            r"C:\Program Files\WindowsApps\Claude_1.14271.0.0_x64__pzs8sxrjxfjjc\app\claude.exe",
        );
        assert_eq!(
            package_full_name(path),
            Some("Claude_1.14271.0.0_x64__pzs8sxrjxfjjc".to_string())
        );
        assert!(is_in_package_store(path));
    }

    #[test]
    fn a_mixed_case_windowsapps_component_still_matches() {
        let path = Path::new(
            r"C:\Program Files\windowsapps\Claude_1.14271.0.0_x64__pzs8sxrjxfjjc\app\claude.exe",
        );
        assert_eq!(
            package_full_name(path),
            Some("Claude_1.14271.0.0_x64__pzs8sxrjxfjjc".to_string())
        );
    }

    #[test]
    fn a_path_with_no_windowsapps_component_has_no_package_full_name() {
        let path = Path::new(r"C:\Program Files\AnthropicClaude\claude.exe");
        assert_eq!(package_full_name(path), None);
    }
}
