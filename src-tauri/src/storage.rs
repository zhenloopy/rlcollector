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

        // Migrate: add title column to capture_sessions if it doesn't exist
        let has_title: bool = {
            let mut stmt = conn.prepare("PRAGMA table_info(capture_sessions)")?;
            let columns = stmt.query_map([], |row| row.get::<_, String>(1))?
                .collect::<SqlResult<Vec<_>>>()?;
            columns.iter().any(|c| c == "title")
        };
        if !has_title {
            conn.execute_batch(
                "ALTER TABLE capture_sessions ADD COLUMN title TEXT;"
            )?;
        }

        // Migrate: add capture_group column to screenshots if it doesn't exist
        let has_capture_group: bool = {
            let mut stmt = conn.prepare("PRAGMA table_info(screenshots)")?;
            let columns = stmt.query_map([], |row| row.get::<_, String>(1))?
                .collect::<SqlResult<Vec<_>>>()?;
            columns.iter().any(|c| c == "capture_group")
        };
        if !has_capture_group {
            conn.execute_batch(
                "ALTER TABLE screenshots ADD COLUMN capture_group TEXT;"
            )?;
        }

        Ok(())
    }

    pub fn insert_screenshot(&self, filepath: &str, captured_at: &str, window_title: Option<&str>, monitor: i32, session_id: Option<i64>, capture_group: Option<&str>) -> SqlResult<i64> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO screenshots (filepath, captured_at, active_window_title, monitor_index, session_id, capture_group) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![filepath, captured_at, window_title, monitor, session_id, capture_group],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Get the total number of screenshots in the database.
    #[cfg(test)]
    pub fn get_screenshot_count(&self) -> SqlResult<i64> {
        let conn = self.conn()?;
        conn.query_row("SELECT COUNT(*) FROM screenshots", [], |row| row.get(0))
    }

    /// Get a single screenshot by ID.
    #[cfg(test)]
    pub fn get_screenshot(&self, id: i64) -> SqlResult<Screenshot> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT id, filepath, captured_at, active_window_title, monitor_index, capture_group FROM screenshots WHERE id = ?1",
            params![id],
            |row| {
                Ok(Screenshot {
                    id: row.get(0)?,
                    filepath: row.get(1)?,
                    captured_at: row.get(2)?,
                    active_window_title: row.get(3)?,
                    monitor_index: row.get(4)?,
                    capture_group: row.get(5)?,
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
            "SELECT s.id, s.filepath, s.captured_at, s.active_window_title, s.monitor_index, s.capture_group
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
                capture_group: row.get(5)?,
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

    #[cfg(test)]
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

    pub fn create_session(&self, started_at: &str, description: Option<&str>, title: Option<&str>) -> SqlResult<i64> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO capture_sessions (started_at, description, title) VALUES (?1, ?2, ?3)",
            params![started_at, description, title],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Delete a session and all its associated data.
    /// Returns the filepaths of deleted screenshots so the caller can remove files from disk.
    pub fn delete_session(&self, id: i64) -> SqlResult<Vec<String>> {
        let conn = self.conn()?;

        // 1. Collect screenshot filepaths for this session
        let mut stmt = conn.prepare(
            "SELECT filepath FROM screenshots WHERE session_id = ?1",
        )?;
        let paths = stmt.query_map(params![id], |row| row.get::<_, String>(0))?
            .collect::<SqlResult<Vec<_>>>()?;

        // 2. Collect screenshot IDs
        let mut stmt = conn.prepare(
            "SELECT id FROM screenshots WHERE session_id = ?1",
        )?;
        let screenshot_ids = stmt.query_map(params![id], |row| row.get::<_, i64>(0))?
            .collect::<SqlResult<Vec<_>>>()?;

        // 3. Delete task_screenshots links for these screenshots
        for ss_id in &screenshot_ids {
            conn.execute(
                "DELETE FROM task_screenshots WHERE screenshot_id = ?1",
                params![ss_id],
            )?;
        }

        // 4. Delete orphaned tasks (tasks with no remaining screenshot links)
        conn.execute(
            "DELETE FROM tasks WHERE id NOT IN (SELECT DISTINCT task_id FROM task_screenshots)",
            [],
        )?;

        // 5. Delete screenshots
        conn.execute(
            "DELETE FROM screenshots WHERE session_id = ?1",
            params![id],
        )?;

        // 6. Delete the session
        conn.execute(
            "DELETE FROM capture_sessions WHERE id = ?1",
            params![id],
        )?;

        Ok(paths)
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
                    cs.description, cs.title,
                    (SELECT COUNT(*) FROM screenshots s2
                     WHERE s2.session_id = cs.id
                     AND s2.id NOT IN (SELECT ts.screenshot_id FROM task_screenshots ts)
                    ) as unanalyzed_count
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
                title: row.get(5)?,
                unanalyzed_count: row.get(6)?,
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
                    cs.description, cs.title,
                    (SELECT COUNT(*) FROM screenshots s2
                     WHERE s2.session_id = cs.id
                     AND s2.id NOT IN (SELECT ts.screenshot_id FROM task_screenshots ts)
                    ) as unanalyzed_count
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
                    title: row.get(5)?,
                    unanalyzed_count: row.get(6)?,
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
            "SELECT id, filepath, captured_at, active_window_title, monitor_index, capture_group
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
                capture_group: row.get(5)?,
            })
        })?
        .collect::<SqlResult<Vec<_>>>()?;
        Ok(screenshots)
    }

    /// Get sessions that are ended and still have unanalyzed screenshots.
    pub fn get_pending_sessions(&self, limit: i64, offset: i64) -> SqlResult<Vec<CaptureSession>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT cs.id, cs.started_at, cs.ended_at,
                    (SELECT COUNT(*) FROM screenshots s WHERE s.session_id = cs.id) as screenshot_count,
                    cs.description, cs.title,
                    (SELECT COUNT(*) FROM screenshots s2
                     WHERE s2.session_id = cs.id
                     AND s2.id NOT IN (SELECT ts.screenshot_id FROM task_screenshots ts)
                    ) as unanalyzed_count
             FROM capture_sessions cs
             WHERE cs.ended_at IS NOT NULL
             AND (SELECT COUNT(*) FROM screenshots s3
                  WHERE s3.session_id = cs.id
                  AND s3.id NOT IN (SELECT ts2.screenshot_id FROM task_screenshots ts2)
                 ) > 0
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
                title: row.get(5)?,
                unanalyzed_count: row.get(6)?,
            })
        })?
        .collect::<SqlResult<Vec<_>>>()?;
        Ok(sessions)
    }

    /// Get sessions that are ended, have screenshots, and all screenshots are analyzed.
    pub fn get_completed_sessions(&self, limit: i64, offset: i64) -> SqlResult<Vec<CaptureSession>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT cs.id, cs.started_at, cs.ended_at,
                    (SELECT COUNT(*) FROM screenshots s WHERE s.session_id = cs.id) as screenshot_count,
                    cs.description, cs.title,
                    (SELECT COUNT(*) FROM screenshots s2
                     WHERE s2.session_id = cs.id
                     AND s2.id NOT IN (SELECT ts.screenshot_id FROM task_screenshots ts)
                    ) as unanalyzed_count
             FROM capture_sessions cs
             WHERE cs.ended_at IS NOT NULL
             AND (SELECT COUNT(*) FROM screenshots s3 WHERE s3.session_id = cs.id) > 0
             AND (SELECT COUNT(*) FROM screenshots s4
                  WHERE s4.session_id = cs.id
                  AND s4.id NOT IN (SELECT ts2.screenshot_id FROM task_screenshots ts2)
                 ) = 0
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
                title: row.get(5)?,
                unanalyzed_count: row.get(6)?,
            })
        })?
        .collect::<SqlResult<Vec<_>>>()?;
        Ok(sessions)
    }

    /// Get unanalyzed screenshots for a specific session.
    pub fn get_unanalyzed_screenshots_for_session(&self, session_id: i64, limit: i64) -> SqlResult<Vec<Screenshot>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT s.id, s.filepath, s.captured_at, s.active_window_title, s.monitor_index, s.capture_group
             FROM screenshots s
             LEFT JOIN task_screenshots ts ON s.id = ts.screenshot_id
             WHERE ts.task_id IS NULL
             AND s.session_id = ?1
             ORDER BY s.captured_at ASC
             LIMIT ?2",
        )?;
        let screenshots = stmt.query_map(params![session_id, limit], |row| {
            Ok(Screenshot {
                id: row.get(0)?,
                filepath: row.get(1)?,
                captured_at: row.get(2)?,
                active_window_title: row.get(3)?,
                monitor_index: row.get(4)?,
                capture_group: row.get(5)?,
            })
        })?
        .collect::<SqlResult<Vec<_>>>()?;
        Ok(screenshots)
    }

    /// Get the task linked to a specific screenshot, if any.
    pub fn get_task_for_screenshot(&self, screenshot_id: i64) -> SqlResult<Option<Task>> {
        let conn = self.conn()?;
        let result = conn.query_row(
            "SELECT t.id, t.title, t.description, t.category, t.started_at, t.ended_at,
                    t.ai_reasoning, t.user_verified, t.metadata
             FROM tasks t
             INNER JOIN task_screenshots ts ON t.id = ts.task_id
             WHERE ts.screenshot_id = ?1
             LIMIT 1",
            params![screenshot_id],
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
        );
        match result {
            Ok(task) => Ok(Some(task)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Get all tasks linked to screenshots in a given session, in chronological order.
    pub fn get_session_tasks(&self, session_id: i64) -> SqlResult<Vec<Task>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT t.id, t.title, t.description, t.category, t.started_at, t.ended_at,
                    t.ai_reasoning, t.user_verified, t.metadata
             FROM tasks t
             INNER JOIN task_screenshots ts ON t.id = ts.task_id
             INNER JOIN screenshots s ON ts.screenshot_id = s.id
             WHERE s.session_id = ?1
             ORDER BY t.started_at ASC",
        )?;
        let tasks = stmt.query_map(params![session_id], |row| {
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

    /// Get the most recent tasks linked to screenshots in a given session.
    /// Returns up to `limit` tasks, ordered most-recent first.
    pub fn get_recent_tasks_for_session(&self, session_id: i64, limit: i64) -> SqlResult<Vec<Task>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT t.id, t.title, t.description, t.category, t.started_at, t.ended_at,
                    t.ai_reasoning, t.user_verified, t.metadata
             FROM tasks t
             INNER JOIN task_screenshots ts ON t.id = ts.task_id
             INNER JOIN screenshots s ON ts.screenshot_id = s.id
             WHERE s.session_id = ?1
             ORDER BY t.started_at DESC
             LIMIT ?2",
        )?;
        let tasks = stmt.query_map(params![session_id, limit], |row| {
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

    /// Get all screenshots from a single capture group (same tick).
    #[cfg(test)]
    pub fn get_capture_group(&self, capture_group: &str) -> SqlResult<Vec<Screenshot>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, filepath, captured_at, active_window_title, monitor_index, capture_group
             FROM screenshots
             WHERE capture_group = ?1
             ORDER BY monitor_index ASC",
        )?;
        let screenshots = stmt.query_map(params![capture_group], |row| {
            Ok(Screenshot {
                id: row.get(0)?,
                filepath: row.get(1)?,
                captured_at: row.get(2)?,
                active_window_title: row.get(3)?,
                monitor_index: row.get(4)?,
                capture_group: row.get(5)?,
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
        let ss_id = db.insert_screenshot("test.png", "2025-01-01T00:00:00", Some("Terminal"), 0, None, None).unwrap();
        db.link_screenshot_to_task(task_id, ss_id).unwrap();
        // Linking again should not fail (OR IGNORE)
        db.link_screenshot_to_task(task_id, ss_id).unwrap();
    }

    #[test]
    fn test_delete_unanalyzed_screenshots() {
        let db = Database::in_memory().unwrap();
        let ss1 = db.insert_screenshot("shot1.webp", "2025-01-01T00:00:00", None, 0, None, None).unwrap();
        let _ss2 = db.insert_screenshot("shot2.webp", "2025-01-01T00:00:01", None, 0, None, None).unwrap();
        let ss3 = db.insert_screenshot("shot3.webp", "2025-01-01T00:00:02", None, 0, None, None).unwrap();

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
        let id = db.insert_screenshot("test.webp", "2025-01-01T00:00:00", Some("Terminal"), 0, None, None).unwrap();
        let screenshot = db.get_screenshot(id).unwrap();
        assert_eq!(screenshot.filepath, "test.webp");
        assert_eq!(screenshot.captured_at, "2025-01-01T00:00:00");
        assert_eq!(screenshot.active_window_title, Some("Terminal".to_string()));
        assert_eq!(screenshot.monitor_index, 0);
    }

    #[test]
    fn test_get_unanalyzed_screenshots() {
        let db = Database::in_memory().unwrap();
        let ss1 = db.insert_screenshot("shot1.webp", "2025-01-01T00:00:00", None, 0, None, None).unwrap();
        let _ss2 = db.insert_screenshot("shot2.webp", "2025-01-01T00:00:01", None, 0, None, None).unwrap();
        let _ss3 = db.insert_screenshot("shot3.webp", "2025-01-01T00:00:02", None, 0, None, None).unwrap();

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
        db.insert_screenshot("shot1.webp", "2025-01-01T00:00:00", None, 0, None, None).unwrap();
        db.insert_screenshot("shot2.webp", "2025-01-01T00:00:01", Some("Browser"), 0, None, None).unwrap();
        db.insert_screenshot("shot3.webp", "2025-01-01T00:00:02", Some("Editor"), 1, None, None).unwrap();

        // Count should be 3
        assert_eq!(db.get_screenshot_count().unwrap(), 3);
    }

    #[test]
    fn test_create_and_end_session() {
        let db = Database::in_memory().unwrap();
        let id = db.create_session("2025-01-01T10:00:00", None, None).unwrap();
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
        let session_id = db.create_session("2025-01-01T10:00:00", None, None).unwrap();

        db.insert_screenshot("s1.webp", "2025-01-01T10:00:00", None, 0, Some(session_id), None).unwrap();
        db.insert_screenshot("s2.webp", "2025-01-01T10:00:30", None, 0, Some(session_id), None).unwrap();
        db.insert_screenshot("s3.webp", "2025-01-01T10:01:00", None, 0, None, None).unwrap(); // no session

        let sessions = db.get_sessions(10, 0).unwrap();
        assert_eq!(sessions[0].screenshot_count, 2);
    }

    #[test]
    fn test_get_session_screenshots() {
        let db = Database::in_memory().unwrap();
        let session_id = db.create_session("2025-01-01T10:00:00", None, None).unwrap();

        db.insert_screenshot("s1.webp", "2025-01-01T10:00:00", None, 0, Some(session_id), None).unwrap();
        db.insert_screenshot("s2.webp", "2025-01-01T10:00:30", Some("Editor"), 0, Some(session_id), None).unwrap();
        db.insert_screenshot("other.webp", "2025-01-01T10:01:00", None, 0, None, None).unwrap();

        let screenshots = db.get_session_screenshots(session_id).unwrap();
        assert_eq!(screenshots.len(), 2);
        assert_eq!(screenshots[0].filepath, "s1.webp");
        assert_eq!(screenshots[1].filepath, "s2.webp");
    }

    #[test]
    fn test_session_description() {
        let db = Database::in_memory().unwrap();
        let id = db.create_session("2025-01-01T10:00:00", Some("Building a React form"), Some("React work")).unwrap();
        let session = db.get_session(id).unwrap();
        assert_eq!(session.description, Some("Building a React form".to_string()));
        assert_eq!(session.title, Some("React work".to_string()));

        // Session without description or title
        let id2 = db.create_session("2025-01-01T11:00:00", None, None).unwrap();
        let session2 = db.get_session(id2).unwrap();
        assert_eq!(session2.description, None);
        assert_eq!(session2.title, None);
    }

    #[test]
    fn test_get_screenshot_session_id() {
        let db = Database::in_memory().unwrap();
        let session_id = db.create_session("2025-01-01T10:00:00", None, None).unwrap();
        let ss_id = db.insert_screenshot("s1.webp", "2025-01-01T10:00:00", None, 0, Some(session_id), None).unwrap();
        let ss_no_session = db.insert_screenshot("s2.webp", "2025-01-01T10:00:01", None, 0, None, None).unwrap();

        assert_eq!(db.get_screenshot_session_id(ss_id).unwrap(), Some(session_id));
        assert_eq!(db.get_screenshot_session_id(ss_no_session).unwrap(), None);
    }

    #[test]
    fn test_get_sessions_pagination() {
        let db = Database::in_memory().unwrap();
        for i in 0..5 {
            db.create_session(&format!("2025-01-0{}T10:00:00", i + 1), None, None).unwrap();
        }
        let page1 = db.get_sessions(2, 0).unwrap();
        assert_eq!(page1.len(), 2);
        let page2 = db.get_sessions(2, 2).unwrap();
        assert_eq!(page2.len(), 2);
        let page3 = db.get_sessions(2, 4).unwrap();
        assert_eq!(page3.len(), 1);
    }

    #[test]
    fn test_unanalyzed_count() {
        let db = Database::in_memory().unwrap();
        let session_id = db.create_session("2025-01-01T10:00:00", None, None).unwrap();
        let ss1 = db.insert_screenshot("s1.webp", "2025-01-01T10:00:00", None, 0, Some(session_id), None).unwrap();
        let _ss2 = db.insert_screenshot("s2.webp", "2025-01-01T10:00:30", None, 0, Some(session_id), None).unwrap();

        // Both unanalyzed
        let session = db.get_session(session_id).unwrap();
        assert_eq!(session.unanalyzed_count, 2);

        // Link one
        let task_id = db.insert_task("Task", "2025-01-01T10:00:00").unwrap();
        db.link_screenshot_to_task(task_id, ss1).unwrap();

        let session = db.get_session(session_id).unwrap();
        assert_eq!(session.unanalyzed_count, 1);
    }

    #[test]
    fn test_get_pending_sessions() {
        let db = Database::in_memory().unwrap();

        // Session 1: ended, has unanalyzed screenshots -> pending
        let s1 = db.create_session("2025-01-01T10:00:00", None, Some("Pending session")).unwrap();
        db.end_session(s1, "2025-01-01T10:30:00").unwrap();
        db.insert_screenshot("s1.webp", "2025-01-01T10:00:00", None, 0, Some(s1), None).unwrap();

        // Session 2: ended, all screenshots analyzed -> completed, not pending
        let s2 = db.create_session("2025-01-01T11:00:00", None, Some("Completed session")).unwrap();
        db.end_session(s2, "2025-01-01T11:30:00").unwrap();
        let ss2 = db.insert_screenshot("s2.webp", "2025-01-01T11:00:00", None, 0, Some(s2), None).unwrap();
        let task_id = db.insert_task("Task", "2025-01-01T11:00:00").unwrap();
        db.link_screenshot_to_task(task_id, ss2).unwrap();

        // Session 3: not ended -> not pending
        let s3 = db.create_session("2025-01-01T12:00:00", None, Some("Active session")).unwrap();
        db.insert_screenshot("s3.webp", "2025-01-01T12:00:00", None, 0, Some(s3), None).unwrap();

        let pending = db.get_pending_sessions(10, 0).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, s1);
        assert_eq!(pending[0].title, Some("Pending session".to_string()));
    }

    #[test]
    fn test_get_completed_sessions() {
        let db = Database::in_memory().unwrap();

        // Session 1: ended, has unanalyzed screenshots -> not completed
        let s1 = db.create_session("2025-01-01T10:00:00", None, Some("Pending")).unwrap();
        db.end_session(s1, "2025-01-01T10:30:00").unwrap();
        db.insert_screenshot("s1.webp", "2025-01-01T10:00:00", None, 0, Some(s1), None).unwrap();

        // Session 2: ended, all screenshots analyzed -> completed
        let s2 = db.create_session("2025-01-01T11:00:00", None, Some("Done")).unwrap();
        db.end_session(s2, "2025-01-01T11:30:00").unwrap();
        let ss2 = db.insert_screenshot("s2.webp", "2025-01-01T11:00:00", None, 0, Some(s2), None).unwrap();
        let task_id = db.insert_task("Task", "2025-01-01T11:00:00").unwrap();
        db.link_screenshot_to_task(task_id, ss2).unwrap();

        // Session 3: ended, no screenshots -> not completed (no screenshots)
        let s3 = db.create_session("2025-01-01T12:00:00", None, Some("Empty")).unwrap();
        db.end_session(s3, "2025-01-01T12:30:00").unwrap();

        let completed = db.get_completed_sessions(10, 0).unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].id, s2);
        assert_eq!(completed[0].title, Some("Done".to_string()));
    }

    #[test]
    fn test_get_task_for_screenshot() {
        let db = Database::in_memory().unwrap();
        let ss_id = db.insert_screenshot("shot1.webp", "2025-01-01T00:00:00", None, 0, None, None).unwrap();
        let ss_no_task = db.insert_screenshot("shot2.webp", "2025-01-01T00:00:01", None, 0, None, None).unwrap();

        // No task linked yet
        assert!(db.get_task_for_screenshot(ss_id).unwrap().is_none());

        // Link a task
        let task_id = db.insert_full_task("Coding", "Writing Rust", "coding", "2025-01-01T00:00:00", "IDE open").unwrap();
        db.link_screenshot_to_task(task_id, ss_id).unwrap();

        // Should return the linked task
        let task = db.get_task_for_screenshot(ss_id).unwrap().unwrap();
        assert_eq!(task.id, task_id);
        assert_eq!(task.title, "Coding");
        assert_eq!(task.category, Some("coding".to_string()));

        // Screenshot without a task should return None
        assert!(db.get_task_for_screenshot(ss_no_task).unwrap().is_none());
    }

    #[test]
    fn test_delete_session() {
        let db = Database::in_memory().unwrap();

        // Create two sessions
        let s1 = db.create_session("2025-01-01T10:00:00", Some("Session 1"), None).unwrap();
        let s2 = db.create_session("2025-01-01T11:00:00", Some("Session 2"), None).unwrap();

        // Add screenshots to both
        let ss1 = db.insert_screenshot("s1.webp", "2025-01-01T10:00:00", None, 0, Some(s1), None).unwrap();
        let ss2 = db.insert_screenshot("s2.webp", "2025-01-01T10:00:30", None, 0, Some(s1), None).unwrap();
        let ss3 = db.insert_screenshot("s3.webp", "2025-01-01T11:00:00", None, 0, Some(s2), None).unwrap();

        // Create tasks linked to screenshots
        let t1 = db.insert_full_task("Task A", "Only in s1", "coding", "2025-01-01T10:00:00", "reason").unwrap();
        db.link_screenshot_to_task(t1, ss1).unwrap();

        let t2 = db.insert_full_task("Task B", "In both sessions", "coding", "2025-01-01T10:00:30", "reason").unwrap();
        db.link_screenshot_to_task(t2, ss2).unwrap();
        db.link_screenshot_to_task(t2, ss3).unwrap(); // shared with s2

        // Delete session 1
        let deleted_paths = db.delete_session(s1).unwrap();
        assert_eq!(deleted_paths.len(), 2);

        // Session 1 should be gone
        assert!(db.get_session(s1).is_err());
        assert_eq!(db.get_session_screenshots(s1).unwrap().len(), 0);

        // Task A should be deleted (orphaned), Task B should survive (still linked to ss3)
        assert!(db.get_task(t1).is_err());
        assert!(db.get_task(t2).is_ok());

        // Session 2 should be intact
        let s2_screenshots = db.get_session_screenshots(s2).unwrap();
        assert_eq!(s2_screenshots.len(), 1);
        assert_eq!(s2_screenshots[0].filepath, "s3.webp");
    }

    #[test]
    fn test_get_recent_tasks_for_session() {
        let db = Database::in_memory().unwrap();
        let s1 = db.create_session("2025-01-01T10:00:00", None, None).unwrap();
        let s2 = db.create_session("2025-01-01T11:00:00", None, None).unwrap();

        // Create screenshots in session 1
        let ss1 = db.insert_screenshot("s1.webp", "2025-01-01T10:00:00", None, 0, Some(s1), None).unwrap();
        let ss2 = db.insert_screenshot("s2.webp", "2025-01-01T10:00:30", None, 0, Some(s1), None).unwrap();
        let ss3 = db.insert_screenshot("s3.webp", "2025-01-01T10:01:00", None, 0, Some(s1), None).unwrap();
        // Screenshot in session 2
        let ss4 = db.insert_screenshot("s4.webp", "2025-01-01T11:00:00", None, 0, Some(s2), None).unwrap();

        // Create tasks and link to screenshots
        let t1 = db.insert_full_task("Task A", "First task", "coding", "2025-01-01T10:00:00", "reason").unwrap();
        db.link_screenshot_to_task(t1, ss1).unwrap();

        let t2 = db.insert_full_task("Task B", "Second task", "browsing", "2025-01-01T10:00:30", "reason").unwrap();
        db.link_screenshot_to_task(t2, ss2).unwrap();
        db.link_screenshot_to_task(t2, ss3).unwrap();

        let t3 = db.insert_full_task("Task C", "Other session", "writing", "2025-01-01T11:00:00", "reason").unwrap();
        db.link_screenshot_to_task(t3, ss4).unwrap();

        // Get recent tasks for session 1 — should return Task B, Task A (most recent first)
        let tasks = db.get_recent_tasks_for_session(s1, 2).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].title, "Task B");
        assert_eq!(tasks[1].title, "Task A");

        // Limit 1 — only most recent
        let tasks = db.get_recent_tasks_for_session(s1, 1).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Task B");

        // Session 2 — only Task C
        let tasks = db.get_recent_tasks_for_session(s2, 2).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Task C");

        // Non-existent session — empty
        let tasks = db.get_recent_tasks_for_session(999, 2).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_get_unanalyzed_screenshots_for_session() {
        let db = Database::in_memory().unwrap();
        let s1 = db.create_session("2025-01-01T10:00:00", None, None).unwrap();
        let s2 = db.create_session("2025-01-01T11:00:00", None, None).unwrap();

        let ss1 = db.insert_screenshot("s1.webp", "2025-01-01T10:00:00", None, 0, Some(s1), None).unwrap();
        db.insert_screenshot("s2.webp", "2025-01-01T10:00:30", None, 0, Some(s1), None).unwrap();
        db.insert_screenshot("s3.webp", "2025-01-01T11:00:00", None, 0, Some(s2), None).unwrap();

        // Link ss1 to a task
        let task_id = db.insert_task("Task", "2025-01-01T10:00:00").unwrap();
        db.link_screenshot_to_task(task_id, ss1).unwrap();

        // Session 1 should have 1 unanalyzed (s2)
        let unanalyzed = db.get_unanalyzed_screenshots_for_session(s1, 10).unwrap();
        assert_eq!(unanalyzed.len(), 1);
        assert_eq!(unanalyzed[0].filepath, "s2.webp");

        // Session 2 should have 1 unanalyzed (s3)
        let unanalyzed2 = db.get_unanalyzed_screenshots_for_session(s2, 10).unwrap();
        assert_eq!(unanalyzed2.len(), 1);
        assert_eq!(unanalyzed2[0].filepath, "s3.webp");
    }

    #[test]
    fn test_capture_group() {
        let db = Database::in_memory().unwrap();
        let session = db.create_session("2025-01-01T10:00:00", None, None).unwrap();

        // Insert screenshots in the same capture group (simulating multi-monitor)
        let group = "2025-01-01T10-00-00";
        db.insert_screenshot("mon1.webp", "2025-01-01T10:00:00", None, 1, Some(session), Some(group)).unwrap();
        db.insert_screenshot("mon2.webp", "2025-01-01T10:00:00", None, 2, Some(session), Some(group)).unwrap();
        // Screenshot with no group (legacy)
        db.insert_screenshot("legacy.webp", "2025-01-01T10:00:01", None, 0, Some(session), None).unwrap();

        let grouped = db.get_capture_group(group).unwrap();
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0].monitor_index, 1);
        assert_eq!(grouped[1].monitor_index, 2);
        assert_eq!(grouped[0].capture_group, Some(group.to_string()));

        // Legacy screenshot should not appear in group query
        let all = db.get_session_screenshots(session).unwrap();
        assert_eq!(all.len(), 3);
    }
}
