use jarvis_core::BrowserKind::*;
use jarvis_tools::browser::*;

#[test]
fn windows_prog_ids_map_to_their_browsers() {
    for (id, kind) in [
        ("ChromeHTML", Chrome),
        ("BraveHTML", Brave),
        ("MSEdgeHTM", Edge),
        ("FirefoxURL-308046B0AF4A39CB", Firefox),
        ("OperaStable", Opera),
        ("ChromiumHTM", Chromium),
        ("SomethingElse", Other),
    ] {
        assert_eq!(kind_from_prog_id(id), kind, "{id}");
    }
}

#[test]
fn linux_desktop_ids_map_to_their_browsers() {
    for (id, kind) in [
        ("google-chrome.desktop", Chrome),
        ("brave-browser.desktop", Brave),
        ("org.mozilla.firefox.desktop", Firefox),
        ("microsoft-edge.desktop", Edge),
        ("chromium.desktop", Chromium),
        ("epiphany.desktop", Other),
        ("", Other),
    ] {
        assert_eq!(kind_from_desktop_id(id), kind, "{id}");
    }
}

#[test]
fn a_browser_setting_is_matched_by_name_ignoring_case() {
    let (b, exe) = from_setting("  Chrome ");
    assert_eq!((b.kind, b.name.as_str(), exe), (Chrome, "Chrome", None));
}

#[test]
fn a_browser_setting_that_is_a_path_keeps_the_path_and_guesses_the_kind() {
    let (b, exe) =
        from_setting(r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe");
    assert_eq!(b.kind, Brave);
    assert!(exe.unwrap().ends_with("brave.exe"));
}

#[test]
fn an_unknown_browser_path_is_other_and_named_after_the_file() {
    let (b, exe) = from_setting("/opt/weird/bin/zorp");
    assert_eq!((b.kind, b.name.as_str()), (Other, "zorp"));
    assert_eq!(exe.as_deref(), Some("/opt/weird/bin/zorp"));
}

#[test]
fn the_executable_is_taken_from_a_quoted_registry_command() {
    assert_eq!(
        exe_from_command(
            r#""C:\Program Files\Google\Chrome\Application\chrome.exe" --single-argument %1"#
        )
        .as_deref(),
        Some(r"C:\Program Files\Google\Chrome\Application\chrome.exe")
    );
}

#[test]
fn the_executable_is_taken_from_an_unquoted_registry_command() {
    assert_eq!(
        exe_from_command(r"C:\Opera\opera.exe -url %1").as_deref(),
        Some(r"C:\Opera\opera.exe")
    );
}

#[test]
fn reg_query_output_yields_the_named_value() {
    let out = "\r\nHKEY_CURRENT_USER\\Software\\X\\UserChoice\r\n    Hash    REG_SZ    abc=\r\n    ProgId    REG_SZ    ChromeHTML\r\n\r\n";
    assert_eq!(
        parse_reg_value(out, "ProgId").as_deref(),
        Some("ChromeHTML")
    );
    assert_eq!(parse_reg_value(out, "Missing"), None);
}

#[test]
fn reg_query_output_yields_the_default_value() {
    let out = "HKEY_CLASSES_ROOT\\ChromeHTML\\shell\\open\\command\r\n    (Default)    REG_SZ    \"C:\\a b\\chrome.exe\" %1\r\n";
    assert_eq!(
        parse_reg_value(out, "(Default)").as_deref(),
        Some("\"C:\\a b\\chrome.exe\" %1")
    );
}
