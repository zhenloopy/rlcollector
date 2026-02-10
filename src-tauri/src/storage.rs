use crate::models::{CaptureSession, Screenshot, Task, TaskUpdate};
use rusqlite::{params, Connection, Result as SqlResult};
use std::path::Path;
use std::sync::Mutex;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Lock the database connection, converting a poisoned mutex into a rusqlite error.
    fn conn(&self) -> SqlResult<std::sync::MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|e| {
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
                Some(format!("Mutex poisoned: {}", e)),
            )
        })
    }

    pub fn new(path: &Path) -> SqlResult<Self> {
        let conn = Connection::open(path)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.initialize()?;
        Ok(db)
    }

    /// Create an in-memory database (for testing)
    #[cfg(test)]
    pub fn in_memory() -> SqlResult<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.initialize()?;
        Ok(db)
    }

    fn initialize(&self) -> SqlResult<()> {
        let conn = self.conn()?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS screenshots (
                id INTEGER PRIMARY KEY,
                filepath TEXT NOT NULL,
                captured_at TEXT NOT NULL,
                active_window_title TEXT,
                monitor_index INTEGER DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS tasks (
                id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                description TEXT,
                category TEXT,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                ai_reasoning TEXT,
                user_verified INTEGER DEFAULT 0,
                metadata TEXT
            );

            CREATE TABLE IF NOT EXISTS task_screenshots (
                task_id INTEGER REFERENCES tasks(id) ON DELETE CASCADE,
                screenshot_id INTEGER REFERENCES screenshots(id) ON DELETE CASCADE,
                PRIMARY KEY (task_id, screenshot_id)
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS capture_sessions (
                id INTEGER PRIMARY KEY,
                started_at TEXT NOT NULL,
                ended_at TEXT
            );",
        )?;

        // Migrate: add session_id column to screenshots if it doesn't exist
        let has_session_id: bool = {
            let mut stmt = conn.prepare("PRAGMA table_info(screenshots)")?;
            let columns = stmt.query_map([], |row| row.get::<_, String>(1))?
                .collect::<SqlResult<Vec<_>>>()?;
            columns.iter().any(|c| c == "session_id")
        };
        if !has_session_id {
            conn.execute_batch(
                "ALTER TABLE screenshots ADD COLUMN session_id INTEGER REFERENCES capture_sessions(id);"
            )?;
        }

        // Migrate: add description column to capture_sessions if it doesn't exist
        let has_description: bool = {
            let mut stmt = conn.prepare("PRAGMA table_info(capture_sessions)")?;
            let columns = stmt.query_map([], |row| row.get::<_, String>(1))?
                .collect::<SqlResult<Vec<_>>>()?;
            columns.iter().any(|c| c == "description")
        };
        if !has_description {
            conn.execute_batch(
                "ALTER TABLE capture_sessions ADD COLUMN description TEXT;"
            )?;
        }

        Ok(())
    }

    pub fn insert_screenshot(&self, filepath: &str, captured_at: &str, window_title: Option<&str>, monitor: i32, session_id: Option<i64>) -> SqlResult<i64> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO screenshots (filepath, captured_at, active_window_title, monitor_index, session_id) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![filepath, captured_at, window_title, monitor, session_id],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Get the total number of screenshots in the database.
    pub fn get_screenshot_count(&self) -> SqlResult<i64> {
        let conn = self.conn()?;
        conn.query_row("SELECT COUNT(*) FROM screenshots", [], |row| row.get(0))
    }

    /// Get a single screenshot by ID.
    pub fn get_screenshot(&self, id: i64) -> SqlResult<Screenshot> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT id, filepath, captured_at, active_window_title, monitor_index FROM screenshots WHERE id = ?1",
            params![id],
            |row| {
                Ok(Screenshot {
                    id: row.get(0)?,
                    filepath: row.get(1)?,
                    captured_at: row.get(2)?,
                    active_window_title: row.get(3)?,
                    monitor_index: row.get(4)?,
                })
            },
        )
    }

    /// Delete all screenshots that have not been linked to any task.
    /// Returns the filepaths of deleted rows so the caller can remove files from disk.
    pub fn delete_unanalyzed_screenshots(&self) -> SqlResult<Vec<String>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT s.filepath FROM screenshots s
             LEFT JOIN task_screenshots ts ON s.id = ts.screenshot_id
             WHERE ts.task_id IS NULL",
        )?;
        let paths = stmt.query_map([], |row| row.get::<_, String>(0))?
            .collect::<SqlResult<Vec<_>>>()?;
        conn.execute(
            "DELETE FROM screenshots WHERE id IN (
                SELECT s.id FROM screenshots s
                LEFT JOIN task_screenshots ts ON s.id = ts.screenshot_id
                WHERE ts.task_id IS NULL
            )",
            [],
        )?;
        Ok(paths)
    }

    /// Get screenshots that have not been linked to any task yet.
    pub fn get_unanalyzed_screenshots(&self, limit: i64) -> SqlResult<Vec<Screenshot>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT s.id, s.filepath, s.captured_at, s.active_window_title, s.monitor_index
             FROM screenshots s
             LEFT JOIN task_screenshots ts ON s.id = ts.screenshot_id
             WHERE ts.task_id IS NULL
             ORDER BY s.captured_at ASC
             LIMIT ?1",
        )?;
        let screenshots = stmt.query_map(params![limit], |row| {
            Ok(Screenshot {
                id: row.get(0)?,
                filepath: row.get(1)?,
                captured_at: row.get(2)?,
                active_window_title: row.get(3)?,
                monitor_index: row.get(4)?,
            })
        })?
        .collect::<SqlResult<Vec<_>>>()?;
        Ok(screenshots)
    }

    /// Insert a task with all AI-analyzed fields populated.
    pub fn insert_full_task(
        &self,
        title: &str,
        description: &str,
        category: &str,
        started_at: &str,
        ai_reasoning: &str,
    ) -> SqlResult<i64> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO tasks (title, description, category, started_at, ai_reasoning) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![title, description, category, started_at, ai_reasoning],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_tasks(&self, limit: i64, offset: i64) -> SqlResult<Vec<Task>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, title, description, category, started_at, ended_at, ai_reasoning, user_verified, metadata
             FROM tasks ORDER BY started_at DESC LIMIT ?1 OFFSET ?2",
        )?;
        let tasks = stmt.query_map(params![limit, offset], |row| {
            Ok(Task {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                category: row.get(3)?,
                started_at: row.get(4)?,
                ended_at: row.get(5)?,
                ai_reasoning: row.get(6)?,
                user_verified: row.get(7)?,
                metadata: row.get(8)?,
            })
        })?
        .collect::<SqlResult<Vec<_>>>()?;
        Ok(tasks)
    }

    pub fn get_task(&self, id: i64) -> SqlResult<Task> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT id, title, description, category, started_at, ended_at, ai_reasoning, user_verified, metadata
             FROM tasks WHERE id = ?1",
            params![id],
            |row| {
                Ok(Task {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    description: row.get(2)?,
                    category: row.get(3)?,
                    started_at: row.get(4)?,
                    ended_at: row.get(5)?,
                    ai_reasoning: row.get(6)?,
                    user_verified: row.get(7)?,
                    metadata: row.get(8)?,
                })
            },
        )
    }

    pub fn insert_task(&self, title: &str, started_at: &str) -> SqlResult<i64> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO tasks (title, started_at) VALUES (?1, ?2)",
            params![title, started_at],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_task(&self, id: i64, update: &TaskUpdate) -> SqlResult<()> {
        let conn = self.conn()?;
        if let Some(ref title) = update.title {
            conn.execute("UPDATE tasks SET title = ?1 WHERE id = ?2", params![title, id])?;
        }
        if let Some(ref desc) = update.description {
            conn.execute("UPDATE tasks SET description = ?1 WHERE id = ?2", params![desc, id])?;
        }
        if let Some(ref cat) = update.category {
            conn.execute("UPDATE tasks SET category = ?1 WHERE id = ?2", params![cat, id])?;
        }
        if let Some(ref verified) = update.user_verified {
            conn.execute("UPDATE tasks SET user_verified = ?1 WHERE id = ?2", params![verified, id])?;
        }
        Ok(())
    }

    pub fn delete_task(&self, id: i64) -> SqlResult<()> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn link_screenshot_to_task(&self, task_id: i64, screenshot_id: i64) -> SqlResult<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR IGNORE INTO task_screenshots (task_id, screenshot_id) VALUES (?1, ?2)",
            params![task_id, screenshot_id],
        )?;
        Ok(())
    }

    pub fn create_session(&self, started_at: &str, description: Option<&str>) -> SqlResult<i64> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO capture_sessions (started_at, description) VALUES (?1, ?2)",
            params![started_at, description],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn end_session(&self, id: i64, ended_at: &str) -> SqlResult<()> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE capture_sessions SET ended_at = ?1 WHERE id = ?2",
            params![ended_at, id],
        )?;
        Ok(())
    }

    pub fn get_sessions(&self, limit: i64, offset: i64) -> SqlResult<Vec<CaptureSession>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT cs.id, cs.started_at, cs.ended_at,
                    (SELECT COUNT(*) FROM screenshots s WHERE s.session_id = cs.id) as screenshot_count,
                    cs.description
             FROM capture_sessions cs
             ORDER BY cs.started_at DESC
             LIMIT ?1 OFFSET ?2",
        )?;
        let sessions = stmt.query_map(params![limit, offset], |row| {
            Ok(CaptureSession {
                id: row.get(0)?,
                started_at: row.get(1)?,
                ended_at: row.get(2)?,
                screenshot_count: row.get(3)?,
                description: row.get(4)?,
            })
        })?
        .collect::<SqlResult<Vec<_>>>()?;
        Ok(sessions)
    }

    pub fn get_session(&self, id: i64) -> SqlResult<CaptureSession> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT cs.id, cs.started_at, cs.ended_at,
                    (SELECT COUNT(*) FROM screenshots s WHERE s.session_id = cs.id) as screenshot_count,
                    cs.description
             FROM capture_sessions cs
             WHERE cs.id = ?1",
            params![id],
            |row| {
                Ok(CaptureSession {
                    id: row.get(0)?,
                    started_at: row.get(1)?,
                    ended_at: row.get(2)?,
                    screenshot_count: row.get(3)?,
                    description: row.get(4)?,
                })
            },
        )
    }

    /// Get the session_id for a given screenshot, if any.
    pub fn get_screenshot_session_id(&self, screenshot_id: i64) -> SqlResult<Option<i64>> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT session_id FROM screenshots WHERE id = ?1",
            params![screenshot_id],
            |row| row.get(0),
        )
    }

    pub fn get_session_screenshots(&self, session_id: i64) -> SqlResult<Vec<Screenshot>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, filepath, captured_at, active_window_title, monitor_index
             FROM screenshots
             WHERE session_id = ?1
             ORDER BY captured_at ASC",
        )?;
        let screenshots = stmt.query_map(params![session_id], |row| {
            Ok(Screenshot {
                id: row.get(0)?,
                filepath: row.get(1)?,
                captured_at: row.get(2)?,
                active_window_title: row.get(3)?,
                monitor_index: row.get(4)?,
            })
        })?
        .collect::<SqlResult<Vec<_>>>()?;
        Ok(screenshots)
    }

    pub fn get_setting(&self, key: &str) -> SqlResult<Option<String>> {
        let conn = self.conn()?;
        let result = conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        );
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> SqlResult<()> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get_task() {
        let db = Database::in_memory().unwrap();
        let id = db.insert_task("Test task", "2025-01-01T00:00:00").unwrap();
        let task = db.get_task(id).unwrap();
        assert_eq!(task.title, "Test task");
        assert_eq!(task.user_verified, false);
    }

    #[test]
    fn test_update_task() {
        let db = Database::in_memory().unwrap();
        let id = db.insert_task("Original", "2025-01-01T00:00:00").unwrap();
        db.update_task(id, &TaskUpdate {
            title: Some("Updated".to_string()),
            description: None,
            category: Some("coding".to_string()),
            ended_at: None,
            user_verified: Some(true),
        }).unwrap();
        let task = db.get_task(id).unwrap();
        assert_eq!(task.title, "Updated");
        assert_eq!(task.category, Some("coding".to_string()));
        assert_eq!(task.user_verified, true);
    }

    #[test]
    fn test_delete_task() {
        let db = Database::in_memory().unwrap();
        let id = db.insert_task("To delete", "2025-01-01T00:00:00").unwrap();
        db.delete_task(id).unwrap();
        assert!(db.get_task(id).is_err());
    }

    #[test]
    fn test_settings() {
        let db = Database::in_memory().unwrap();
        assert_eq!(db.get_setting("foo").unwrap(), None);
        db.set_setting("foo", "bar").unwrap();
        assert_eq!(db.get_setting("foo").unwrap(), Some("bar".to_string()));
        db.set_setting("foo", "baz").unwrap();
        assert_eq!(db.get_setting("foo").unwrap(), Some("baz".to_string()));
    }

    #[test]
    fn test_screenshot_task_link() {
        let db = Database::in_memory().unwrap();
        let task_id = db.insert_task("Task", "2025-01-01T00:00:00").unwrap();
        let ss_id = db.insert_screenshot("test.png", "2025-01-01T00:00:00", Some("Terminal"), 0, None).unwrap();
        db.link_screenshot_to_task(task_id, ss_id).unwrap();
        // Linking again should not fail (OR IGNORE)
        db.link_screenshot_to_task(task_id, ss_id).unwrap();
    }

    #[test]
    fn test_delete_unanalyzed_screenshots() {
        let db = Database::in_memory().unwrap();
        let ss1 = db.insert_screenshot("shot1.webp", "2025-01-01T00:00:00", None, 0, None).unwrap();
        let _ss2 = db.insert_screenshot("shot2.webp", "2025-01-01T00:00:01", None, 0, None).unwrap();
        let ss3 = db.insert_screenshot("shot3.webp", "2025-01-01T00:00:02", None, 0, None).unwrap();

        // Link ss1 to a task — it should NOT be deleted
        let task_id = db.insert_task("Task", "2025-01-01T00:00:00").unwrap();
        db.link_screenshot_to_task(task_id, ss1).unwrap();

        // Link ss3 to a task too
        db.link_screenshot_to_task(task_id, ss3).unwrap();

        // Only ss2 is unanalyzed
        let deleted = db.delete_unanalyzed_screenshots().unwrap();
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0], "shot2.webp");

        // Verify only 2 screenshots remain
        assert_eq!(db.get_screenshot_count().unwrap(), 2);
    }

    #[test]
    fn test_get_tasks_pagination() {
        let db = Database::in_memory().unwrap();
        for i in 0..5 {
            db.insert_task(&format!("Task {}", i), &format!("2025-01-0{}T00:00:00", i + 1)).unwrap();
        }
        let page1 = db.get_tasks(2, 0).unwrap();
        assert_eq!(page1.len(), 2);
        let page2 = db.get_tasks(2, 2).unwrap();
        assert_eq!(page2.len(), 2);
        let page3 = db.get_tasks(2, 4).unwrap();
        assert_eq!(page3.len(), 1);
    }

    #[test]
    fn test_get_screenshot() {
        let db = Database::in_memory().unwrap();
        let id = db.insert_screenshot("test.webp", "2025-01-01T00:00:00", Some("Terminal"), 0, None).unwrap();
        let screenshot = db.get_screenshot(id).unwrap();
        assert_eq!(screenshot.filepath, "test.webp");
        assert_eq!(screenshot.captured_at, "2025-01-01T00:00:00");
        assert_eq!(screenshot.active_window_title, Some("Terminal".to_string()));
        assert_eq!(screenshot.monitor_index, 0);
    }

    #[test]
    fn test_get_unanalyzed_screenshots() {
        let db = Database::in_memory().unwrap();
        let ss1 = db.insert_screenshot("shot1.webp", "2025-01-01T00:00:00", None, 0, None).unwrap();
        let _ss2 = db.insert_screenshot("shot2.webp", "2025-01-01T00:00:01", None, 0, None).unwrap();
        let _ss3 = db.insert_screenshot("shot3.webp", "2025-01-01T00:00:02", None, 0, None).unwrap();

        // Link ss1 to a task
        let task_id = db.insert_task("Task", "2025-01-01T00:00:00").unwrap();
        db.link_screenshot_to_task(task_id, ss1).unwrap();

        // Only 2 unanalyzed screenshots should remain
        let unanalyzed = db.get_unanalyzed_screenshots(10).unwrap();
        assert_eq!(unanalyzed.len(), 2);
        assert_eq!(unanalyzed[0].filepath, "shot2.webp");
        assert_eq!(unanalyzed[1].filepath, "shot3.webp");
    }

    #[test]
    fn test_insert_full_task() {
        let db = Database::in_memory().unwrap();
        let id = db.insert_full_task(
            "Writing code",
            "User is editing a Rust file",
            "coding",
            "2025-01-01T00:00:00",
            "IDE is open with Rust code",
        ).unwrap();
        let task = db.get_task(id).unwrap();
        assert_eq!(task.title, "Writing code");
        assert_eq!(task.description, Some("User is editing a Rust file".to_string()));
        assert_eq!(task.category, Some("coding".to_string()));
        assert_eq!(task.ai_reasoning, Some("IDE is open with Rust code".to_string()));
    }

    #[test]
    fn test_get_screenshot_count() {
        let db = Database::in_memory().unwrap();

        // Initially, count should be 0
        assert_eq!(db.get_screenshot_count().unwrap(), 0);

        // Insert 3 screenshots
        db.insert_screenshot("shot1.webp", "2025-01-01T00:00:00", None, 0, None).unwrap();
        db.insert_screenshot("shot2.webp", "2025-01-01T00:00:01", Some("Browser"), 0, None).unwrap();
        db.insert_screenshot("shot3.webp", "2025-01-01T00:00:02", Some("Editor"), 1, None).unwrap();

        // Count should be 3
        assert_eq!(db.get_screenshot_count().unwrap(), 3);
    }

    #[test]
    fn test_create_and_end_session() {
        let db = Database::in_memory().unwrap();
        let id = db.create_session("2025-01-01T10:00:00", None).unwrap();
        assert!(id > 0);

        db.end_session(id, "2025-01-01T10:30:00").unwrap();

        let sessions = db.get_sessions(10, 0).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, id);
        assert_eq!(sessions[0].started_at, "2025-01-01T10:00:00");
        assert_eq!(sessions[0].ended_at, Some("2025-01-01T10:30:00".to_string()));
        assert_eq!(sessions[0].screenshot_count, 0);
    }

    #[test]
    fn test_session_screenshot_count() {
        let db = Database::in_memory().unwrap();
        let session_id = db.create_session("2025-01-01T10:00:00", None).unwrap();

        db.insert_screenshot("s1.webp", "2025-01-01T10:00:00", None, 0, Some(session_id)).unwrap();
        db.insert_screenshot("s2.webp", "2025-01-01T10:00:30", None, 0, Some(session_id)).unwrap();
        db.insert_screenshot("s3.webp", "2025-01-01T10:01:00", None, 0, None).unwrap(); // no session

        let sessions = db.get_sessions(10, 0).unwrap();
        assert_eq!(sessions[0].screenshot_count, 2);
    }

    #[test]
    fn test_get_session_screenshots() {
        let db = Database::in_memory().unwrap();
        let session_id = db.create_session("2025-01-01T10:00:00", None).unwrap();

        db.insert_screenshot("s1.webp", "2025-01-01T10:00:00", None, 0, Some(session_id)).unwrap();
        db.insert_screenshot("s2.webp", "2025-01-01T10:00:30", Some("Editor"), 0, Some(session_id)).unwrap();
        db.insert_screenshot("other.webp", "2025-01-01T10:01:00", None, 0, None).unwrap();

        let screenshots = db.get_session_screenshots(session_id).unwrap();
        assert_eq!(screenshots.len(), 2);
        assert_eq!(screenshots[0].filepath, "s1.webp");
        assert_eq!(screenshots[1].filepath, "s2.webp");
    }

    #[test]
    fn test_session_description() {
        let db = Database::in_memory().unwrap();
        let id = db.create_session("2025-01-01T10:00:00", Some("Building a React form")).unwrap();
        let session = db.get_session(id).unwrap();
        assert_eq!(session.description, Some("Building a React form".to_string()));

        // Session without description
        let id2 = db.create_session("2025-01-01T11:00:00", None).unwrap();
        let session2 = db.get_session(id2).unwrap();
        assert_eq!(session2.description, None);
    }

    #[test]
    fn test_get_screenshot_session_id() {
        let db = Database::in_memory().unwrap();
        let session_id = db.create_session("2025-01-01T10:00:00", None).unwrap();
        let ss_id = db.insert_screenshot("s1.webp", "2025-01-01T10:00:00", None, 0, Some(session_id)).unwrap();
        let ss_no_session = db.insert_screenshot("s2.webp", "2025-01-01T10:00:01", None, 0, None).unwrap();

        assert_eq!(db.get_screenshot_session_id(ss_id).unwrap(), Some(session_id));
        assert_eq!(db.get_screenshot_session_id(ss_no_session).unwrap(), None);
    }

    #[test]
    fn test_get_sessions_pagination() {
        let db = Database::in_memory().unwrap();
        for i in 0..5 {
            db.create_session(&format!("2025-01-0{}T10:00:00", i + 1), None).unwrap();
        }
        let page1 = db.get_sessions(2, 0).unwrap();
        assert_eq!(page1.len(), 2);
        let page2 = db.get_sessions(2, 2).unwrap();
        assert_eq!(page2.len(), 2);
        let page3 = db.get_sessions(2, 4).unwrap();
        assert_eq!(page3.len(), 1);
    }
}
