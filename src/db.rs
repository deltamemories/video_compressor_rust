use rusqlite::{Connection, TransactionBehavior};
use rusqlite_migration::{M, Migrations};
use std::error::Error;
use std::path::{Path};

pub struct Db {
    conn: Connection,
}

pub struct FileInfo {
    pub path: String,
    pub hash: String,
    pub size: i64,
}

#[derive(Debug)]
pub enum AddFileError {
    AlreadyExists,
    DatabaseError(rusqlite::Error),
}

impl From<rusqlite::Error> for AddFileError {
    fn from(err: rusqlite::Error) -> Self {
        Self::DatabaseError(err)
    }
}

impl Db {
    pub fn open(db_path: &Path) -> Result<Self, Box<dyn Error>> {
        let conn = Connection::open(db_path)?;

        conn.pragma_update(None,"foreign_keys","ON")?;
        conn.pragma_update(None,"busy_timeout", 5000)?;

        let mut db = Db { conn };
        db.init()?;
        Ok(db)
    }

    fn init(&mut self) -> rusqlite_migration::Result<()> {
        let mig = Migrations::new(vec![M::up(
            r#"
                CREATE TABLE files (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content_hash TEXT NOT NULL,
                    file_path BLOB NOT NULL,
                    file_size INTEGER NOT NULL
                );

                CREATE TABLE compression_jobs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    source_file_id INTEGER NOT NULL,
                    status TEXT NOT NULL CHECK(status IN ('processing', 'completed', 'failed', 'aborted')),
                    start_time DATETIME NOT NULL,

                    FOREIGN KEY(source_file_id) REFERENCES files(id) ON DELETE RESTRICT
                );

                CREATE TABLE job_successes (
                    job_id INTEGER PRIMARY KEY,
                    output_file_id INTEGER NOT NULL,
                    size_before INTEGER NOT NULL,
                    size_after INTEGER NOT NULL,
                    elapsed_time INTEGER NOT NULL,

                    FOREIGN KEY(job_id) REFERENCES compression_jobs(id) ON DELETE CASCADE,
                    FOREIGN KEY(output_file_id) REFERENCES files(id) ON DELETE CASCADE
                );

                CREATE TABLE job_failures (
                    job_id INTEGER PRIMARY KEY,
                    error_message TEXT NOT NULL,
                    elapsed_time INTEGER NOT NULL,

                    FOREIGN KEY(job_id) REFERENCES compression_jobs(id) ON DELETE CASCADE
                );

                CREATE UNIQUE INDEX idx_files_file_path_and_hash ON files(content_hash, file_path);

                CREATE INDEX idx_jobs_status ON compression_jobs(status);
                "#,
        )]);

        mig.to_latest(&mut self.conn)?;
        Ok(())
    }

    pub fn add_file(&mut self, file: FileInfo) -> Result<i64, AddFileError> {
        let rows_affected = self.conn.execute(
            r#"
            INSERT INTO files (content_hash, file_path, file_size)
            VALUES (?1, ?2, ?3)
            ON CONFLICT (content_hash, file_path) DO NOTHING
            "#,
            rusqlite::params![file.hash, file.path, file.size]
        )?;

        if rows_affected == 0 {
            return Err(AddFileError::AlreadyExists)
        }

        Ok(self.conn.last_insert_rowid())
    }

    pub fn start_job(&mut self, source_file_id: i64) -> Result<i64, Box<dyn Error>> {
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let job_id = {
            let mut stmt = tx.prepare(
                r#"
                INSERT INTO compression_jobs (source_file_id, status, start_time)
                VALUES (?1, 'processing', datetime('now'))
                "#
            )?;
            stmt.execute(rusqlite::params![source_file_id])?;
            tx.last_insert_rowid()
        };

        tx.commit()?;
        Ok(job_id)
    }

    pub fn complete_job(
        &mut self,
        job_id: i64,
        output_file_id: i64,
        size_before: i64,
        size_after: i64,
        elapsed_time: i64,
    ) -> Result<(), Box<dyn Error>> {
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        {
            let mut update_job = tx.prepare(
                r#"UPDATE compression_jobs SET status = 'completed' WHERE id = ?1"#
            )?;
            update_job.execute(rusqlite::params![job_id])?;

            let mut insert_suc = tx.prepare(
                r#"
                INSERT INTO job_successes (job_id, output_file_id, size_before, size_after, elapsed_time)
                VALUES (?1, ?2, ?3, ?4, ?5)
                "#
            )?;
            insert_suc.execute(rusqlite::params![job_id, output_file_id, size_before, size_after, elapsed_time])?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn fail_job(
        &mut self,
        job_id: i64,
        error_message: &str,
        elapsed_time: i64,
    ) -> Result<(), Box<dyn Error>> {
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        {
            let mut update_job = tx.prepare(
                r#"UPDATE compression_jobs SET status = 'failed' WHERE id = ?1 "#
            )?;
            update_job.execute(rusqlite::params![job_id])?;

            let mut insert_failure = tx.prepare(
                r#"
                INSERT INTO job_failures (job_id, error_message, elapsed_time)
                VALUES (?1, ?2, ?3)
                "#
            )?;
            insert_failure.execute(rusqlite::params![job_id, error_message, elapsed_time])?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn is_compressed_output(&self, hash: &str) -> Result<bool, Box<dyn Error>> {
        Ok(self.conn.query_row(
            r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM job_successes js
                    JOIN files f ON f.id = js.output_file_id
                    WHERE f.content_hash = ?1
                )
            "#,
            rusqlite::params![hash],
            |row| row.get(0),
        )?)
    }
}
