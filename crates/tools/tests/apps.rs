use std::fs;
use std::path::PathBuf;

use jarvis_core::Volume;
use jarvis_tools::apps::*;
use jarvis_tools::files::newest_file;
use jarvis_tools::volume::target_level;

const FIREFOX: &str = "[Desktop Entry]\nType=Application\nName=Firefox\nExec=firefox %u\n[Desktop Action new-window]\nName=New Window\nExec=firefox --new-window\n";

#[test]
fn a_desktop_file_gives_its_name_and_exec_without_field_codes() {
    let e = parse_desktop(FIREFOX).unwrap();
    assert_eq!((e.name.as_str(), e.exec.as_str()), ("Firefox", "firefox"));
}

#[test]
fn names_inside_other_groups_are_ignored() {
    let e = parse_desktop(
        "[Desktop Action x]\nName=Wrong\n[Desktop Entry]\nType=Application\nName=Right\nExec=r\n",
    )
    .unwrap();
    assert_eq!(e.name, "Right");
}

#[test]
fn nodisplay_hidden_and_non_application_entries_are_skipped() {
    for extra in ["NoDisplay=true", "Hidden=true", "Type=Link"] {
        let text = format!("[Desktop Entry]\nType=Application\nName=X\nExec=x\n{extra}\n");
        assert_eq!(parse_desktop(&text), None, "{extra}");
    }
    assert!(
        parse_desktop("[Desktop Entry]\nType=Application\nName=X\nExec=x\nNoDisplay=false\n")
            .is_some()
    );
    assert_eq!(
        parse_desktop("[Desktop Entry]\nName=NoType\nExec=x\n"),
        None
    );
}

#[test]
fn exec_field_codes_are_removed_and_percent_percent_is_kept() {
    assert_eq!(
        strip_field_codes("vlc --started-from-file %U"),
        "vlc --started-from-file"
    );
    assert_eq!(strip_field_codes("app %f %F %i --x %c"), "app --x");
    assert_eq!(strip_field_codes("echo 100%%"), "echo 100%");
}

#[test]
fn desktop_dirs_are_scanned_deduped_by_name_with_earlier_dirs_winning() {
    let t = tempfile::tempdir().unwrap();
    let (a, b) = (t.path().join("a"), t.path().join("b"));
    fs::create_dir_all(&a).unwrap();
    fs::create_dir_all(&b).unwrap();
    fs::write(a.join("ff.desktop"), FIREFOX).unwrap();
    fs::write(b.join("ff2.desktop"), FIREFOX).unwrap();
    fs::write(
        b.join("hidden.desktop"),
        "[Desktop Entry]\nType=Application\nName=H\nExec=h\nNoDisplay=true\n",
    )
    .unwrap();
    fs::write(b.join("readme.txt"), "x").unwrap();
    let apps = scan_desktop_dirs(&[a.clone(), b]);
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].name, "Firefox");
    assert_eq!(PathBuf::from(&apps[0].target), a.join("ff.desktop"));
}

#[test]
fn xdg_dirs_use_data_home_then_data_dirs_with_defaults() {
    let d = xdg_application_dirs(None, Some(&PathBuf::from("/home/u")), None);
    let want: Vec<PathBuf> = [
        "/home/u/.local/share/applications",
        "/usr/local/share/applications",
        "/usr/share/applications",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();
    assert_eq!(d, want);
    let d = xdg_application_dirs(Some("/h"), None, Some("/x:/y"));
    let want: Vec<PathBuf> = ["/h/applications", "/x/applications", "/y/applications"]
        .into_iter()
        .map(PathBuf::from)
        .collect();
    assert_eq!(d, want);
}

#[test]
fn start_menu_shortcuts_are_found_recursively_named_by_stem_without_uninstallers() {
    let t = tempfile::tempdir().unwrap();
    let (r1, r2) = (t.path().join("common"), t.path().join("user"));
    fs::create_dir_all(r1.join("Games/Sub")).unwrap();
    fs::create_dir_all(&r2).unwrap();
    fs::write(r1.join("Notepad.lnk"), "").unwrap();
    fs::write(r1.join("Games/Sub/Steam.lnk"), "").unwrap();
    fs::write(r1.join("Games/Uninstall Steam.lnk"), "").unwrap();
    fs::write(r1.join("Games/readme.txt"), "").unwrap();
    fs::write(r2.join("Spotify.LNK"), "").unwrap();
    let mut names: Vec<_> = scan_start_menu(&[r1.clone(), r2, t.path().join("missing")])
        .into_iter()
        .map(|a| a.name)
        .collect();
    names.sort();
    assert_eq!(names, ["Notepad", "Spotify", "Steam"]);
    let steam = scan_start_menu(std::slice::from_ref(&r1))
        .into_iter()
        .find(|a| a.name == "Steam")
        .unwrap();
    assert_eq!(PathBuf::from(steam.target), r1.join("Games/Sub/Steam.lnk"));
}

#[test]
fn mac_apps_are_the_app_bundles_in_each_folder() {
    let t = tempfile::tempdir().unwrap();
    fs::create_dir_all(t.path().join("Safari.app")).unwrap();
    fs::create_dir_all(t.path().join("Utilities")).unwrap();
    fs::write(t.path().join("notes.txt"), "").unwrap();
    let apps = scan_mac_apps(&[t.path().to_path_buf()]);
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].name, "Safari");
}

#[test]
fn the_newest_file_ignores_hidden_files_and_subfolders() {
    let t = tempfile::tempdir().unwrap();
    let set = |name: &str, secs: u64| {
        let f = fs::File::create(t.path().join(name)).unwrap();
        f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
            .unwrap();
    };
    set("old.pdf", 1_000);
    set("report.pdf", 2_000);
    set(".hidden", 9_000);
    fs::create_dir(t.path().join("newdir")).unwrap();
    assert_eq!(
        newest_file(t.path()).unwrap().file_name().unwrap(),
        "report.pdf"
    );
}

#[test]
fn a_folder_without_regular_files_has_no_newest_file() {
    let t = tempfile::tempdir().unwrap();
    fs::create_dir(t.path().join("sub")).unwrap();
    assert_eq!(newest_file(t.path()), None);
    assert_eq!(newest_file(&t.path().join("missing")), None);
}

#[test]
fn volume_changes_are_clamped_between_zero_and_a_hundred() {
    assert_eq!(target_level(50, Volume::Up(10)), Some(60));
    assert_eq!(target_level(95, Volume::Up(10)), Some(100));
    assert_eq!(target_level(5, Volume::Down(10)), Some(0));
    assert_eq!(target_level(5, Volume::Set(200)), Some(100));
    assert_eq!(target_level(5, Volume::Mute), None);
}
