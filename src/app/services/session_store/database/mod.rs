//! Local-only transactional session storage. All access belongs to one worker.
mod content;
#[cfg(test)]
mod fault_tests;
mod persist;
mod quarantine;
#[cfg(test)]
mod quarantine_tests;
mod read;
#[cfg(test)]
mod tests;
mod worker;

use super::model::SessionManifest;
use super::types::SessionPersistProfile;
pub(super) use read::ReadScope;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;
use turso::{Connection, Database as TursoDatabase};
pub(super) use worker::DatabaseWorker;

const APPLICATION_ID: i64 = 0x5350_4350;
const SCHEMA: &str = "
CREATE TABLE session_state(id INTEGER PRIMARY KEY CHECK(id=1), payload BLOB NOT NULL);
CREATE TABLE tabs(position INTEGER PRIMARY KEY, payload BLOB NOT NULL);
CREATE TABLE snapshots(id TEXT PRIMARY KEY, bytes INTEGER NOT NULL, digest BLOB NOT NULL);
CREATE TABLE chunks(id TEXT NOT NULL, part INTEGER NOT NULL, body BLOB NOT NULL,
                    PRIMARY KEY(id, part));";

pub(super) struct StoredSession {
    pub manifest: SessionManifest,
    pub contents: HashMap<String, String>,
    pub quarantined: HashSet<String>,
}
pub(super) struct Database {
    conn: Connection,
    database: TursoDatabase,
    _ownership: File,
    tab_cache: Vec<Vec<u8>>,
    state_cache: Vec<u8>,
    revisions: HashMap<String, crate::app::domain::buffer::SnapshotRevision>,
    latest_request: u64,
    pub(super) blocked: Option<String>,
    verified: bool,
    quarantined: HashMap<String, super::model::SessionBuffer>,
}
impl Database {
    async fn open(root: &Path) -> io::Result<Self> {
        create_private_directory(root)?;
        let ownership = private_file(&root.join("session.db.lock"))?;
        ownership.try_lock().map_err(error)?;
        let path = root.join("session.db");
        drop(private_file(&path)?);
        let path = path
            .to_str()
            .ok_or_else(|| invalid("session database path is not UTF-8"))?;
        let database = turso::Builder::new_local(path)
            .with_io(turso::IoBackend::Syscall)
            .build()
            .await
            .map_err(error)?;
        let conn = database.connect().map_err(error)?;
        conn.execute_batch("PRAGMA synchronous=FULL; PRAGMA cache_size=-8192;")
            .await
            .map_err(error)?;
        let mut db = Self {
            conn,
            database,
            _ownership: ownership,
            tab_cache: Vec::new(),
            state_cache: Vec::new(),
            revisions: HashMap::new(),
            latest_request: 0,
            blocked: None,
            verified: false,
            quarantined: HashMap::new(),
        };
        db.initialize().await?;
        db.load_quarantine().await?;
        Ok(db)
    }
    async fn initialize(&mut self) -> io::Result<()> {
        let version = self.integer("PRAGMA user_version").await?;
        let application = self.integer("PRAGMA application_id").await?;
        if application == APPLICATION_ID {
            match version {
                2 => return Ok(()),
                1 => {
                    return self
                        .create_schema(&format!("{} PRAGMA user_version=2;", quarantine::SCHEMA))
                        .await;
                }
                _ => {}
            }
        }
        if version != 0
            || application != 0
            || self
                .integer("SELECT count(*) FROM sqlite_master WHERE type='table'")
                .await?
                != 0
        {
            return Err(invalid(
                "unsupported session database schema; preserve it for recovery",
            ));
        }
        self.create_schema(&format!(
            "{SCHEMA} {} PRAGMA application_id={APPLICATION_ID}; PRAGMA user_version=2;",
            quarantine::SCHEMA
        ))
        .await
    }
    async fn create_schema(&self, sql: &str) -> io::Result<()> {
        self.conn
            .transactional_batch(
                sql.split(';')
                    .filter(|statement| !statement.trim().is_empty()),
                turso::transaction::TransactionBehavior::Immediate,
            )
            .await
            .map(|_| ())
            .map_err(error)
    }
    async fn import_legacy(&mut self, root: &Path, fallback: Option<&Path>) -> io::Result<()> {
        if self.integer("SELECT count(*) FROM session_state").await? != 0 {
            return Ok(());
        }
        // Empty schema after an interrupted import is retryable; orphan rows are not.
        self.load(ReadScope::Shells).await?;
        if let Some(request) = super::legacy::read(root, fallback)? {
            self.persist(request, 0).await?;
            self.load(ReadScope::All).await?;
            self.checkpoint().await?;
        }
        Ok(())
    }
    async fn integer(&self, sql: &str) -> io::Result<i64> {
        self.conn
            .query(sql, ())
            .await
            .map_err(error)?
            .next()
            .await
            .map_err(error)?
            .ok_or_else(|| invalid("missing scalar database result"))?
            .get(0)
            .map_err(error)
    }
    pub async fn checkpoint(&self) -> io::Result<()> {
        let mut rows = self
            .conn
            .query("PRAGMA wal_checkpoint(TRUNCATE)", ())
            .await
            .map_err(error)?;
        while rows.next().await.map_err(error)?.is_some() {}
        Ok(())
    }
    fn ensure_healthy(&self) -> io::Result<()> {
        match &self.blocked {
            Some(cause) => Err(invalid(format!(
                "session is protected after recovery failure: {cause}"
            ))),
            None => Ok(()),
        }
    }
    async fn execute_cached(&self, sql: &str, params: impl turso::IntoParams) -> io::Result<()> {
        self.conn
            .prepare_cached(sql)
            .await
            .map_err(error)?
            .execute(params)
            .await
            .map_err(error)?;
        Ok(())
    }
    async fn finish_transaction(&mut self, result: io::Result<()>) -> io::Result<()> {
        let result = match result {
            Ok(()) => self
                .conn
                .execute("COMMIT", ())
                .await
                .map(|_| ())
                .map_err(error),
            Err(cause) => Err(cause),
        };
        if result.is_err() {
            // A failed COMMIT can be ambiguous. Never trust pre-attempt caches.
            self.state_cache.clear();
            self.tab_cache.clear();
            self.revisions.clear();
            // SQLITE_FULL and rollback triggers can already end the transaction.
            if !self.conn.is_autocommit().unwrap_or(false) {
                let rollback = self
                    .conn
                    .execute("ROLLBACK", ())
                    .await
                    .map(|_| ())
                    .map_err(error);
                self.record_rollback_result(rollback);
            }
        }
        result
    }
    fn record_rollback_result(&mut self, result: io::Result<()>) {
        if let Err(cause) = result {
            self.blocked = Some(format!("rollback failed: {cause}"));
        }
    }
}
fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> io::Result<T> {
    serde_json::from_slice(bytes).map_err(invalid)
}
fn error(cause: impl std::fmt::Display) -> io::Error {
    io::Error::other(cause.to_string())
}
fn invalid(cause: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, cause.to_string())
}
fn create_private_directory(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
fn private_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
