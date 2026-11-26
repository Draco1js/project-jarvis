//! Seam 2: the SQLite history and memory store, against a real database and
//! notes folder in a temporary directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use jarvis_core::{MemoryKind, Message, Role, Store, Tagged};
use jarvis_store::memory::SqliteStore;
use tempfile::TempDir;

/// 2026-11-26 12:00:00 UTC.
fn noon() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_795_694_400)
}

fn tagged(summary: &str, tags: &[&str]) -> Tagged {
    Tagged {
        summary: summary.into(),
        tags: tags.iter().map(|t| t.to_string()).collect(),
    }
}

struct Dirs {
    _root: TempDir,
    db: PathBuf,
    notes: PathBuf,
}

fn dirs() -> Dirs {
    let root = tempfile::tempdir().unwrap();
    let db = root.path().join("data").join("jarvis.sqlite3");
    let notes = root.path().join("Jarvis Notes");
    Dirs {
        _root: root,
        db,
        notes,
    }
}

fn open(d: &Dirs) -> SqliteStore {
    SqliteStore::open(&d.db, d.notes.clone()).unwrap()
}

fn pune_conversation(store: &mut SqliteStore) {
    store.save_message(
        noon(),
        &Message::new(Role::User, "Will it rain in Pune tomorrow?"),
    );
    store.save_message(
        noon(),
        &Message::new(Role::Assistant, "Heavy showers are expected."),
    );
    store.end_conversation(
        noon() + Duration::from_secs(300),
        &tagged(
            "The user asked about the forecast.",
            &["weather", "monsoon"],
        ),
    );
}

#[test]
fn ended_conversation_is_found_by_its_text_summary_or_tags() {
    let d = dirs();
    let mut store = open(&d);
    pune_conversation(&mut store);

    for query in ["Pune", "forecast", "monsoon", "showers"] {
        let hits = store.search(query, 5);
        assert_eq!(hits.len(), 1, "{query}");
        let hit = &hits[0];
        assert_eq!(hit.kind, MemoryKind::Conversation);
        assert_eq!(hit.at, noon());
        assert_eq!(hit.summary, "The user asked about the forecast.");
        assert_eq!(hit.tags, vec!["weather", "monsoon"]);
    }
    let hit = &store.search("Pune", 5)[0];
    assert!(hit.excerpt.contains("Pune"), "{}", hit.excerpt);
    assert!(store.search("giraffe", 5).is_empty());
}

#[test]
fn search_matches_word_forms_and_any_word() {
    let d = dirs();
    let mut store = open(&d);
    pune_conversation(&mut store);

    assert_eq!(store.search("raining", 5).len(), 1, "stemmed");
    assert_eq!(store.search("giraffes or Pune", 5).len(), 1, "any word");
}

#[test]
fn search_survives_query_syntax_characters() {
    let d = dirs();
    let mut store = open(&d);
    pune_conversation(&mut store);

    for query in [r#"what's "Pune" AND (rain"#, "NEAR(", "*", "", "-:^"] {
        let _ = store.search(query, 5);
    }
    assert_eq!(store.search(r#""Pune*"#, 5).len(), 1);
}

#[test]
fn best_match_comes_first_and_limit_is_kept() {
    let d = dirs();
    let mut store = open(&d);
    for (i, text) in ["cats", "cats and dogs", "dogs dogs dogs"]
        .iter()
        .enumerate()
    {
        let at = noon() + Duration::from_secs(i as u64 * 600);
        store.save_message(at, &Message::new(Role::User, *text));
        store.end_conversation(at, &tagged("", &[]));
    }

    let hits = store.search("dogs", 5);
    assert_eq!(hits.len(), 2);
    assert!(hits[0].excerpt.contains("dogs dogs"), "{:?}", hits[0]);
    assert_eq!(store.search("dogs", 1).len(), 1);
}

#[test]
fn conversation_is_not_searchable_until_it_ends() {
    let d = dirs();
    let mut store = open(&d);
    store.save_message(noon(), &Message::new(Role::User, "Pune weather?"));

    assert!(store.search("Pune", 5).is_empty());
}

#[test]
fn history_survives_a_restart() {
    let d = dirs();
    pune_conversation(&mut open(&d));

    assert_eq!(open(&d).search("Pune", 5).len(), 1);
}

#[test]
fn conversation_left_open_by_a_crash_is_kept_on_next_start() {
    let d = dirs();
    {
        let mut store = open(&d);
        store.save_message(noon(), &Message::new(Role::User, "Remind me about Pune"));
    }

    let mut store = open(&d);
    let hits = store.search("Pune", 5);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].summary, "");

    store.save_message(noon(), &Message::new(Role::User, "Something new"));
    store.end_conversation(noon(), &tagged("New one.", &["new"]));
    assert_eq!(
        store.search("Pune", 5)[0].summary,
        "",
        "old one stays separate"
    );
    assert_eq!(store.search("new", 5)[0].summary, "New one.");
}

#[test]
fn clearing_history_forgets_conversations_but_keeps_notes() {
    let d = dirs();
    let mut store = open(&d);
    pune_conversation(&mut store);
    store
        .save_note(
            noon(),
            "Pune trip in March",
            &tagged("A trip.", &["travel"]),
        )
        .unwrap();

    store.clear_history();

    let hits = store.search("Pune", 5);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, MemoryKind::Note);
    assert!(
        open(&d).search("showers", 5).is_empty(),
        "gone after restart too"
    );
}

fn note_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    files
}

#[test]
fn note_is_a_markdown_file_with_its_tags() {
    let d = dirs();
    let mut store = open(&d);

    let saved = store
        .save_note(
            noon(),
            "The wifi password is on the fridge.",
            &tagged("Where the Wi-Fi password is.", &["wifi", "password"]),
        )
        .unwrap();

    let files = note_files(&d.notes);
    assert_eq!(files.len(), 1);
    assert_eq!(saved, files[0].display().to_string());
    let name = files[0].file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with("2026-11-26"), "{name}");
    assert!(name.ends_with(".md"), "{name}");
    assert!(name.contains("wifi-password"), "{name}");

    let text = fs::read_to_string(&files[0]).unwrap();
    assert!(text.starts_with("---\n"), "{text}");
    assert!(text.contains("tags: [wifi, password]"), "{text}");
    assert!(
        text.contains("summary: Where the Wi-Fi password is."),
        "{text}"
    );
    assert!(
        text.trim_end()
            .ends_with("The wifi password is on the fridge."),
        "{text}"
    );
}

#[test]
fn notes_are_searchable_by_text_and_tags() {
    let d = dirs();
    let mut store = open(&d);
    store
        .save_note(
            noon(),
            "The wifi password is on the fridge.",
            &tagged("Where the Wi-Fi password is.", &["internet", "router"]),
        )
        .unwrap();

    for query in ["fridge", "router"] {
        let hits = store.search(query, 5);
        assert_eq!(hits.len(), 1, "{query}");
        assert_eq!(hits[0].kind, MemoryKind::Note);
        assert_eq!(hits[0].at, noon());
        assert_eq!(hits[0].tags, vec!["internet", "router"]);
        assert_eq!(hits[0].summary, "Where the Wi-Fi password is.");
    }
}

#[test]
fn notes_with_the_same_start_get_their_own_files() {
    let d = dirs();
    let mut store = open(&d);
    store.save_note(noon(), "milk", &Tagged::default()).unwrap();
    store.save_note(noon(), "milk", &Tagged::default()).unwrap();

    assert_eq!(note_files(&d.notes).len(), 2);
}

#[test]
fn notes_folder_is_indexed_on_start_including_hand_written_notes() {
    let d = dirs();
    open(&d)
        .save_note(
            noon(),
            "Pune trip in March",
            &tagged("A trip.", &["travel"]),
        )
        .unwrap();
    fs::write(d.notes.join("groceries.md"), "Buy cardamom and saffron.\n").unwrap();
    fs::remove_file(&d.db).unwrap();

    let mut store = open(&d);

    let trip = store.search("travel", 5);
    assert_eq!(trip.len(), 1);
    assert_eq!(trip[0].at, noon());
    assert_eq!(trip[0].summary, "A trip.");
    let groceries = store.search("saffron", 5);
    assert_eq!(groceries.len(), 1);
    assert_eq!(groceries[0].kind, MemoryKind::Note);
    assert_eq!(open(&d).search("saffron", 5).len(), 1, "not indexed twice");
}

#[test]
fn deleted_note_files_drop_out_of_search_on_next_start() {
    let d = dirs();
    let saved = open(&d)
        .save_note(noon(), "Pune trip in March", &Tagged::default())
        .unwrap();
    fs::remove_file(saved).unwrap();

    assert!(open(&d).search("Pune", 5).is_empty());
}
