//! Starts a package-store binary as its registered MSIX app, so the process carries package
//! identity. Claude's updater checks for that identity; a plain CreateProcess of the payload or of
//! a copy has none, and the updater then disables itself.
//!
//! UNVERIFIED: whether a second activation with a different `--user-data-dir` yields a separate
//! process rather than a hand-off to a running instance. Plan 12 records the answer.

use super::{activation_inputs, canonical, io_err};
use crate::core::types::{CdmError, Result};
use std::fs;
use std::path::{Path, PathBuf};
use windows::core::HSTRING;
use windows::Win32::Foundation::{RPC_E_CHANGED_MODE, S_FALSE, S_OK};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    ApplicationActivationManager, IApplicationActivationManager, AO_NOERRORUI,
};

pub(super) fn activate(package_full_name: &str, binary: &Path, data_dir: &Path) -> Result<u32> {
    let root = package_root(binary, package_full_name)?;
    let executable = relative_executable(binary, &root)?;

    let manifest_path = root.join(activation_inputs::MANIFEST_FILE);
    let manifest = fs::read_to_string(&manifest_path)
        .map_err(|e| io_err(&format!("read {}", manifest_path.display()), e))?;

    let family = activation_inputs::package_family_name(package_full_name).ok_or_else(|| {
        other(format!(
            "no family name in package full name {package_full_name} ({})",
            manifest_path.display()
        ))
    })?;
    let app_id = activation_inputs::application_id(&manifest, &executable).ok_or_else(|| {
        other(format!(
            "no Application Id for {executable} in {}",
            manifest_path.display()
        ))
    })?;

    let aumid = activation_inputs::aumid(&family, &app_id);
    let args = activation_inputs::arguments(&canonical(data_dir).to_string_lossy());
    activate_aumid(&aumid, &args).map_err(|e| other(format!("activate {aumid}: {e}")))
}

fn other(message: String) -> CdmError {
    CdmError::Other(message)
}

fn package_root(binary: &Path, package_full_name: &str) -> Result<PathBuf> {
    binary
        .ancestors()
        .find(|dir| {
            dir.file_name().is_some_and(|name| {
                name.to_string_lossy()
                    .eq_ignore_ascii_case(package_full_name)
            })
        })
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            other(format!(
                "{} is not inside package {package_full_name}",
                binary.display()
            ))
        })
}

fn relative_executable(binary: &Path, root: &Path) -> Result<String> {
    let relative = binary.strip_prefix(root).map_err(|_| {
        other(format!(
            "{} is not under {}",
            binary.display(),
            root.display()
        ))
    })?;
    Ok(relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("\\"))
}

struct ComScope {
    owned: bool,
}

impl ComScope {
    fn enter() -> windows::core::Result<Self> {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if hr == S_OK || hr == S_FALSE {
            Ok(Self { owned: true })
        } else if hr == RPC_E_CHANGED_MODE {
            Ok(Self { owned: false })
        } else {
            Err(hr.into())
        }
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.owned {
            unsafe { CoUninitialize() };
        }
    }
}

fn activate_aumid(aumid: &str, args: &str) -> windows::core::Result<u32> {
    let _com = ComScope::enter()?;
    unsafe {
        let manager: IApplicationActivationManager =
            CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER)?;
        manager.ActivateApplication(&HSTRING::from(aumid), &HSTRING::from(args), AO_NOERRORUI)
    }
}
