//! Log files on disk: rotation limits and secret redaction.

use std::fs;
use std::io::Write;
use std::path::Path;

use jarvis_store::logs::{FileLogger, RotatingFile, redact};

fn log_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn rotates_into_at_most_five_files_of_bounded_size() {
    let dir = tempfile::tempdir().unwrap();
    let mut file = RotatingFile::open(dir.path(), 100, 5).unwrap();

    for i in 0..200 {
        writeln!(file, "line {i:03} of the log").unwrap(); // 21 bytes
    }
    drop(file);

    assert_eq!(
        log_files(dir.path()),
        vec![
            "jarvis.log",
            "jarvis.log.1",
            "jarvis.log.2",
            "jarvis.log.3",
            "jarvis.log.4"
        ]
    );
    for name in log_files(dir.path()) {
        let len = fs::metadata(dir.path().join(&name)).unwrap().len();
        assert!(len <= 100, "{name} is {len} bytes");
    }
    // Newest lines are in jarvis.log, the next oldest in .1.
    let current = fs::read_to_string(dir.path().join("jarvis.log")).unwrap();
    assert!(current.ends_with("line 199 of the log\n"), "{current}");
    let previous = fs::read_to_string(dir.path().join("jarvis.log.1")).unwrap();
    assert!(previous.contains("line 19"), "{previous}");
}

#[test]
fn keeps_appending_to_an_existing_log_across_restarts() {
    let dir = tempfile::tempdir().unwrap();
    writeln!(
        RotatingFile::open(dir.path(), 1000, 5).unwrap(),
        "first run"
    )
    .unwrap();
    writeln!(
        RotatingFile::open(dir.path(), 1000, 5).unwrap(),
        "second run"
    )
    .unwrap();

    let text = fs::read_to_string(dir.path().join("jarvis.log")).unwrap();
    assert_eq!(text, "first run\nsecond run\n");
}

#[test]
fn redacts_known_secrets_and_key_shaped_strings() {
    let gemini = "AIzaSyD-1234567890abcdefghijklmnopqrstu";
    let groq = "gsk_abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKL";
    let custom = "my-own-secret-value";

    let line = format!(
        "gemini={gemini} groq={groq} custom={custom} header=Authorization: Bearer tok.en-123 ok"
    );
    let redacted = redact(&line, &[custom.to_string()]);

    for secret in [gemini, groq, custom, "tok.en-123"] {
        assert!(!redacted.contains(secret), "{secret} leaked: {redacted}");
    }
    assert!(redacted.ends_with(" ok"), "{redacted}");
    assert!(redacted.contains("[REDACTED]"), "{redacted}");
}

#[test]
fn logger_writes_redacted_records() {
    let dir = tempfile::tempdir().unwrap();
    let logger = FileLogger::open(dir.path(), vec!["sekrit-key".into()]).unwrap();

    log::Log::log(
        &logger,
        &log::Record::builder()
            .level(log::Level::Warn)
            .target("jarvis_llm")
            .args(format_args!("request failed with key sekrit-key"))
            .build(),
    );
    log::Log::flush(&logger);

    let text = fs::read_to_string(dir.path().join("jarvis.log")).unwrap();
    assert!(text.contains("WARN"), "{text}");
    assert!(text.contains("jarvis_llm"), "{text}");
    assert!(
        text.contains("request failed with key [REDACTED]"),
        "{text}"
    );
    assert!(!text.contains("sekrit-key"), "{text}");
}
