//! Speech recognition, which Claude Desktop's Quick Entry dictation needs.

use super::PrivacyAccess;

#[cfg(target_os = "macos")]
pub const SETTINGS_PANE: &str = "Privacy_SpeechRecognition";

#[cfg(target_os = "macos")]
pub fn status() -> PrivacyAccess {
    use objc2_speech::{SFSpeechRecognizer, SFSpeechRecognizerAuthorizationStatus as Status};

    match unsafe { SFSpeechRecognizer::authorizationStatus() } {
        Status::NotDetermined => PrivacyAccess::NotDetermined,
        Status::Authorized => PrivacyAccess::Granted,
        Status::Denied => PrivacyAccess::Denied,
        _ => PrivacyAccess::Restricted,
    }
}

/// Shows the system prompt and blocks until it is answered.
#[cfg(target_os = "macos")]
pub fn request() -> PrivacyAccess {
    use objc2_speech::{SFSpeechRecognizer, SFSpeechRecognizerAuthorizationStatus};

    super::wait_for_answer::<SFSpeechRecognizerAuthorizationStatus>(|handler| unsafe {
        SFSpeechRecognizer::requestAuthorization(handler)
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
