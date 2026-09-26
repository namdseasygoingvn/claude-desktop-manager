//! Microphone consent. A profile is spawned by cdm, so macOS asks cdm — not Claude — for it.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub enum MicrophoneAccess {
    NotDetermined,
    Granted,
    Denied,
    Restricted,
    Unsupported,
}

#[cfg(target_os = "macos")]
pub const PRIVACY_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";

#[cfg(target_os = "macos")]
pub fn status() -> MicrophoneAccess {
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

    let Some(audio) = (unsafe { AVMediaTypeAudio }) else {
        return MicrophoneAccess::Unsupported;
    };
    match unsafe { AVCaptureDevice::authorizationStatusForMediaType(audio) } {
        AVAuthorizationStatus::NotDetermined => MicrophoneAccess::NotDetermined,
        AVAuthorizationStatus::Authorized => MicrophoneAccess::Granted,
        AVAuthorizationStatus::Denied => MicrophoneAccess::Denied,
        _ => MicrophoneAccess::Restricted,
    }
}

/// Shows the system prompt and blocks until it is answered. The OS only prompts while the
/// status is `NotDetermined`; after that it answers at once with the stored choice.
#[cfg(target_os = "macos")]
pub fn request() -> MicrophoneAccess {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};
    use std::sync::mpsc;

    let Some(audio) = (unsafe { AVMediaTypeAudio }) else {
        return MicrophoneAccess::Unsupported;
    };
    let (answered, answer) = mpsc::channel();
    let handler = RcBlock::new(move |_granted: Bool| {
        let _ = answered.send(());
    });
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(audio, &handler) };
    let _ = answer.recv();
    status()
}

#[cfg(not(target_os = "macos"))]
pub fn status() -> MicrophoneAccess {
    MicrophoneAccess::Unsupported
}

#[cfg(not(target_os = "macos"))]
pub fn request() -> MicrophoneAccess {
    MicrophoneAccess::Unsupported
}
