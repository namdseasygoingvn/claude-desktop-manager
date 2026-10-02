//! Pure inputs to package activation: family name, manifest app id, AUMID, argument string.

use super::USER_DATA_DIR_ARG;

pub(super) const MANIFEST_FILE: &str = "AppxManifest.xml";

const APPLICATION_TAG: &str = "<Application";

pub(super) fn package_family_name(package_full_name: &str) -> Option<String> {
    let pieces: Vec<&str> = package_full_name.split('_').collect();
    if pieces.len() < 5 {
        return None;
    }
    let (name, publisher) = (pieces[0], pieces[pieces.len() - 1]);
    if name.is_empty() || publisher.is_empty() {
        return None;
    }
    Some(format!("{name}_{publisher}"))
}

pub(super) fn application_id(manifest_xml: &str, executable: &str) -> Option<String> {
    let wanted = normalize_executable(executable);
    let applications: Vec<(Option<String>, Option<String>)> = application_tags(manifest_xml)
        .map(|tag| (attribute(tag, "Id"), attribute(tag, "Executable")))
        .collect();
    let matched = applications.iter().find(|(_, exe)| {
        exe.as_deref()
            .is_some_and(|e| normalize_executable(e) == wanted)
    });
    match (matched, applications.as_slice()) {
        (Some((id, _)), _) => id.clone(),
        (None, [(id, _)]) => id.clone(),
        _ => None,
    }
}

pub(super) fn aumid(family_name: &str, application_id: &str) -> String {
    format!("{family_name}!{application_id}")
}

pub(super) fn arguments(user_data_dir: &str) -> String {
    let trailing = user_data_dir.len() - user_data_dir.trim_end_matches('\\').len();
    format!(
        "{USER_DATA_DIR_ARG}\"{user_data_dir}{}\"",
        "\\".repeat(trailing)
    )
}

fn normalize_executable(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase()
}

fn application_tags(xml: &str) -> impl Iterator<Item = &str> {
    xml.match_indices(APPLICATION_TAG)
        .filter_map(move |(start, _)| {
            let body = &xml[start + APPLICATION_TAG.len()..];
            body.starts_with(|c: char| c.is_whitespace())
                .then(|| &body[..tag_end(body)])
        })
}

fn tag_end(tag_body: &str) -> usize {
    let mut quote = None;
    for (i, c) in tag_body.char_indices() {
        match (quote, c) {
            (None, '>') => return i,
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            _ => {}
        }
    }
    tag_body.len()
}

fn attribute(tag_body: &str, name: &str) -> Option<String> {
    let mut rest = tag_body;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '/');
        let name_end = rest.find(|c: char| c.is_whitespace() || c == '=' || c == '/')?;
        let (key, after_key) = rest.split_at(name_end);
        let after_eq = after_key.trim_start().strip_prefix('=')?.trim_start();
        let quote = after_eq
            .chars()
            .next()
            .filter(|c| matches!(c, '"' | '\''))?;
        let value_and_rest = &after_eq[1..];
        let value_end = value_and_rest.find(quote)?;
        if key == name {
            return Some(value_and_rest[..value_end].to_string());
        }
        rest = &value_and_rest[value_end + 1..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL_MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10">
  <Applications>
    <Application Id="Claude" Executable="app\claude.exe" EntryPoint="Windows.FullTrustApplication">
      <uap:VisualElements DisplayName="Claude" />
    </Application>
  </Applications>
</Package>"#;

    fn two_applications(first_exe: &str, second_exe: &str) -> String {
        format!(
            r#"<Applications>
<Application Id="First" Executable="{first_exe}" />
<Application Id="Second" Executable="{second_exe}" />
</Applications>"#
        )
    }

    #[test]
    fn family_name_from_full_name() {
        assert_eq!(
            package_family_name("Claude_1.14271.0.0_x64__pzs8sxrjxfjjc").as_deref(),
            Some("Claude_pzs8sxrjxfjjc")
        );
    }

    #[test]
    fn family_name_rejects_malformed() {
        assert_eq!(package_family_name("Claude_pzs8sxrjxfjjc"), None);
        assert_eq!(package_family_name("Claude_1.0_x64_"), None);
        assert_eq!(package_family_name("_1.0_x64__pzs8sxrjxfjjc"), None);
        assert_eq!(package_family_name("Claude_1.0_x64__"), None);
        assert_eq!(package_family_name(""), None);
    }

    #[test]
    fn real_manifest_yields_id() {
        assert_eq!(
            application_id(REAL_MANIFEST, r"app\claude.exe").as_deref(),
            Some("Claude")
        );
    }

    #[test]
    fn matches_by_executable_among_several() {
        let xml = two_applications(r"app\other.exe", r"app\claude.exe");
        assert_eq!(
            application_id(&xml, r"app\claude.exe").as_deref(),
            Some("Second")
        );
    }

    #[test]
    fn no_match_among_several_is_none() {
        let xml = two_applications(r"app\a.exe", r"app\b.exe");
        assert_eq!(application_id(&xml, r"app\claude.exe"), None);
    }

    #[test]
    fn single_application_without_match_falls_back() {
        let xml = r#"<Applications><Application Id="Only" Executable="VFS\ProgramFilesX64\Claude\claude.exe"/></Applications>"#;
        assert_eq!(
            application_id(xml, r"app\claude.exe").as_deref(),
            Some("Only")
        );
    }

    #[test]
    fn single_quotes_are_accepted() {
        let xml = r"<Applications><Application Id='Claude' Executable='app\claude.exe'></Application></Applications>";
        assert_eq!(
            application_id(xml, r"app\claude.exe").as_deref(),
            Some("Claude")
        );
    }

    #[test]
    fn executable_ignores_case_and_slash_direction() {
        let xml = two_applications(r"APP\Claude.EXE", r"app\other.exe");
        assert_eq!(
            application_id(&xml, "app/claude.exe").as_deref(),
            Some("First")
        );
    }

    #[test]
    fn prefixed_id_attribute_is_not_the_id() {
        let xml = r#"<Applications><Application uap10:Id="Decoy" Id="Claude" Executable="app\claude.exe"/></Applications>"#;
        assert_eq!(
            application_id(xml, r"app\claude.exe").as_deref(),
            Some("Claude")
        );
    }

    #[test]
    fn prefixed_id_alone_is_none() {
        let xml = r#"<Application uap10:Id="Decoy" Executable="app\claude.exe"/>"#;
        assert_eq!(application_id(xml, r"app\claude.exe"), None);
    }

    #[test]
    fn container_tags_are_not_applications() {
        let xml = r#"<Applications><ApplicationContentUriRules Id="x" Executable="app\claude.exe"/></Applications>"#;
        assert_eq!(application_id(xml, r"app\claude.exe"), None);
    }

    #[test]
    fn id_text_inside_another_value_is_ignored() {
        let xml =
            r#"<Application Executable="app\claude.exe" Description=' Id="Fake" ' Id="Real"/>"#;
        assert_eq!(
            application_id(xml, r"app\claude.exe").as_deref(),
            Some("Real")
        );
    }

    #[test]
    fn gt_inside_quoted_value_does_not_end_the_tag() {
        let xml = r#"<Application Description="a > b" Id="Claude" Executable="app\claude.exe">"#;
        assert_eq!(
            application_id(xml, r"app\claude.exe").as_deref(),
            Some("Claude")
        );
    }

    #[test]
    fn manifest_with_bom_and_no_applications_is_none() {
        assert_eq!(
            application_id("\u{feff}<?xml version=\"1.0\"?><Package/>", "x.exe"),
            None
        );
    }

    #[test]
    fn aumid_joins_with_bang() {
        assert_eq!(
            aumid("Claude_pzs8sxrjxfjjc", "Claude"),
            "Claude_pzs8sxrjxfjjc!Claude"
        );
    }

    #[test]
    fn arguments_quote_a_path_with_spaces() {
        assert_eq!(
            arguments(r"C:\Users\a b\AppData\Roaming\Claude-x"),
            r#"--user-data-dir="C:\Users\a b\AppData\Roaming\Claude-x""#
        );
    }

    #[test]
    fn arguments_double_trailing_backslashes() {
        assert_eq!(arguments(r"C:\dir\"), r#"--user-data-dir="C:\dir\\""#);
        assert_eq!(arguments(r"C:\dir\\"), r#"--user-data-dir="C:\dir\\\\""#);
    }
}
