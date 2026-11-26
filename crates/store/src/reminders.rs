//! Timers, alarms and reminders saved in SQLite, so they survive restarts.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jarvis_core::{Reminder, ReminderKind, Reminders};
use rusqlite::{Connection, params};

/// The [`Reminders`] port backed by a SQLite database file.
pub struct SqliteReminders {
    db: Connection,
}

impl SqliteReminders {
    /// Open (or create) the database at `path` and its reminders table.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let db = Connection::open(path)?;
        // AUTOINCREMENT so an id the user heard is never given to a new one.
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS reminders (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL,
                text TEXT NOT NULL,
                due_ms INTEGER NOT NULL
            );",
        )?;
        Ok(Self { db })
    }
}

impl Reminders for SqliteReminders {
    fn add(&mut self, kind: ReminderKind, text: &str, due: SystemTime) -> i64 {
        let result = self.db.execute(
            "INSERT INTO reminders (kind, text, due_ms) VALUES (?1, ?2, ?3)",
            params![kind_name(kind), text, millis(due)],
        );
        match result {
            Ok(_) => self.db.last_insert_rowid(),
            Err(err) => {
                log::error!("could not save a reminder: {err}");
                -1
            }
        }
    }

    fn pending(&mut self) -> Vec<Reminder> {
        let rows = self
            .db
            .prepare("SELECT id, kind, text, due_ms FROM reminders ORDER BY due_ms, id")
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()
            });
        match rows {
            Ok(rows) => rows
                .into_iter()
                .filter_map(|(id, kind, text, due_ms)| {
                    Some(Reminder {
                        id,
                        kind: kind_from_name(&kind)?,
                        text,
                        due: UNIX_EPOCH + Duration::from_millis(due_ms.max(0) as u64),
                    })
                })
                .collect(),
            Err(err) => {
                log::error!("could not read reminders: {err}");
                Vec::new()
            }
        }
    }

    fn remove(&mut self, id: i64) -> bool {
        match self
            .db
            .execute("DELETE FROM reminders WHERE id = ?1", params![id])
        {
            Ok(n) => n > 0,
            Err(err) => {
                log::error!("could not remove reminder {id}: {err}");
                false
            }
        }
    }
}

fn millis(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn kind_name(kind: ReminderKind) -> &'static str {
    match kind {
        ReminderKind::Timer => "timer",
        ReminderKind::Alarm => "alarm",
        ReminderKind::Reminder => "reminder",
    }
}

fn kind_from_name(name: &str) -> Option<ReminderKind> {
    match name {
        "timer" => Some(ReminderKind::Timer),
        "alarm" => Some(ReminderKind::Alarm),
        "reminder" => Some(ReminderKind::Reminder),
        _ => None,
    }
}
