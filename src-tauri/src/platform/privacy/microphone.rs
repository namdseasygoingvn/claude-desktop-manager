use super::PrivacyAccess;

#[cfg(target_os = "macos")]
pub const SETTINGS_PANE: &str = "Privacy_Microphone";

#[cfg(target_os = "macos")]
pub fn status() -> PrivacyAccess {
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

    let Some(audio) = (unsafe { AVMediaTypeAudio }) else {
        return PrivacyAccess::Unsupported;
    };
    match unsafe { AVCaptureDevice::authorizationStatusForMediaType(audio) } {
        AVAuthorizationStatus::NotDetermined => PrivacyAccess::NotDetermined,
        AVAuthorizationStatus::Authorized => PrivacyAccess::Granted,
        AVAuthorizationStatus::Denied => PrivacyAccess::Denied,
        _ => PrivacyAccess::Restricted,
    }
}

/// Shows the system prompt and blocks until it is answered.
#[cfg(target_os = "macos")]
pub fn request() -> PrivacyAccess {
    use objc2::runtime::Bool;
    use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};

    let Some(audio) = (unsafe { AVMediaTypeAudio }) else {
        return PrivacyAccess::Unsupported;
    };
    super::wait_for_answer::<Bool>(|handler| unsafe {
        AVCaptureDevice::requestAccessForMediaType_completionHandler(audio, handler)
    });
    status()
}

#[cfg(not(target_os = "macos"))]
pub fn status() -> PrivacyAccess {
    PrivacyAccess::Unsupported
}

#[cfg(not(target_os = "macos"))]
pub fn request() -> PrivacyAccess {
    PrivacyAccess::Unsupported
}
