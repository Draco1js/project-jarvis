//! Mapping browser identifiers (settings, ProgIds, desktop ids) to kinds.

use jarvis_core::{Browser, BrowserKind};

pub fn display_name(kind: BrowserKind) -> &'static str {
    match kind {
        BrowserKind::Chrome => "Chrome",
        BrowserKind::Brave => "Brave",
        BrowserKind::Chromium => "Chromium",
        BrowserKind::Edge => "Edge",
        BrowserKind::Firefox => "Firefox",
        BrowserKind::Opera => "Opera",
        BrowserKind::Safari => "Safari",
        BrowserKind::Other => "your browser",
    }
}

/// Find a known browser name inside `text` (case-insensitive).
fn kind_from_text(text: &str) -> BrowserKind {
    let t = text.to_lowercase();
    // Order matters: "chromium" and "chrome" overlap, Edge's id is "msedge".
    let table: [(&[&str], BrowserKind); 7] = [
        (&["brave"], BrowserKind::Brave),
        (&["msedge", "edge"], BrowserKind::Edge),
        (&["chromium"], BrowserKind::Chromium),
        (&["chrome"], BrowserKind::Chrome),
        (&["firefox"], BrowserKind::Firefox),
        (&["opera"], BrowserKind::Opera),
        (&["safari"], BrowserKind::Safari),
    ];
    for (needles, kind) in table {
        if needles.iter().any(|n| t.contains(n)) {
            return kind;
        }
    }
    BrowserKind::Other
}

/// A Windows ProgId such as `ChromeHTML`, `MSEdgeHTM`, `FirefoxURL-308046B0AF4A39CB`.
pub fn kind_from_prog_id(prog_id: &str) -> BrowserKind {
    kind_from_text(prog_id)
}

/// A Linux desktop id such as `org.mozilla.firefox.desktop` or `google-chrome.desktop`.
pub fn kind_from_desktop_id(id: &str) -> BrowserKind {
    kind_from_text(id.trim_end_matches(".desktop"))
}

/// The `browser` setting: a name ("Chrome") or an executable path. Returns the
/// kind and, for a path, the executable.
pub fn from_setting(setting: &str) -> (Browser, Option<String>) {
    let setting = setting.trim();
    let is_path = setting.contains('/') || setting.contains('\\');
    let base = setting.rsplit(['/', '\\']).next().unwrap_or(setting);
    let base = base.strip_suffix(".exe").unwrap_or(base);
    // For a path only the file name counts: a directory like `ledger-tools`
    // must not make Firefox look like Edge.
    let kind = kind_from_text(if is_path { base } else { setting });
    let name = if kind == BrowserKind::Other {
        base.to_string()
    } else {
        display_name(kind).to_string()
    };
    (Browser { kind, name }, is_path.then(|| setting.to_string()))
}

/// Pull the first executable out of a registry shell command such as
/// `"C:\Program Files\Chrome\chrome.exe" --single-argument %1`.
pub fn exe_from_command(command: &str) -> Option<String> {
    let c = command.trim();
    if let Some(rest) = c.strip_prefix('"') {
        return rest.split('"').next().map(str::to_string);
    }
    let lower = c.to_lowercase();
    match lower.find(".exe") {
        Some(i) => Some(c[..i + 4].to_string()),
        None => c.split_whitespace().next().map(str::to_string),
    }
}

/// The data of a value in `reg query` output (`    ProgId    REG_SZ    ChromeHTML`).
pub fn parse_reg_value(output: &str, value_name: &str) -> Option<String> {
    for line in output.lines() {
        let line = line.trim();
        if !line.starts_with(value_name) {
            continue;
        }
        let rest = line[value_name.len()..].trim_start();
        for ty in ["REG_SZ", "REG_EXPAND_SZ"] {
            if let Some(data) = rest.strip_prefix(ty) {
                let data = data.trim();
                if !data.is_empty() {
                    return Some(data.to_string());
                }
            }
        }
    }
    None
}

/// The data of a key's default value in `reg query ... /ve` output. The value
/// name is localized ("(Default)", "(Standard)", ...), so take the last
/// `REG_SZ`/`REG_EXPAND_SZ` line instead of matching the name.
pub fn parse_reg_default(output: &str) -> Option<String> {
    output.lines().rev().find_map(|line| {
        ["REG_EXPAND_SZ", "REG_SZ"].iter().find_map(|ty| {
            let (_, data) = line.split_once(&format!("    {ty}    "))?;
            let data = data.trim();
            (!data.is_empty()).then(|| data.to_string())
        })
    })
}

/// Usual executable names for a browser, to look up on PATH.
pub fn binary_names(kind: BrowserKind) -> &'static [&'static str] {
    match kind {
        BrowserKind::Chrome => &["google-chrome-stable", "google-chrome", "chrome"],
        BrowserKind::Brave => &["brave-browser", "brave"],
        BrowserKind::Chromium => &["chromium", "chromium-browser"],
        BrowserKind::Edge => &["microsoft-edge-stable", "microsoft-edge", "msedge"],
        BrowserKind::Firefox => &["firefox"],
        BrowserKind::Opera => &["opera"],
        BrowserKind::Safari | BrowserKind::Other => &[],
    }
}
