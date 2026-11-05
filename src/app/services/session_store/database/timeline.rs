use super::{Database, error, invalid};
use crate::app::services::session_store::timeline::{
    TIMELINE_PAGE_SIZE, TimelineCursor, TimelinePage, TimelineRow,
};
use std::io;
use turso::params;

impl Database {
    pub(in crate::app::services::session_store) async fn timeline_page(
        &self,
        before: Option<TimelineCursor>,
        loaded: Vec<String>,
        search: String,
    ) -> io::Result<TimelinePage> {
        self.ensure_healthy()?;
        let first = before.is_none();
        let cursor = before.unwrap_or(TimelineCursor {
            sequence: i64::MAX as u64,
            document: String::new(),
            node: i64::MAX as u64,
        });
        let sequence = i64::try_from(cursor.sequence).map_err(invalid)?;
        let node = i64::try_from(cursor.node).map_err(invalid)?;
        let loaded = serde_json::to_string(&loaded).map_err(invalid)?;
        let rows = self.query_collect(
            "SELECT h.global_seq,h.temp_id,h.seq,h.edited_at,h.source,h.summary,d.name,d.path
             FROM history h JOIN documents d ON d.id=h.temp_id
             WHERE NOT (h.source='load' AND h.parent_id=0 AND h.edited_at=0 AND h.summary='Initial contents') AND h.temp_id NOT IN (SELECT value FROM json_each(?4))
             AND (?5='' OR instr(lower(d.name),?5)>0 OR instr(lower(coalesce(d.path,'')),?5)>0)
             AND (?7=1 OR h.global_seq<?1 OR (h.global_seq=?1 AND (h.temp_id<?2 OR (h.temp_id=?2 AND h.seq<?3))))
             ORDER BY h.global_seq DESC,h.temp_id DESC,h.seq DESC LIMIT ?6",
            params![sequence,cursor.document,node,loaded,search.to_lowercase(),(TIMELINE_PAGE_SIZE+1) as i64,i64::from(first)],
            |row| {
                let source: String = row.get(4).map_err(error)?;
                let source = serde_json::from_value(serde_json::Value::String(source)).map_err(invalid)?;
                Ok(TimelineRow {cursor:TimelineCursor {sequence:row.get::<i64>(0).map_err(error)? as u64,document:row.get(1).map_err(error)?,node:row.get::<i64>(2).map_err(error)? as u64},timestamp:row.get::<i64>(3).map_err(error)? as u64,source,summary:row.get(5).map_err(error)?,name:row.get(6).map_err(error)?,path:row.get(7).map_err(error)?})
            },
        ).await?;
        let more = rows.len() > TIMELINE_PAGE_SIZE;
        let rows: Vec<_> = rows.into_iter().take(TIMELINE_PAGE_SIZE).collect();
        let older = more
            .then(|| rows.last().map(|r| r.cursor.clone()))
            .flatten();
        Ok(TimelinePage { rows, older })
    }
}
