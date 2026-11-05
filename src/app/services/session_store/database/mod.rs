//! Local-only transactional session storage. All access belongs to one worker.
mod activity;
#[cfg(test)]
mod activity_tests;
#[cfg(test)]
mod app_safety_tests;
mod chunking;
pub(super) mod closed;
#[cfg(test)]
mod closed_tests;
mod content;
mod documents;
#[cfg(test)]
mod documents_tests;
#[cfg(test)]
mod fault_tests;
mod history;
#[cfg(test)]
mod history_fingerprint_tests;
mod history_rows;
#[cfg(test)]
mod incremental_tests;
#[cfg(test)]
mod inventory_tests;
mod persist;
mod profiling;
pub use profiling::SessionMaintenanceFixture;
mod quarantine;
#[cfg(test)]
mod quarantine_tests;
#[cfg(test)]
mod range_tests;
mod read;
mod reset;
pub(super) use reset::start_new_session;
mod schema_reset;
#[cfg(test)]
mod schema_reset_tests;
#[cfg(test)]
mod size_tests;
#[cfg(test)]
mod tests;
mod timeline;
#[cfg(test)]
mod timeline_tests;
pub(super) mod usage;
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
CREATE TABLE documents(id TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT '', path TEXT, path_key TEXT, search_text TEXT NOT NULL DEFAULT '',
    payload BLOB NOT NULL DEFAULT x'7b7d', observed_digest TEXT, observed_missing INTEGER NOT NULL DEFAULT 0 CHECK(observed_missing IN (0,1))) STRICT;
CREATE INDEX documents_path ON documents(path_key);
CREATE TABLE activity(global_seq INTEGER PRIMARY KEY CHECK(global_seq>0), observed_at INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('created','opened','closed','disk_conflict','missing')),
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE, name TEXT NOT NULL, path TEXT, search_text TEXT NOT NULL DEFAULT '') STRICT;
CREATE INDEX activity_document_sequence ON activity(document_id,global_seq);
CREATE TABLE tab_documents(position INTEGER NOT NULL REFERENCES tabs(position) ON DELETE CASCADE,
    document_id TEXT NOT NULL REFERENCES documents(id), PRIMARY KEY(position,document_id)) STRICT;
CREATE TABLE history_nodes(temp_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    node_id INTEGER NOT NULL CHECK(node_id>=0), PRIMARY KEY(temp_id,node_id)) STRICT;
CREATE TABLE history(temp_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
                     seq INTEGER NOT NULL, parent_id INTEGER NOT NULL CHECK(parent_id>=0 AND parent_id<seq), edited_at INTEGER NOT NULL, global_seq INTEGER NOT NULL CHECK(global_seq>0),
                     source TEXT NOT NULL CHECK(source IN ('load','edit','paste','cut','search_replace','external')), summary TEXT NOT NULL, entry BLOB NOT NULL, bytes INTEGER NOT NULL DEFAULT 0 CHECK(bytes>=0), PRIMARY KEY(temp_id, seq),
                     FOREIGN KEY(temp_id,seq) REFERENCES history_nodes(temp_id,node_id) ON DELETE CASCADE,
                     FOREIGN KEY(temp_id,parent_id) REFERENCES history_nodes(temp_id,node_id) ON DELETE CASCADE) STRICT;
CREATE TABLE document_history(temp_id TEXT PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
    now_id INTEGER NOT NULL CHECK(now_id>=0), tip_id INTEGER NOT NULL CHECK(tip_id>=0), edited_at INTEGER NOT NULL, edit_seq INTEGER NOT NULL,
    FOREIGN KEY(temp_id,now_id) REFERENCES history_nodes(temp_id,node_id) ON DELETE CASCADE,
    FOREIGN KEY(temp_id,tip_id) REFERENCES history_nodes(temp_id,node_id) ON DELETE CASCADE) STRICT;
CREATE INDEX document_recency ON document_history(edit_seq DESC);
CREATE INDEX history_parent ON history(temp_id, parent_id);
CREATE INDEX history_global_seq ON history(global_seq,temp_id,seq);
CREATE INDEX history_document_sequence ON history(temp_id,global_seq,seq);
CREATE INDEX history_sizes ON history(temp_id, bytes);
CREATE TABLE snapshots(id TEXT PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
                       bytes INTEGER NOT NULL, digest BLOB NOT NULL) STRICT;
CREATE TABLE blobs(digest BLOB PRIMARY KEY, body BLOB NOT NULL, bytes INTEGER NOT NULL DEFAULT 0 CHECK(bytes>=0)) STRICT;
CREATE INDEX blob_sizes ON blobs(bytes, digest);
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
    ownership: File,
    startup_notices: Vec<super::RestoreStatus>,
    tab_cache: Vec<Vec<u8>>,
    state_cache: Vec<u8>,
    revisions: HashMap<String, crate::app::domain::buffer::SnapshotRevision>,
    chunk_cache: HashMap<String, chunking::ChunkCheckpoint>,
    history_cache: HashMap<String, Vec<history::HistoryFingerprint>>,
    document_cache: HashMap<String, Vec<u8>>,
    // Complete committed IDs, including cold/quarantined content; revisions alone
    // only describe snapshots written by this process. None requires a fresh scan.
    inventory: Option<ContentInventory>,
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
        Self::open_owned(root, ownership).await
    }
    async fn open_owned(root: &Path, ownership: File) -> io::Result<Self> {
        let mut db = Self::connect(root, ownership).await?;
        // Version mismatches need the same visible, explicit fresh-start action
        // as other restore failures. Never silently discard an older session.
        db.initialize().await?;
        db.load_quarantine().await?;
        // Reserve sequences from cold tabs and parked branches without reading payloads.
        let maximum = db
            .integer("SELECT max(value) FROM (SELECT coalesce(max(global_seq),0) AS value FROM history UNION ALL SELECT coalesce(max(global_seq),0) FROM activity)")
            .await?;
        crate::app::domain::buffer::register_text_history_global_seq(maximum as u64);
        // Closed documents/layouts may outlive every restored tab. Reserve their
        // runtime IDs before New or split can allocate IDs used by a later reopen.
        let buffer_maximum = db.integer("SELECT coalesce(max(CASE WHEN json_valid(CAST(payload AS TEXT)) THEN json_extract(CAST(payload AS TEXT),'$.id') ELSE 0 END),0) FROM documents").await?;
        crate::app::domain::buffer::register_existing_buffer_id(buffer_maximum as u64);
        let view_maximum = db.integer("SELECT coalesce(max(json_extract(v.value,'$.id')),0) FROM (SELECT payload FROM tabs UNION ALL SELECT payload FROM closed_tabs) w, json_each(CASE WHEN json_valid(CAST(w.payload AS TEXT)) THEN CAST(w.payload AS TEXT) ELSE '{}' END,'$.views') v").await?;
        crate::app::domain::view::register_existing_view_id(view_maximum as u64);
        Ok(db)
    }
    async fn connect(root: &Path, ownership: File) -> io::Result<Self> {
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
        Ok(Self {
            conn,
            database,
            ownership,
            startup_notices: Vec::new(),
            tab_cache: Vec::new(),
            state_cache: Vec::new(),
            revisions: HashMap::new(),
            chunk_cache: HashMap::new(),
            history_cache: HashMap::new(),
            document_cache: HashMap::new(),
            inventory: None,
            latest_request: 0,
            blocked: None,
            verified: false,
            quarantined: HashMap::new(),
        })
    }
    async fn initialize(&mut self) -> io::Result<()> {
        let version = self.integer("PRAGMA user_version").await?;
        let application = self.integer("PRAGMA application_id").await?;
        if application == APPLICATION_ID && version == 11 {
            return Ok(());
        }
        if version != 0
            || application != 0
            || self
                .integer("SELECT count(*) FROM sqlite_master WHERE type='table'")
                .await?
                != 0
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                schema_reset::UnsupportedSchema {
                    version,
                    application,
                },
            ));
        }
        self.create_schema(&format!(
            "{SCHEMA} {} {} PRAGMA application_id={APPLICATION_ID}; PRAGMA user_version=11;",
            quarantine::SCHEMA,
            closed::SCHEMA
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
            .prepare_cached(sql)
            .await
            .map_err(error)?
            .query(())
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
        let mut statement = self.conn.prepare_cached(sql).await.map_err(error)?;
        let mut rows = statement.query(params).await.map_err(error)?;
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
    // Only acknowledge our own writes: an earlier unexpected write must still
    // invalidate the inventory on the next save.

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
            self.document_cache.clear();
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
