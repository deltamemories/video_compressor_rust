use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};
use std::error::Error;
use std::path::Path;

pub struct Db {
    conn: Connection,
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
}
