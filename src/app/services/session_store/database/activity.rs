use super::{Database, error, invalid};
use crate::app::services::session_store::activity::ActivityEvent;
use std::io;
use turso::params;

impl Database {
    pub(super) async fn write_activity(
        &self,
        events: &[ActivityEvent],
        forgotten: &[String],
    ) -> io::Result<()> {
        for document in forgotten {
            self.execute_cached(
                "DELETE FROM activity WHERE document_id=?",
                [document.as_str()],
            )
            .await?;
        }
        let mut insert=self.conn.prepare_cached("INSERT INTO activity(global_seq,observed_at,kind,document_id,name,path) VALUES(?,?,?,?,?,?) ON CONFLICT(global_seq) DO NOTHING").await.map_err(error)?;
        for event in events {
            let sequence = i64::try_from(event.sequence).map_err(invalid)?;
            let timestamp = i64::try_from(event.timestamp).map_err(invalid)?;
            if sequence <= 0 {
                return Err(invalid("activity sequences must be positive"));
            }
            insert
                .execute(params![
                    sequence,
                    timestamp,
                    event.kind.key(),
                    event.document.as_str(),
                    event.name.as_str(),
                    event.path.clone()
                ])
                .await
                .map_err(error)?;
        }
        Ok(())
    }
}
