use super::{Database, error, invalid};
use crate::app::services::session_store::activity::ActivityEvent;
use std::io;
use turso::params;

pub(super) const REPLACE_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS replace_operations(global_seq INTEGER PRIMARY KEY CHECK(global_seq>0), observed_at INTEGER NOT NULL, payload TEXT NOT NULL, document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE, name TEXT NOT NULL, path TEXT, search_text TEXT NOT NULL DEFAULT '') STRICT; CREATE INDEX IF NOT EXISTS replace_operations_document ON replace_operations(document_id, global_seq);";
impl Database {
    pub(super) async fn write_activity(
        &self,
        events: &[ActivityEvent],
        forgotten: &[String],
    ) -> io::Result<()> {
        for document in forgotten {
            self.execute_cached(
                "DELETE FROM replace_operations WHERE document_id=?",
                [document.as_str()],
            )
            .await?;
            self.execute_cached(
                "DELETE FROM activity WHERE document_id=?",
                [document.as_str()],
            )
            .await?;
        }
        let mut insert=self.conn.prepare_cached("INSERT INTO activity(global_seq,observed_at,kind,document_id,name,path,search_text) VALUES(?,?,?,?,?,?,?) ON CONFLICT(global_seq) DO NOTHING").await.map_err(error)?;
        let mut operation = self.conn.prepare_cached("INSERT INTO replace_operations(global_seq,observed_at,payload,document_id,name,path,search_text) VALUES(?,?,?,?,?,?,?) ON CONFLICT(global_seq) DO NOTHING").await.map_err(error)?;
        for event in events {
            let sequence = i64::try_from(event.sequence).map_err(invalid)?;
            let timestamp = i64::try_from(event.timestamp).map_err(invalid)?;
            if sequence <= 0 {
                return Err(invalid("activity sequences must be positive"));
            }
            let statement = if matches!(
                event.kind,
                crate::app::services::session_store::activity::ActivityKind::ReplaceAll { .. }
            ) {
                &mut operation
            } else {
                &mut insert
            };
            statement
                .execute(params![
                    sequence,
                    timestamp,
                    event.kind.storage_key().map_err(invalid)?,
                    event.document.as_str(),
                    event.name.as_str(),
                    event.path.clone(),
                    format!(
                        "{}\n{}",
                        event.name,
                        event.path.as_deref().unwrap_or_default()
                    )
                    .to_lowercase()
                ])
                .await
                .map_err(error)?;
        }
        Ok(())
    }
}
