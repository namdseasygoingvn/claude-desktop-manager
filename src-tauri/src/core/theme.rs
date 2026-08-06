use serde::{Deserialize, Serialize};

/// The appearance the user asked for. `System` is not a third palette: it defers to whatever
/// the OS reports, which is why the window layer takes it as "no preference" rather than a value.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    Light,
    Dark,
    #[default]
    System,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_install_follows_the_system() {
        assert_eq!(Theme::default(), Theme::System);
    }

    #[test]
    fn the_stored_values_are_lowercase() {
        assert_eq!(serde_json::to_string(&Theme::Light).unwrap(), r#""light""#);
        assert_eq!(serde_json::to_string(&Theme::Dark).unwrap(), r#""dark""#);
        assert_eq!(serde_json::to_string(&Theme::System).unwrap(), r#""system""#);
    }
}
