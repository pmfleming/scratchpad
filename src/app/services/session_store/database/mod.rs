//! Local-only transactional session storage. All access belongs to one worker.
#[cfg(test)]
mod app_safety_tests;
mod chunking;
pub(super) mod closed;
mod content;
#[cfg(test)]
mod fault_tests;
mod history;
#[cfg(test)]
mod history_fingerprint_tests;
#[cfg(test)]
mod incremental_tests;
#[cfg(test)]
mod inventory_tests;
mod persist;
mod quarantine;
#[cfg(test)]
mod quarantine_tests;
#[cfg(test)]
mod range_tests;
mod read;
mod retention;
#[cfg(test)]
mod retention_policy_tests;
#[cfg(test)]
mod retention_tests;
#[cfg(test)]
mod tests;
pub(super) mod usage;
#[cfg(test)]
mod version_tests;
pub(super) mod versions;
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
CREATE TABLE session_state(id INTEGER PRIMARY KEY CHECK(id=1), payload BLOB NOT NULL) STRICT;
CREATE TABLE tabs(position INTEGER PRIMARY KEY, payload BLOB NOT NULL) STRICT;
CREATE TABLE buffer_ids(id TEXT PRIMARY KEY) STRICT;
CREATE TABLE history(temp_id TEXT NOT NULL REFERENCES buffer_ids(id) ON DELETE CASCADE,
                     seq INTEGER NOT NULL, global_seq INTEGER NOT NULL CHECK(global_seq>0),
                     entry BLOB NOT NULL, PRIMARY KEY(temp_id, seq)) STRICT;
CREATE INDEX history_global_seq ON history(global_seq);
CREATE TABLE snapshots(id TEXT PRIMARY KEY REFERENCES buffer_ids(id) ON DELETE CASCADE,
                       bytes INTEGER NOT NULL, digest BLOB NOT NULL) STRICT;
CREATE TABLE blobs(digest BLOB PRIMARY KEY, body BLOB NOT NULL) STRICT;
CREATE TABLE parts(temp_id TEXT NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
                   part INTEGER NOT NULL, digest BLOB NOT NULL REFERENCES blobs(digest),
                   PRIMARY KEY(temp_id, part)) STRICT;
CREATE INDEX parts_digest ON parts(digest);";

pub(super) struct StoredSession {
    pub manifest: SessionManifest,
    pub contents: HashMap<String, String>,
    pub quarantined: HashSet<String>,
}
#[derive(Default)]
struct ContentInventory {
    buffers: HashSet<String>,
    snapshots: HashSet<String>,
    full_cleanup_pending: bool,
    connection_changes: Option<i64>,
}

pub(super) struct Database {
    conn: Connection,
    database: TursoDatabase,
    _ownership: File,
    tab_cache: Vec<Vec<u8>>,
    state_cache: Vec<u8>,
    revisions: HashMap<String, crate::app::domain::buffer::SnapshotRevision>,
    chunk_cache: HashMap<String, chunking::ChunkCheckpoint>,
    history_cache: HashMap<String, Vec<history::HistoryFingerprint>>,
    // Complete committed IDs, including cold/quarantined content; revisions alone
    // only describe snapshots written by this process. None requires a fresh scan.
    inventory: Option<ContentInventory>,
    latest_request: u64,
    recovery_policy: Option<crate::app::services::settings_store::RecoverySettings>,
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
        conn.execute_batch(
            "PRAGMA synchronous=FULL; PRAGMA cache_size=-8192; PRAGMA foreign_keys=ON;",
        )
        .await
        .map_err(error)?;
        let mut db = Self {
            conn,
            database,
            _ownership: ownership,
            tab_cache: Vec::new(),
            state_cache: Vec::new(),
            revisions: HashMap::new(),
            chunk_cache: HashMap::new(),
            history_cache: HashMap::new(),
            inventory: None,
            latest_request: 0,
            recovery_policy: None,
            blocked: None,
            verified: false,
            quarantined: HashMap::new(),
        };
        db.initialize().await?;
        db.load_quarantine().await?;
        // Reserve sequences from cold tabs too, without reading their payloads.
        let maximum = db
            .integer("SELECT coalesce(max(global_seq),0) FROM history")
            .await?;
        crate::app::domain::buffer::register_text_history_global_seq(maximum as u64);
        Ok(db)
    }
    async fn initialize(&mut self) -> io::Result<()> {
        let version = self.integer("PRAGMA user_version").await?;
        let application = self.integer("PRAGMA application_id").await?;
        if application == APPLICATION_ID && version == 7 {
            return Ok(());
        }
        if application == APPLICATION_ID && version == 6 {
            return self.create_schema("ALTER TABLE buffer_versions ADD COLUMN source_token TEXT; CREATE UNIQUE INDEX version_source ON buffer_versions(temp_id, source_token); PRAGMA user_version=7;").await;
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
            "{SCHEMA} {} {} {} PRAGMA application_id={APPLICATION_ID}; PRAGMA user_version=7;",
            quarantine::SCHEMA,
            closed::SCHEMA,
            versions::SCHEMA
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
    // Map one row at a time: fingerprint callers release each undo payload here,
    // rather than materializing a second complete history before hashing it.
    async fn query_collect<T>(
        &self,
        sql: &str,
        params: impl turso::IntoParams,
        mut map: impl FnMut(&turso::Row) -> io::Result<T>,
    ) -> io::Result<Vec<T>> {
        let mut rows = self.conn.query(sql, params).await.map_err(error)?;
        let mut values = Vec::new();
        while let Some(row) = rows.next().await.map_err(error)? {
            values.push(map(&row)?);
        }
        Ok(values)
    }

    async fn collect_digests(
        &self,
        sql: &str,
        params: impl turso::IntoParams,
        into: &mut HashSet<Vec<u8>>,
    ) -> io::Result<()> {
        let mut rows = self.conn.query(sql, params).await.map_err(error)?;
        while let Some(row) = rows.next().await.map_err(error)? {
            into.insert(row.get(0).map_err(error)?);
        }
        Ok(())
    }

    async fn string_set(&self, sql: &str) -> io::Result<HashSet<String>> {
        let mut rows = self.conn.query(sql, ()).await.map_err(error)?;
        let mut values = HashSet::new();
        while let Some(row) = rows.next().await.map_err(error)? {
            values.insert(row.get(0).map_err(error)?);
        }
        Ok(values)
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
            self.chunk_cache.clear();
            self.history_cache.clear();
            self.inventory = None;
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
