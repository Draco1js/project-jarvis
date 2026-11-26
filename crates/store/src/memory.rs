//! Conversation history and notes in SQLite, searchable with FTS5.
//!
//! Conversations live only in the database. Notes are Markdown files in the
//! notes folder (the files are the source of truth); the database indexes
//! them and re-syncs the folder on every start, so hand-written or deleted
//! notes are picked up.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jarvis_core::{MemoryHit, MemoryKind, Message, Role, Store, Tagged};
use rusqlite::{Connection, OptionalExtension, params};

/// The default notes folder: `Documents/Jarvis Notes`.
pub fn default_notes_dir() -> Option<PathBuf> {
    dirs::document_dir().map(|dir| dir.join("Jarvis Notes"))
}

const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = "
CREATE TABLE conversations (
    id INTEGER PRIMARY KEY,
    started_ms INTEGER NOT NULL,
    ended_ms INTEGER,
    summary TEXT NOT NULL DEFAULT '',
    tags TEXT NOT NULL DEFAULT ''
);
CREATE TABLE messages (
    id INTEGER PRIMARY KEY,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    at_ms INTEGER NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL
);
CREATE INDEX messages_by_conversation ON messages(conversation_id);
CREATE TABLE notes (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    modified_ms INTEGER NOT NULL,
    at_ms INTEGER NOT NULL,
    summary TEXT NOT NULL,
    tags TEXT NOT NULL
);
CREATE VIRTUAL TABLE memory_fts USING fts5(
    kind UNINDEXED,
    item UNINDEXED,
    body,
    summary,
    tags,
    tokenize = 'porter unicode61'
);
";

const CONVERSATION: &str = "conversation";
const NOTE: &str = "note";

/// Words in the first part of a note used for its file name.
const NOTE_NAME_WORDS: usize = 6;

pub struct SqliteStore {
    db: Connection,
    notes_dir: PathBuf,
    /// The conversation messages are being added to, if one is open.
    current: Option<i64>,
}

impl SqliteStore {
    /// Open (creating if needed) the database at `db_path`, close any
    /// conversation a crash left open, and index the notes folder.
    pub fn open(db_path: &Path, notes_dir: PathBuf) -> Result<Self, String> {
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let db = Connection::open(db_path).map_err(|e| e.to_string())?;
        // Reminders use the same file through their own connection.
        db.busy_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        let mut store = Self {
            db,
            notes_dir,
            current: None,
        };
        store.migrate().map_err(|e| e.to_string())?;
        store
            .close_open_conversations()
            .map_err(|e| e.to_string())?;
        store.sync_notes().map_err(|e| e.to_string())?;
        Ok(store)
    }

    fn migrate(&mut self) -> rusqlite::Result<()> {
        self.db.pragma_update(None, "foreign_keys", true)?;
        let version: i64 = self
            .db
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < 1 {
            let tx = self.db.transaction()?;
            tx.execute_batch(SCHEMA)?;
            tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            tx.commit()?;
        }
        Ok(())
    }

    /// A conversation still open at start was cut off by a crash or a kill:
    /// keep it, searchable, with no summary.
    fn close_open_conversations(&mut self) -> rusqlite::Result<()> {
        let open: Vec<i64> = self
            .db
            .prepare("SELECT id FROM conversations WHERE ended_ms IS NULL")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        for id in open {
            let last: Option<i64> = self.db.query_row(
                "SELECT MAX(at_ms) FROM messages WHERE conversation_id = ?1",
                [id],
                |r| r.get(0),
            )?;
            match last {
                Some(last) => self.finish_conversation(id, last, &Tagged::default())?,
                None => {
                    self.db
                        .execute("DELETE FROM conversations WHERE id = ?1", [id])?;
                }
            }
        }
        Ok(())
    }

    fn finish_conversation(
        &mut self,
        id: i64,
        ended_ms: i64,
        tagged: &Tagged,
    ) -> rusqlite::Result<()> {
        let tags = join_tags(&tagged.tags);
        let tx = self.db.transaction()?;
        tx.execute(
            "UPDATE conversations SET ended_ms = ?2, summary = ?3, tags = ?4 WHERE id = ?1",
            params![id, ended_ms, tagged.summary, tags],
        )?;
        let body: String = tx.query_row(
            "SELECT COALESCE(group_concat(content, char(10)), '') FROM
                (SELECT content FROM messages WHERE conversation_id = ?1 ORDER BY id)",
            [id],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO memory_fts (kind, item, body, summary, tags) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![CONVERSATION, id, body, tagged.summary, tags],
        )?;
        tx.commit()
    }

    /// Bring the notes index in line with the files in the notes folder.
    fn sync_notes(&mut self) -> rusqlite::Result<()> {
        let mut on_disk = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.notes_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("md"))
                    && path.is_file()
                {
                    on_disk.push(path);
                }
            }
        }

        let indexed: Vec<(i64, String, i64)> = self
            .db
            .prepare("SELECT id, path, modified_ms FROM notes")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<_, _>>()?;
        for (id, path, modified) in &indexed {
            let path = PathBuf::from(path);
            let current = on_disk
                .contains(&path)
                .then(|| modified_ms(&path))
                .flatten();
            if current != Some(*modified) {
                self.remove_note(*id)?;
            }
        }

        for path in on_disk {
            let key = path.display().to_string();
            let known: Option<i64> = self
                .db
                .query_row("SELECT id FROM notes WHERE path = ?1", [&key], |r| r.get(0))
                .optional()?;
            if known.is_some() {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let modified = modified_ms(&path).unwrap_or(0);
            let note = parse_note(&text);
            let at = note.at.map(to_ms).unwrap_or(modified);
            self.index_note(&key, modified, at, note.body, &note.tagged)?;
        }
        Ok(())
    }

    fn remove_note(&mut self, id: i64) -> rusqlite::Result<()> {
        let tx = self.db.transaction()?;
        tx.execute(
            "DELETE FROM memory_fts WHERE kind = ?1 AND item = ?2",
            params![NOTE, id],
        )?;
        tx.execute("DELETE FROM notes WHERE id = ?1", [id])?;
        tx.commit()
    }

    fn index_note(
        &mut self,
        path: &str,
        modified: i64,
        at: i64,
        body: &str,
        tagged: &Tagged,
    ) -> rusqlite::Result<()> {
        let tags = join_tags(&tagged.tags);
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT INTO notes (path, modified_ms, at_ms, summary, tags) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![path, modified, at, tagged.summary, tags],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO memory_fts (kind, item, body, summary, tags) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![NOTE, id, body, tagged.summary, tags],
        )?;
        tx.commit()
    }

    fn try_save_message(&mut self, at: SystemTime, message: &Message) -> rusqlite::Result<()> {
        let at = to_ms(at);
        let id = match self.current {
            Some(id) => id,
            None => {
                self.db
                    .execute("INSERT INTO conversations (started_ms) VALUES (?1)", [at])?;
                let id = self.db.last_insert_rowid();
                self.current = Some(id);
                id
            }
        };
        self.db.execute(
            "INSERT INTO messages (conversation_id, at_ms, role, content) VALUES (?1, ?2, ?3, ?4)",
            params![id, at, role_name(message.role), message.content],
        )?;
        Ok(())
    }

    fn try_clear_history(&mut self) -> rusqlite::Result<()> {
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM memory_fts WHERE kind = ?1", [CONVERSATION])?;
        tx.execute("DELETE FROM messages", [])?;
        tx.execute("DELETE FROM conversations", [])?;
        tx.commit()?;
        self.current = None;
        // Reclaim the space so deleted text does not linger in the file.
        if let Err(e) = self.db.execute_batch("VACUUM") {
            log::warn!("history cleared, but the database could not be compacted: {e}");
        }
        Ok(())
    }

    fn try_search(&mut self, query: &str, limit: usize) -> rusqlite::Result<Vec<MemoryHit>> {
        let Some(fts_query) = fts_query(query) else {
            return Ok(Vec::new());
        };
        let mut statement = self.db.prepare(
            "SELECT f.kind,
                    COALESCE(c.started_ms, n.at_ms, 0),
                    f.summary,
                    f.tags,
                    snippet(memory_fts, 2, '', '', '…', 16)
             FROM memory_fts f
             LEFT JOIN conversations c ON f.kind = 'conversation' AND c.id = f.item
             LEFT JOIN notes n ON f.kind = 'note' AND n.id = f.item
             WHERE memory_fts MATCH ?1
             ORDER BY f.rank
             LIMIT ?2",
        )?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        statement
            .query_map(params![fts_query, limit], |r| {
                let kind: String = r.get(0)?;
                let tags: String = r.get(3)?;
                Ok(MemoryHit {
                    kind: if kind == NOTE {
                        MemoryKind::Note
                    } else {
                        MemoryKind::Conversation
                    },
                    at: from_ms(r.get(1)?),
                    summary: r.get(2)?,
                    tags: split_tags(&tags),
                    excerpt: r.get(4)?,
                })
            })?
            .collect()
    }

    fn write_note_file(
        &self,
        at: SystemTime,
        text: &str,
        tagged: &Tagged,
    ) -> Result<PathBuf, String> {
        fs::create_dir_all(&self.notes_dir)
            .map_err(|e| format!("{}: {e}", self.notes_dir.display()))?;
        let stamp = humantime::format_rfc3339_seconds(at).to_string();
        let date = &stamp[..10];
        let stem = format!("{date} {}", slug(text));
        let contents = note_file_contents(&stamp, text, tagged);
        for n in 1.. {
            let name = if n == 1 {
                format!("{stem}.md")
            } else {
                format!("{stem} {n}.md")
            };
            let path = self.notes_dir.join(name);
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    use std::io::Write;
                    file.write_all(contents.as_bytes())
                        .map_err(|e| format!("{}: {e}", path.display()))?;
                    return Ok(path);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("{}: {e}", path.display())),
            }
        }
        unreachable!("some numbered file name is free")
    }
}

impl Store for SqliteStore {
    fn save_message(&mut self, at: SystemTime, message: &Message) {
        if let Err(e) = self.try_save_message(at, message) {
            log::warn!("could not save a message to history: {e}");
        }
    }

    fn end_conversation(&mut self, at: SystemTime, tagged: &Tagged) {
        let Some(id) = self.current.take() else {
            return;
        };
        if let Err(e) = self.finish_conversation(id, to_ms(at), tagged) {
            log::warn!("could not close a conversation in history: {e}");
        }
    }

    fn clear_history(&mut self) {
        if let Err(e) = self.try_clear_history() {
            log::warn!("could not clear history: {e}");
        }
    }

    fn search(&mut self, query: &str, limit: usize) -> Vec<MemoryHit> {
        self.try_search(query, limit).unwrap_or_else(|e| {
            log::warn!("memory search failed: {e}");
            Vec::new()
        })
    }

    fn save_note(&mut self, at: SystemTime, text: &str, tagged: &Tagged) -> Result<String, String> {
        let path = self.write_note_file(at, text, tagged)?;
        let key = path.display().to_string();
        let modified = modified_ms(&path).unwrap_or(0);
        if let Err(e) = self.index_note(&key, modified, to_ms(at), text, tagged) {
            // The file is saved; it gets indexed on the next start.
            log::warn!("could not index note {key}: {e}");
        }
        Ok(key)
    }
}

/// The user's words as an FTS5 query matching any of them. Every word is
/// quoted, so query syntax in what was said can't break the search.
fn fts_query(query: &str) -> Option<String> {
    let words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .filter(|w| !matches!(w.to_lowercase().as_str(), "and" | "or" | "not" | "near"))
        .map(|w| format!("\"{w}\""))
        .collect();
    (!words.is_empty()).then(|| words.join(" OR "))
}

fn note_file_contents(stamp: &str, text: &str, tagged: &Tagged) -> String {
    let summary = one_line(&tagged.summary);
    let tags = tagged
        .tags
        .iter()
        .map(|t| one_line(t).replace([',', '[', ']'], " "))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "---\ndate: {stamp}\nsummary: {summary}\ntags: [{tags}]\n---\n\n{}\n",
        text.trim()
    )
}

struct ParsedNote<'a> {
    at: Option<SystemTime>,
    tagged: Tagged,
    body: &'a str,
}

/// Reads the front matter Jarvis writes; a file without one is all body.
fn parse_note(text: &str) -> ParsedNote<'_> {
    let mut note = ParsedNote {
        at: None,
        tagged: Tagged::default(),
        body: text.trim(),
    };
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return note;
    };
    let Some(end) = rest.find("\n---") else {
        return note;
    };
    for line in rest[..end].lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "date" => note.at = humantime::parse_rfc3339_weak(value).ok(),
            "summary" => note.tagged.summary = value.to_string(),
            "tags" => {
                note.tagged.tags = split_tags(value.trim_start_matches('[').trim_end_matches(']'))
            }
            _ => {}
        }
    }
    let after = &rest[end + 4..];
    note.body = after.split_once('\n').map_or("", |(_, body)| body).trim();
    note
}

/// A file-name-safe start of the note: `the-wifi-password-is-on-the`.
fn slug(text: &str) -> String {
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(NOTE_NAME_WORDS)
        .map(str::to_lowercase)
        .collect();
    if words.is_empty() {
        "note".into()
    } else {
        words.join("-")
    }
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn join_tags(tags: &[String]) -> String {
    tags.join(", ")
}

fn split_tags(tags: &str) -> Vec<String> {
    tags.split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(String::from)
        .collect()
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

fn modified_ms(path: &Path) -> Option<i64> {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .map(to_ms)
}

fn to_ms(at: SystemTime) -> i64 {
    at.duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn from_ms(ms: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(u64::try_from(ms).unwrap_or(0))
}
