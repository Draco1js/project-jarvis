//! Seam 1: computer actions, driven through the assistant with a fake chat
//! model and a fake action executor.

mod fakes;

use fakes::Harness;
use jarvis_core::{Action, BrowserKind, Folder, MediaKey, Output, Role, Volume};
use serde_json::json;

fn opened_urls(h: &Harness) -> Vec<(String, bool)> {
    h.actions
        .done()
        .into_iter()
        .filter_map(|a| match a {
            Action::OpenUrl { url, private } => Some((url, private)),
            _ => None,
        })
        .collect()
}

#[test]
fn search_with_a_bang_opens_unduck_with_that_bang() {
    let mut h = Harness::new();
    h.chat
        .will_call("web_search", json!({"query": "lofi beats", "bang": "yt"}))
        .will_reply("Here are lofi beats on YouTube.");

    let outputs = h.type_text("Play lofi beats on YouTube");

    assert_eq!(
        opened_urls(&h),
        vec![("https://unduck.link?q=!yt%20lofi%20beats".to_string(), true)]
    );
    assert_eq!(
        outputs.last(),
        Some(&Output::Reply("Here are lofi beats on YouTube.".into()))
    );
    assert_eq!(h.speaker.spoken(), vec!["Here are lofi beats on YouTube."]);
}

#[test]
fn plain_search_has_no_bang() {
    let mut h = Harness::new();
    h.chat
        .will_call("web_search", json!({"query": "rust & gpui"}))
        .will_reply("Searching.");

    h.type_text("Search for rust and gpui");

    assert_eq!(
        opened_urls(&h),
        vec![("https://unduck.link?q=rust%20%26%20gpui".to_string(), true)]
    );
}

#[test]
fn the_editable_bang_list_is_given_to_the_model() {
    let mut h = Harness::new();
    h.actions
        .set_bangs(&[("YouTube", "yt"), ("Crates", "crates")]);
    h.chat.will_reply("Hello.");

    h.type_text("Hi");

    let prompt = h.chat.last_request()[0].content.clone();
    assert!(prompt.contains("YouTube !yt"), "{prompt}");
    assert!(prompt.contains("Crates !crates"), "{prompt}");
    assert!(!prompt.contains("Reddit"), "{prompt}");
}

#[test]
fn a_site_name_given_as_the_bang_uses_its_bang_from_the_list() {
    let mut h = Harness::new();
    h.chat
        .will_call("web_search", json!({"query": "tokio", "bang": "GitHub"}))
        .will_reply("Searching GitHub.");

    h.type_text("Find tokio on GitHub");

    assert_eq!(
        opened_urls(&h),
        vec![("https://unduck.link?q=!gh%20tokio".to_string(), true)]
    );
}

#[test]
fn a_browser_without_private_mode_opens_normally_and_says_so_once() {
    let mut h = Harness::new();
    h.actions.use_browser(BrowserKind::Safari, "Safari");
    h.chat
        .will_call("web_search", json!({"query": "cats"}))
        .will_reply("Here you go.")
        .will_call("web_search", json!({"query": "dogs"}))
        .will_reply("Here you go.");

    let first = h.type_text("Search cats");
    let second = h.type_text("Search dogs");

    assert_eq!(
        opened_urls(&h),
        vec![
            ("https://unduck.link?q=cats".to_string(), false),
            ("https://unduck.link?q=dogs".to_string(), false),
        ]
    );
    let Some(Output::Reply(first)) = first.last() else {
        panic!("no reply: {first:?}");
    };
    assert!(first.starts_with("Here you go. "), "{first}");
    assert!(
        first.contains("Safari can't open private windows"),
        "{first}"
    );
    assert_eq!(second.last(), Some(&Output::Reply("Here you go.".into())));
}

#[test]
fn urls_open_in_a_private_window_with_https_added_when_missing() {
    let mut h = Harness::new();
    h.chat
        .will_call("open_url", json!({"url": "example.com/docs"}))
        .will_call("open_url", json!({"url": "http://localhost:8080"}))
        .will_reply("Opened both.");

    h.type_text("Open example.com/docs and my local server");

    assert_eq!(
        opened_urls(&h),
        vec![
            ("https://example.com/docs".to_string(), true),
            ("http://localhost:8080".to_string(), true),
        ]
    );
}

#[test]
fn only_web_urls_can_be_opened() {
    let mut h = Harness::new();
    h.chat
        .will_call("open_url", json!({"url": "file:///etc/passwd"}))
        .will_reply("I can't open that.");

    h.type_text("Open file:///etc/passwd");

    assert!(h.actions.done().is_empty());
    assert!(h.chat.last_tool_results()[0].starts_with("Error"));
}

fn opened_apps(h: &Harness) -> Vec<String> {
    h.actions
        .done()
        .into_iter()
        .filter_map(|a| match a {
            Action::OpenApp(app) => Some(app.name),
            _ => None,
        })
        .collect()
}

const APPS: &[&str] = &[
    "Google Chrome",
    "Visual Studio Code",
    "Visual Studio 2022",
    "Spotify",
    "Microsoft Word",
    "Notepad",
    "Notepad++",
];

#[test]
fn an_app_is_found_by_part_of_its_name() {
    let mut h = Harness::new();
    h.actions.install(APPS);
    h.chat
        .will_call("open_app", json!({"name": "chrome"}))
        .will_reply("Chrome is open.");

    h.type_text("Open Chrome");

    assert_eq!(opened_apps(&h), vec!["Google Chrome"]);
}

#[test]
fn small_misspellings_still_find_the_app() {
    let mut h = Harness::new();
    h.actions.install(APPS);
    h.chat
        .will_call("open_app", json!({"name": "spotfy"}))
        .will_reply("Spotify is open.");

    h.type_text("Open spotfy");

    assert_eq!(opened_apps(&h), vec!["Spotify"]);
}

#[test]
fn an_exact_name_wins_over_longer_ones() {
    let mut h = Harness::new();
    h.actions.install(APPS);
    h.chat
        .will_call("open_app", json!({"name": "notepad"}))
        .will_reply("Notepad is open.");

    h.type_text("Open Notepad");

    assert_eq!(opened_apps(&h), vec!["Notepad"]);
}

#[test]
fn an_ambiguous_app_name_asks_which_one_and_opens_nothing() {
    let mut h = Harness::new();
    h.actions.install(APPS);
    h.chat
        .will_call("open_app", json!({"name": "visual studio"}));

    let outputs = h.type_text("Open Visual Studio");

    assert!(opened_apps(&h).is_empty());
    assert_eq!(
        h.chat.requests().len(),
        1,
        "the question needs no model call"
    );
    let Some(Output::Reply(question)) = outputs.last() else {
        panic!("no question: {outputs:?}");
    };
    assert!(question.starts_with("Which one"), "{question}");
    assert!(question.contains("Visual Studio Code"), "{question}");
    assert!(question.contains("Visual Studio 2022"), "{question}");
    assert_eq!(h.speaker.spoken(), vec![question.clone()]);

    // The answer goes to the model with the question in context.
    h.chat
        .will_call("open_app", json!({"name": "Visual Studio Code"}))
        .will_reply("Opening it.");
    h.type_text("The code one");

    let request = h.chat.requests()[1].clone();
    assert!(request.iter().any(|m| m.content == *question));
    assert_eq!(opened_apps(&h), vec!["Visual Studio Code"]);
}

#[test]
fn an_unknown_app_is_reported_to_the_model() {
    let mut h = Harness::new();
    h.actions.install(APPS);
    h.chat
        .will_call("open_app", json!({"name": "photoshop"}))
        .will_reply("I couldn't find Photoshop.");

    h.type_text("Open Photoshop");

    assert!(opened_apps(&h).is_empty());
    assert!(
        h.chat.last_tool_results()[0].contains("No app"),
        "{:?}",
        h.chat.last_tool_results()
    );
}

#[test]
fn folders_and_the_latest_file_in_them_can_be_opened() {
    let mut h = Harness::new();
    h.actions.will_return(Ok(""));
    h.actions.will_return(Ok("Opened report.pdf"));
    h.chat
        .will_call("open_folder", json!({"folder": "downloads"}))
        .will_call("open_recent_file", json!({"folder": "Documents"}))
        .will_reply("Done.");

    h.type_text("Open my Downloads and my latest document");

    assert_eq!(
        h.actions.done(),
        vec![
            Action::OpenFolder(Folder::Downloads),
            Action::OpenRecentFile(Folder::Documents),
        ]
    );
    assert_eq!(
        h.chat.last_tool_results(),
        vec!["Done.", "Opened report.pdf"]
    );
}

#[test]
fn volume_can_be_set_raised_lowered_and_muted() {
    let mut h = Harness::new();
    h.chat
        .will_call("volume", json!({"change": "set", "level": 30}))
        .will_call("volume", json!({"change": "up"}))
        .will_call("volume", json!({"change": "down", "level": 5}))
        .will_reply("Done.");
    h.type_text("Volume things");
    h.chat
        .will_call("volume", json!({"change": "mute"}))
        .will_call("volume", json!({"change": "unmute"}))
        .will_call("volume", json!({"change": "set", "level": 250}))
        .will_reply("Done.");
    h.type_text("More volume things");

    assert_eq!(
        h.actions.done(),
        vec![
            Action::Volume(Volume::Set(30)),
            Action::Volume(Volume::Up(10)),
            Action::Volume(Volume::Down(5)),
            Action::Volume(Volume::Mute),
            Action::Volume(Volume::Unmute),
            Action::Volume(Volume::Set(100)),
        ]
    );
}

#[test]
fn media_keys_play_pause_skip_and_go_back() {
    let mut h = Harness::new();
    h.chat
        .will_call("media", json!({"key": "play_pause"}))
        .will_call("media", json!({"key": "next"}))
        .will_call("media", json!({"key": "previous"}))
        .will_reply("Done.");

    h.type_text("Media things");

    assert_eq!(
        h.actions.done(),
        vec![
            Action::Media(MediaKey::PlayPause),
            Action::Media(MediaKey::Next),
            Action::Media(MediaKey::Previous),
        ]
    );
}

#[test]
fn a_failed_action_is_reported_to_the_model() {
    let mut h = Harness::new();
    h.actions.will_return(Err("no audio device"));
    h.chat
        .will_call("volume", json!({"change": "mute"}))
        .will_reply("I couldn't mute; there's no audio device.");

    let outputs = h.type_text("Mute");

    assert_eq!(h.chat.last_tool_results(), vec!["Failed: no audio device"]);
    assert_eq!(
        outputs.last(),
        Some(&Output::Reply(
            "I couldn't mute; there's no audio device.".into()
        ))
    );
}

#[test]
fn the_whole_action_turn_is_kept_for_follow_ups() {
    let mut h = Harness::new();
    h.chat
        .will_call("media", json!({"key": "next"}))
        .will_reply("Skipped.")
        .will_reply("You skipped a song.");

    h.type_text("Next song");
    h.type_text("What did you just do?");

    let request = h.chat.last_request();
    let roles: Vec<Role> = request.iter().map(|m| m.role).collect();
    assert_eq!(
        roles,
        vec![
            Role::System,
            Role::User,
            Role::Assistant,
            Role::Tool,
            Role::Assistant,
            Role::User,
        ]
    );
    assert_eq!(request[2].tool_calls[0].name, "media");
    assert_eq!(request[3].tool_call_id.as_deref(), Some("call-1"));
}

#[test]
fn every_private_capable_browser_gets_a_private_window() {
    for kind in [
        BrowserKind::Chrome,
        BrowserKind::Brave,
        BrowserKind::Chromium,
        BrowserKind::Edge,
        BrowserKind::Firefox,
        BrowserKind::Opera,
    ] {
        let mut h = Harness::new();
        h.actions.use_browser(kind, "Browser");
        h.chat
            .will_call("web_search", json!({"query": "cats"}))
            .will_reply("Here you go.");

        let outputs = h.type_text("Search cats");

        assert!(opened_urls(&h)[0].1, "{kind:?}");
        assert_eq!(
            outputs.last(),
            Some(&Output::Reply("Here you go.".into())),
            "{kind:?}"
        );
    }
}
