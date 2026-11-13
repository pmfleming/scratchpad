//! Local-only transactional session storage. All access belongs to one worker.
mod activity;
#[cfg(test)]
mod activity_tests;
#[cfg(test)]
mod app_safety_tests;
mod chunks;
pub(super) mod closed;
#[cfg(test)]
mod closed_tests;
mod content;
mod content_read;
mod document_rows;
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
mod insert;
#[cfg(test)]
mod inventory_tests;
mod layouts;
mod persist;
mod profiling;
#[cfg(test)]
mod reference_tests;
pub use profiling::SessionMaintenanceFixture;
mod quarantine;
#[cfg(test)]
mod quarantine_tests;
#[cfg(test)]
mod range_tests;
mod read;
mod recovery;
#[cfg(test)]
mod recovery_tests;
mod recovery_text;
mod reset;
mod rows;
pub(super) use reset::start_new_session;
mod schema_reset;
#[cfg(test)]
mod schema_reset_tests;
#[cfg(test)]
mod size_tests;
#[cfg(test)]
mod test_fixture;
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
const WRITER_BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(250);
const SCHEMA: &str = "
CREATE TABLE activity(global_seq INTEGER PRIMARY KEY CHECK(global_seq>0), observed_at INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('created','opened','closed','disk_conflict','missing')),
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE, name TEXT NOT NULL, path TEXT, search_text TEXT NOT NULL DEFAULT '') STRICT;
CREATE INDEX activity_document_sequence ON activity(document_id,global_seq);
CREATE TABLE history_nodes(temp_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    node_id INTEGER NOT NULL CHECK(node_id>=0), PRIMARY KEY(temp_id,node_id)) STRICT;
CREATE TABLE history(temp_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
                     seq INTEGER NOT NULL, parent_id INTEGER NOT NULL CHECK(parent_id>=0 AND parent_id<seq), edited_at INTEGER NOT NULL, global_seq INTEGER NOT NULL CHECK(global_seq>0),
                     source TEXT NOT NULL CHECK(source IN ('load','edit','paste','cut','search_replace','external')), summary TEXT NOT NULL, replayable INTEGER NOT NULL CHECK(replayable IN (0,1)), bytes INTEGER NOT NULL DEFAULT 0 CHECK(bytes>=0),
                     previous_primary INTEGER NOT NULL,previous_primary_next INTEGER NOT NULL,previous_secondary INTEGER NOT NULL,previous_secondary_next INTEGER NOT NULL,
                     next_primary INTEGER NOT NULL,next_primary_next INTEGER NOT NULL,next_secondary INTEGER NOT NULL,next_secondary_next INTEGER NOT NULL,packed_spans BLOB, PRIMARY KEY(temp_id, seq),
                     FOREIGN KEY(temp_id,seq) REFERENCES history_nodes(temp_id,node_id) ON DELETE CASCADE,
                     FOREIGN KEY(temp_id,parent_id) REFERENCES history_nodes(temp_id,node_id) ON DELETE CASCADE) STRICT;
CREATE TABLE history_spans(temp_id TEXT NOT NULL,seq INTEGER NOT NULL,edit INTEGER NOT NULL CHECK(edit>=0),role INTEGER NOT NULL CHECK(role IN (0,1)),part INTEGER NOT NULL CHECK(part>=0),start_char INTEGER NOT NULL CHECK(start_char>=0),buffer INTEGER NOT NULL CHECK(buffer IN (0,1)),start_byte INTEGER NOT NULL CHECK(start_byte>=0),byte_len INTEGER NOT NULL CHECK(byte_len>=0),char_len INTEGER NOT NULL CHECK(char_len>=0),PRIMARY KEY(temp_id,seq,edit,role,part),FOREIGN KEY(temp_id,seq) REFERENCES history(temp_id,seq) ON DELETE CASCADE) STRICT;
CREATE TABLE document_history(temp_id TEXT PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
    now_id INTEGER NOT NULL CHECK(now_id>=0), tip_id INTEGER NOT NULL CHECK(tip_id>=0), edited_at INTEGER NOT NULL, edit_seq INTEGER NOT NULL,
    FOREIGN KEY(temp_id,now_id) REFERENCES history_nodes(temp_id,node_id) ON DELETE CASCADE,
    FOREIGN KEY(temp_id,tip_id) REFERENCES history_nodes(temp_id,node_id) ON DELETE CASCADE) STRICT;
CREATE INDEX document_recency ON document_history(edit_seq DESC);
CREATE INDEX history_parent ON history(temp_id, parent_id);
CREATE INDEX history_global_seq ON history(global_seq,temp_id,seq);
CREATE INDEX history_document_sequence ON history(temp_id,global_seq,seq);
CREATE INDEX history_sizes ON history(temp_id, bytes);
CREATE TABLE document_pieces(id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,part INTEGER NOT NULL CHECK(part>=0),bytes INTEGER NOT NULL CHECK(bytes>=0 AND (part=0 OR bytes=0)),digest BLOB NOT NULL CHECK(length(digest)=32),body BLOB NOT NULL CHECK(length(body)<=65536),PRIMARY KEY(id,part)) STRICT;
CREATE TABLE document_text(temp_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,buffer INTEGER NOT NULL CHECK(buffer IN (0,1)),position INTEGER NOT NULL CHECK(position>=0),start_byte INTEGER NOT NULL CHECK(start_byte>=0),chunk_id INTEGER NOT NULL REFERENCES chunks(id),PRIMARY KEY(temp_id,buffer,position)) STRICT;
CREATE INDEX document_text_chunk ON document_text(chunk_id);
CREATE TABLE document_tail(temp_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,start_byte INTEGER NOT NULL CHECK(start_byte>=0),offset INTEGER NOT NULL CHECK(offset>=0),body BLOB NOT NULL,digest BLOB NOT NULL CHECK(length(digest)=32),PRIMARY KEY(temp_id,offset)) STRICT;";

pub(super) struct StoredSession {
    pub manifest: SessionManifest,
    pub quarantined: HashSet<String>,
    pub profile: super::SessionRestoreProfile,
}
#[cfg(test)]
impl StoredSession {
    pub(crate) fn text_contents(&self) -> HashMap<String, String> {
        self.manifest
            .tabs
            .iter()
            .flat_map(|t| &t.buffers)
            .filter_map(|b| {
                b.history_document
                    .as_ref()
                    .map(|d| (b.temp_id.clone(), d.try_extract_text().unwrap()))
            })
            .collect()
    }
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
    // The actor runs operations serially. Reuse execution state as well as SQL
    // programs; Turso's prepare_cached creates a fresh VM for every call.
    statements: std::sync::Mutex<HashMap<String, turso::Statement>>,
    chunks: Option<crate::app::services::chunk_store::ChunkStoreHandle>,
    database: TursoDatabase,
    ownership: File,
    startup_notices: Vec<super::RestoreStatus>,
    tab_cache: Vec<u64>,
    layout_cache: HashMap<u64, layouts::LayoutRow>,
    state_cache: Option<layouts::StateRow>,
    revisions: HashMap<String, crate::app::domain::buffer::SnapshotRevision>,
    tail_cache: HashMap<String, content::TailCheckpoint>,
    history_positions: HashMap<String, (u64, u64, u64, u64)>,
    history_cache: HashMap<String, std::sync::Arc<[history::HistoryFingerprint]>>,
    history_inputs: HashMap<
        String,
        crate::app::domain::buffer::PersistentVector<crate::app::domain::buffer::PieceHistoryEntry>,
    >,
    document_cache: HashMap<String, document_rows::DocumentRow>,
    // Complete committed IDs, including cold/quarantined content; revisions alone
    // only describe snapshots written by this process. None requires a fresh scan.
    inventory: Option<ContentInventory>,
    latest_request: u64,
    pub(super) blocked: Option<String>,
    verified: bool,
    collection_cursor: i64,
    collection_after: std::time::Instant,
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
        let buffer_maximum = db.integer("SELECT coalesce(max(value),0) FROM (SELECT max(buffer_id) AS value FROM documents UNION ALL SELECT max(buffer_id) FROM layout_documents)").await?;
        crate::app::domain::buffer::register_existing_buffer_id(buffer_maximum as u64);
        let view_maximum = db
            .integer("SELECT coalesce(max(id),0) FROM layout_views")
            .await?;
        crate::app::domain::view::register_existing_view_id(view_maximum as u64);
        let layout_maximum = db
            .integer("SELECT coalesce(max(id),0) FROM layouts")
            .await?;
        crate::app::domain::tab::register_existing_layout_id(layout_maximum as u64);
        db.chunks = Some(crate::app::services::chunk_store::ChunkStoreHandle::open(
            &root.join("session.db"),
        )?);
        db.repair_quarantined_lengths().await?;
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
        // Retry brief lock contention on the actor thread without waiting
        // indefinitely for an unexpected second writer.
        conn.busy_timeout(WRITER_BUSY_TIMEOUT).map_err(error)?;
        conn.execute_batch(
            "PRAGMA synchronous=FULL; PRAGMA cache_size=-8192; PRAGMA foreign_keys=ON;",
        )
        .await
        .map_err(error)?;
        Ok(Self {
            conn,
            statements: std::sync::Mutex::new(HashMap::new()),
            chunks: None,
            database,
            ownership,
            startup_notices: Vec::new(),
            tab_cache: Vec::new(),
            state_cache: None,
            layout_cache: HashMap::new(),
            revisions: HashMap::new(),
            tail_cache: HashMap::new(),
            history_positions: HashMap::new(),
            history_cache: HashMap::new(),
            history_inputs: HashMap::new(),
            document_cache: HashMap::new(),
            inventory: None,
            latest_request: 0,
            blocked: None,
            verified: false,
            collection_cursor: 0,
            collection_after: std::time::Instant::now(),
            quarantined: HashMap::new(),
        })
    }
    async fn initialize(&mut self) -> io::Result<()> {
        let version = self.integer("PRAGMA user_version").await?;
        let application = self.integer("PRAGMA application_id").await?;
        if application == APPLICATION_ID && version == 13 {
            self.create_schema(activity::REPLACE_SCHEMA).await?;
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
            "{} {} {SCHEMA} {} {} {} {} PRAGMA application_id={APPLICATION_ID}; PRAGMA user_version=13;",
            document_rows::SCHEMA,
            layouts::SCHEMA,
            quarantine::SCHEMA,
            closed::SCHEMA, chunks::SCHEMA, activity::REPLACE_SCHEMA
        ))
        .await
    }
    async fn create_schema(&mut self, sql: &str) -> io::Result<()> {
        self.conn
            .execute("BEGIN IMMEDIATE", ())
            .await
            .map_err(error)?;
        // Let Turso parse statement boundaries, including trigger bodies and
        // quoted semicolons, while retaining all-or-nothing schema creation.
        let result = self.conn.execute_batch(sql).await.map_err(error);
        self.finish_transaction(result).await
    }
    async fn integer(&self, sql: &str) -> io::Result<i64> {
        // Exhaust the cursor before retaining its VM, including scalar queries.
        self.query_collect(sql, (), |row| row.get(0).map_err(error))
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| invalid("missing scalar database result"))
    }
    async fn statement(&self, sql: &str) -> io::Result<turso::Statement> {
        if let Some(statement) = self.statements.lock().unwrap().get(sql).cloned() {
            return Ok(statement);
        }
        let statement = self.conn.prepare(sql).await.map_err(error)?;
        let mut statements = self.statements.lock().unwrap();
        if statements.len() >= 128 {
            statements.clear();
        }
        statements.insert(sql.to_owned(), statement.clone());
        Ok(statement)
    }
    // Map one row at a time: fingerprint callers release each undo payload here,
    // rather than materializing a second complete history before hashing it.
    async fn query_collect<T>(
        &self,
        sql: &str,
        params: impl turso::IntoParams,
        mut map: impl FnMut(&turso::Row) -> io::Result<T>,
    ) -> io::Result<Vec<T>> {
        let mut statement = self.statement(sql).await?;
        let result = async {
            let mut rows = statement.query(params).await.map_err(error)?;
            let mut values = Vec::new();
            while let Some(row) = rows.next().await.map_err(error)? {
                values.push(map(&row)?);
            }
            Ok(values)
        }
        .await;
        if result.is_err() {
            // A decoding failure can leave a cursor before Done. Drop that VM
            // so recovery/rollback does not inherit an active statement.
            self.statements.lock().unwrap().remove(sql);
        }
        result
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
        self.execute_count_cached(sql, params).await.map(|_| ())
    }
    async fn execute_count_cached(
        &self,
        sql: &str,
        params: impl turso::IntoParams,
    ) -> io::Result<u64> {
        let mut statement = self.statement(sql).await?;
        let result = statement.execute(params).await.map_err(error);
        if result.is_err() {
            self.statements.lock().unwrap().remove(sql);
        }
        result
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
        if result.is_ok() {
            self.collection_after = std::time::Instant::now();
        } else {
            // A failed COMMIT can be ambiguous. Never trust pre-attempt caches.
            self.state_cache = None;
            self.layout_cache.clear();
            self.tab_cache.clear();
            self.revisions.clear();
            self.tail_cache.clear();
            self.history_positions.clear();
            self.history_cache.clear();
            self.history_inputs.clear();
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
