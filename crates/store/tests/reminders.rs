//! Seam 2: the SQLite reminders store, against a real database file.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jarvis_core::{ReminderKind, Reminders};
use jarvis_store::reminders::SqliteReminders;

fn at(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

#[test]
fn reminders_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jarvis.db");

    let (tea, call) = {
        let mut store = SqliteReminders::open(&path).unwrap();
        let tea = store
            .add(ReminderKind::Timer, "tea", at(1_800_000_300))
            .unwrap();
        let call = store
            .add(ReminderKind::Reminder, "call mum", at(1_800_000_100))
            .unwrap();
        (tea, call)
    };

    let mut store = SqliteReminders::open(&path).unwrap();
    let pending = store.pending();
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0].id, call);
    assert_eq!(pending[0].kind, ReminderKind::Reminder);
    assert_eq!(pending[0].text, "call mum");
    assert_eq!(pending[0].due, at(1_800_000_100));
    assert_eq!(pending[1].id, tea);
    assert_eq!(pending[1].kind, ReminderKind::Timer);
}

#[test]
fn removed_reminders_stay_gone_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jarvis.db");
    {
        let mut store = SqliteReminders::open(&path).unwrap();
        let alarm = store
            .add(ReminderKind::Alarm, "", at(1_800_000_000))
            .unwrap();
        assert!(store.remove(alarm));
        assert!(!store.remove(alarm));
    }
    let mut store = SqliteReminders::open(&path).unwrap();
    assert!(store.pending().is_empty());
}

#[test]
fn ids_are_not_reused_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jarvis.db");
    let first = {
        let mut store = SqliteReminders::open(&path).unwrap();
        let id = store
            .add(ReminderKind::Timer, "", at(1_800_000_000))
            .unwrap();
        store.remove(id);
        id
    };
    let mut store = SqliteReminders::open(&path).unwrap();
    let second = store
        .add(ReminderKind::Timer, "", at(1_800_000_000))
        .unwrap();
    assert_ne!(first, second);
}
