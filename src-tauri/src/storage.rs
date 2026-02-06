use crate::models::{Task, TaskUpdate};
use rusqlite::{params, Connection, Result as SqlResult};
use std::path::Path;
use std::sync::Mutex;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
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
        let conn = self.conn.lock().unwrap();
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
            );",
        )?;
        Ok(())
    }

    pub fn insert_screenshot(&self, filepath: &str, captured_at: &str, window_title: Option<&str>, monitor: i32) -> SqlResult<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO screenshots (filepath, captured_at, active_window_title, monitor_index) VALUES (?1, ?2, ?3, ?4)",
            params![filepath, captured_at, window_title, monitor],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_tasks(&self, limit: i64, offset: i64) -> SqlResult<Vec<Task>> {
        let conn = self.conn.lock().unwrap();
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
        let conn = self.conn.lock().unwrap();
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
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO tasks (title, started_at) VALUES (?1, ?2)",
            params![title, started_at],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_task(&self, id: i64, update: &TaskUpdate) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
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
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn link_screenshot_to_task(&self, task_id: i64, screenshot_id: i64) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO task_screenshots (task_id, screenshot_id) VALUES (?1, ?2)",
            params![task_id, screenshot_id],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> SqlResult<Option<String>> {
        let conn = self.conn.lock().unwrap();
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
        let conn = self.conn.lock().unwrap();
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
        let ss_id = db.insert_screenshot("test.png", "2025-01-01T00:00:00", Some("Terminal"), 0).unwrap();
        db.link_screenshot_to_task(task_id, ss_id).unwrap();
        // Linking again should not fail (OR IGNORE)
        db.link_screenshot_to_task(task_id, ss_id).unwrap();
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
}
