use rusqlite::{params, OptionalExtension};
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::Processing => "processing",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
        }
    }
}

pub struct Db {
    conn: rusqlite::Connection,
}

impl Db {
    pub fn open(db_path: &Path) -> rusqlite::Result<Self> {
        let conn = rusqlite::Connection::open(db_path)?;
        let db = Db { conn };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> rusqlite::Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS tasks (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    file_path TEXT NOT NULL UNIQUE,
                    status TEXT NOT NULL,
                    original_size INTEGER,
                    compressed_size INTEGER,
                    error_message TEXT,
                    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );",
            [],
        )?;

        Ok(())
    }

    pub fn register_files(&self, files: &[PathBuf]) -> rusqlite::Result<Vec<PathBuf>> {
        let mut added_files: Vec<PathBuf> = Vec::new();

        let tx = self.conn.unchecked_transaction()?;

        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO tasks (file_path, status)
                VALUES (?1, 'pending')
                RETURNING id;",
        )?;

        for file in files {
            if let Some(path_str) = file.to_str() {
                let normalized_path = path_str.replace("\\", "/");

                let task_id: Option<i64> = stmt
                    .query_row(params![normalized_path], |row| row.get(0))
                    .optional()?;

                if let Some(_) = task_id {
                    added_files.push(file.clone());
                }
            }
        }

        drop(stmt);
        tx.commit()?;
        Ok(added_files)
    }
    
    pub fn set_as_processing(&self, file_path: &Path, original_size: Option<u64>) -> rusqlite::Result<()> {
        self.update(file_path, TaskStatus::Processing, original_size, None, None)
    }
    
    pub fn set_as_completed(&self, file_path: &Path, original_size: Option<u64>, compressed_size: Option<u64>) -> rusqlite::Result<()> {
        self.update(file_path, TaskStatus::Completed, original_size, compressed_size, None)
    }
    
    pub fn set_as_failed(&self, file_path: &Path, error_msg: Option<&str>) -> rusqlite::Result<()> {
        self.update(file_path, TaskStatus::Failed, None, None, error_msg)
    }

    fn update(
        &self,
        file_path: &Path,
        status: TaskStatus,
        original_size: Option<u64>,
        compressed_size: Option<u64>,
        error_message: Option<&str>,
    ) -> rusqlite::Result<()> {
        let path_str = file_path
            .as_os_str()
            .to_str()
            .ok_or_else(|| {
                rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid file path",
                )))
            })?
            .replace("\\", "/");

        self.conn.execute(
            "UPDATE tasks
            SET status = ?1,
            original_size = ?2,
            compressed_size = ?3,
            error_message = ?4,
            updated_at = CURRENT_TIMESTAMP
            WHERE file_path = ?5;",
            params![
                status.as_str(),
                original_size.map(|v| v as i64),
                compressed_size.map(|v| v as i64),
                error_message,
                path_str
            ],
        )?;
        Ok(())
    }
}
