use jarvis_tools::ActionSettings;

#[test]
fn default_settings_use_the_system_browser_and_five_bangs() {
    let s = ActionSettings::default();
    assert_eq!(s.browser, None);
    let bangs = s.to_bangs();
    let pairs: Vec<_> = bangs
        .iter()
        .map(|b| (b.name.as_str(), b.bang.as_str()))
        .collect();
    assert_eq!(
        pairs,
        [
            ("YouTube", "yt"),
            ("Reddit", "r"),
            ("GitHub", "gh"),
            ("Amazon", "a"),
            ("Wikipedia", "w")
        ]
    );
}

#[test]
fn saved_settings_load_back_unchanged_and_create_parent_folders() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a/b/actions.json");
    let mut s = ActionSettings {
        browser: Some("brave".into()),
        ..Default::default()
    };
    s.bangs.pop();
    s.save(&path).unwrap();
    assert!(std::fs::read_to_string(&path).unwrap().contains('\n'));
    assert_eq!(ActionSettings::load(&path), s);
}

#[test]
fn a_missing_file_loads_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        ActionSettings::load(&dir.path().join("nope.json")),
        ActionSettings::default()
    );
}

#[test]
fn a_file_with_bad_json_loads_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.json");
    std::fs::write(&path, "{ not json").unwrap();
    assert_eq!(ActionSettings::load(&path), ActionSettings::default());
}

#[test]
fn a_file_naming_only_the_browser_keeps_the_default_bangs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("actions.json");
    std::fs::write(&path, r#"{"browser": "firefox"}"#).unwrap();
    let s = ActionSettings::load(&path);
    assert_eq!(s.browser.as_deref(), Some("firefox"));
    assert_eq!(s.bangs, ActionSettings::default().bangs);
}
