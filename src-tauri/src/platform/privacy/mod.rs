//! Consent prompts. A profile is spawned by cdm, so macOS asks cdm — not Claude — for them.

pub mod microphone;
pub mod speech;

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub enum PrivacyAccess {
    NotDetermined,
    Granted,
    Denied,
    Restricted,
    Unsupported,
}

impl PrivacyAccess {
    /// macOS prompts only once; after a refusal, System Settings is the one place to change it.
    pub fn is_refused(self) -> bool {
        matches!(self, Self::Denied | Self::Restricted)
    }
}

#[cfg(target_os = "macos")]
pub fn settings_url(pane: &str) -> String {
    format!("x-apple.systempreferences:com.apple.preference.security?{pane}")
}

/// Hands `ask` a completion block and blocks until the OS calls it.
#[cfg(target_os = "macos")]
fn wait_for_answer<A>(ask: impl FnOnce(&block2::DynBlock<dyn Fn(A)>))
where
    A: objc2::encode::EncodeArgument + 'static,
{
    let (answered, answer) = std::sync::mpsc::channel();
    let handler = block2::RcBlock::new(move |_: A| {
        let _ = answered.send(());
    });
    ask(&handler);
    let _ = answer.recv();
}
