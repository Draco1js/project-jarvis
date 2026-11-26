//! Building the app index and parsing launchers. Everything here takes paths
//! or text so it can be tested on any OS.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use jarvis_core::App;

/// What we need from a `.desktop` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    pub name: String,
    /// `Exec=` with field codes removed.
    pub exec: String,
}

/// Parse the `[Desktop Entry]` group. `None` for entries that should not be
/// listed (not an application, NoDisplay, Hidden) or that have no name.
pub fn parse_desktop(text: &str) -> Option<DesktopEntry> {
    let mut in_entry = false;
    let (mut name, mut exec, mut kind) = (None, String::new(), None);
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Name" => name = Some(value.trim().to_string()),
            "Exec" => exec = strip_field_codes(value),
            "Type" => kind = Some(value.trim().to_string()),
            "NoDisplay" | "Hidden" if value.trim() == "true" => return None,
            _ => {}
        }
    }
    if kind.as_deref() != Some("Application") {
        return None;
    }
    let name = name.filter(|n| !n.is_empty())?;
    Some(DesktopEntry { name, exec })
}

/// Remove `%f %u %U ...` field codes (and turn `%%` into `%`).
pub fn strip_field_codes(exec: &str) -> String {
    let mut out = String::new();
    let mut chars = exec.trim().chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        if chars.next_if_eq(&'%').is_some() {
            out.push('%');
        } else {
            chars.next();
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = read.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            walk(&path, found);
        } else {
            found.push(path);
        }
    }
}

fn has_ext(path: &Path, ext: &str) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

fn dedupe(apps: Vec<App>) -> Vec<App> {
    let mut seen = HashSet::new();
    apps.into_iter()
        .filter(|a| seen.insert(a.name.to_lowercase()))
        .collect()
}

/// Start Menu shortcuts under each root, recursively. Name is the file stem;
/// uninstallers are skipped.
pub fn scan_start_menu(roots: &[PathBuf]) -> Vec<App> {
    let mut files = Vec::new();
    for root in roots {
        walk(root, &mut files);
    }
    let apps = files
        .into_iter()
        .filter(|p| has_ext(p, "lnk"))
        .filter_map(|p| {
            let name = p.file_stem()?.to_string_lossy().into_owned();
            if name.to_lowercase().starts_with("uninstall") {
                return None;
            }
            Some(App {
                name,
                target: p.to_string_lossy().into_owned(),
            })
        })
        .collect();
    dedupe(apps)
}

/// `.desktop` launchers in each `applications` directory, deduped by name.
pub fn scan_desktop_dirs(dirs: &[PathBuf]) -> Vec<App> {
    let mut files = Vec::new();
    for dir in dirs {
        walk(dir, &mut files);
    }
    let apps = files
        .into_iter()
        .filter(|p| has_ext(p, "desktop"))
        .filter_map(|p| {
            let entry = parse_desktop(&std::fs::read_to_string(&p).ok()?)?;
            Some(App {
                name: entry.name,
                target: p.to_string_lossy().into_owned(),
            })
        })
        .collect();
    dedupe(apps)
}

/// `*.app` bundles directly in each folder.
pub fn scan_mac_apps(dirs: &[PathBuf]) -> Vec<App> {
    let mut apps = Vec::new();
    for dir in dirs {
        let Ok(read) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut paths: Vec<_> = read.flatten().map(|e| e.path()).collect();
        paths.sort();
        for p in paths.into_iter().filter(|p| has_ext(p, "app")) {
            if let Some(name) = p.file_stem() {
                apps.push(App {
                    name: name.to_string_lossy().into_owned(),
                    target: p.to_string_lossy().into_owned(),
                });
            }
        }
    }
    dedupe(apps)
}

/// The XDG `applications` directories, in priority order.
pub fn xdg_application_dirs(
    data_home: Option<&str>,
    home: Option<&Path>,
    data_dirs: Option<&str>,
) -> Vec<PathBuf> {
    let mut bases: Vec<PathBuf> = Vec::new();
    match data_home.filter(|s| !s.is_empty()) {
        Some(d) => bases.push(d.into()),
        None => bases.extend(home.map(|h| h.join(".local/share"))),
    }
    let dirs = data_dirs
        .filter(|s| !s.is_empty())
        .unwrap_or("/usr/local/share:/usr/share");
    bases.extend(dirs.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    bases.into_iter().map(|b| b.join("applications")).collect()
}
