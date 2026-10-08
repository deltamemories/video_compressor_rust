use rusqlite::types::ToSqlOutput;
use rusqlite::{OptionalExtension, ToSql, params};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl ToSql for TaskStatus {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_str()))
    }
}

pub enum HashType {
    OriginalHash,
    CompressedHash,
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
                    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                    original_hash TEXT,
                    compressed_hash TEXT
            );",
            [],
        )?;

        Ok(())
    }

    pub fn register_files(&self, files: &[PathBuf]) -> rusqlite::Result<Vec<PathBuf>> {
        let mut added_files: Vec<PathBuf> = Vec::new();
        let tx = self.conn.unchecked_transaction()?;

        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO tasks (file_path, status)
                VALUES (?1, 'pending')
                RETURNING id;",
            )?;

            for file in files {
                if let Ok(path_str) = normalize_path(file) {
                    let task_id: Option<i64> = stmt
                        .query_row(params![path_str], |row| row.get(0))
                        .optional()?;

                    if task_id.is_some() {
                        added_files.push(file.clone());
                    }
                }
            }
        }

        tx.commit()?;
        Ok(added_files)
    }

    pub fn set_as_processing(
        &self,
        file_path: &Path,
        original_size: u64,
        original_hash: &str,
    ) -> rusqlite::Result<()> {
        let ps = normalize_path(file_path)?;

        self.conn.execute(
            "UPDATE tasks SET status = 'processing', original_size = ?1, updated_at = CURRENT_TIMESTAMP, original_hash = ?2
            WHERE file_path = ?3;",
            params![original_size as i64, original_hash, ps],
        )?;
        Ok(())
    }

    pub fn set_as_completed(
        &self,
        file_path: &Path,
        original_size: u64,
        compressed_size: u64,
        compressed_hash: &str,
    ) -> rusqlite::Result<()> {
        let ps = normalize_path(file_path)?;
        self.conn.execute(
            "UPDATE tasks\
            SET status = 'completed', original_size = ?1, compressed_size = ?2, updated_at = CURRENT_TIMESTAMP, compressed_hash = ?3
            WHERE file_path = ?4;",
            params![original_size as i64, compressed_size as i64, compressed_hash, ps],
        )?;
        Ok(())
    }

    pub fn set_as_failed(&self, file_path: &Path, error_msg: &str) -> rusqlite::Result<()> {
        let ps = normalize_path(file_path)?;
        self.conn.execute(
            "UPDATE tasks\
            SET status = 'completed', original_size = ?1, compressed_size = ?2, updated_at = CURRENT_TIMESTAMP\
            WHERE file_path = ?3;",
            params![error_msg, ps],
        )?;
        Ok(())
    }

    pub fn get_pending(&self) -> rusqlite::Result<Vec<PathBuf>> {
        self.get_by_status(TaskStatus::Pending)
    }

    pub fn get_failed(&self) -> rusqlite::Result<Vec<PathBuf>> {
        self.get_by_status(TaskStatus::Failed)
    }

    pub fn get_processing(&self) -> rusqlite::Result<Vec<PathBuf>> {
        self.get_by_status(TaskStatus::Processing)
    }

    pub fn get_completed(&self) -> rusqlite::Result<Vec<PathBuf>> {
        self.get_by_status(TaskStatus::Completed)
    }

    fn get_by_status(&self, status: TaskStatus) -> rusqlite::Result<Vec<PathBuf>> {
        let s = status.as_str();

        let mut stmt = self
            .conn
            .prepare("SELECT file_path FROM tasks WHERE status = ?1;")?;

        let rows = stmt.query_map([s], |row| {
            let path_str: String = row.get(0)?;
            Ok(PathBuf::from(path_str))
        })?;

        rows.collect()
    }

    pub fn get_by_original_hash(&self, hash: &str) -> rusqlite::Result<Vec<PathBuf>> {
        self.get_by_hash(hash, HashType::OriginalHash)
    }

    pub fn get_by_compressed_hash(&self, hash: &str) -> rusqlite::Result<Vec<PathBuf>> {
        self.get_by_hash(hash, HashType::CompressedHash)
    }

    fn get_by_hash(&self, hash: &str, hash_type: HashType) -> rusqlite::Result<Vec<PathBuf>> {
        let hash_col = match hash_type {
            HashType::OriginalHash => "original_hash",
            HashType::CompressedHash => "compressed_hash",
        };

        let mut stmt = self.conn.prepare(&format!(
            "SELECT file_path FROM tasks WHERE {hash_col} == ?1"
        ))?;

        let rows = stmt.query_map([hash], |row| {
            let path_str: String = row.get(0)?;
            Ok(PathBuf::from(path_str))
        })?;

        rows.collect()
    }
}

fn normalize_path(path: &Path) -> rusqlite::Result<String> {
    path.to_str().map(|s| s.replace("\\", "/")).ok_or_else(|| {
        rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid file path",
        )))
    })
}
