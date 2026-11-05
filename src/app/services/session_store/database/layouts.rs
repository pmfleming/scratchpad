//! Stable layouts shared by live and closed owners, with independent view rows.
use super::{
    Database,
    document_rows::{boolean, signed, unsigned},
    error, invalid,
};
use crate::app::{
    domain::{CharCursor, CursorRange},
    services::session_store::{
        model::{SessionTab, SessionView},
        validation,
    },
};
use std::{collections::HashMap, io};
use turso::params;

pub(super) const SCHEMA: &str = "
CREATE TABLE session_state(id INTEGER PRIMARY KEY CHECK(id=1), tab_count INTEGER NOT NULL CHECK(tab_count>=0), active_tab_index INTEGER NOT NULL CHECK(active_tab_index>=0), active_surface TEXT NOT NULL CHECK(active_surface IN ('workspace','settings'))) STRICT;
CREATE TABLE layouts(id INTEGER PRIMARY KEY CHECK(id>0), active_view_id INTEGER NOT NULL CHECK(active_view_id>0), root_pane TEXT NOT NULL) STRICT;
CREATE TABLE tabs(position INTEGER PRIMARY KEY CHECK(position>=0), layout_id INTEGER NOT NULL UNIQUE REFERENCES layouts(id)) STRICT;
CREATE TABLE layout_documents(layout_id INTEGER NOT NULL REFERENCES layouts(id) ON DELETE CASCADE, ordinal INTEGER NOT NULL CHECK(ordinal>=0), buffer_id INTEGER NOT NULL CHECK(buffer_id>0), document_id TEXT NOT NULL REFERENCES documents(id), PRIMARY KEY(layout_id,ordinal), UNIQUE(layout_id,document_id), UNIQUE(layout_id,buffer_id)) STRICT;
CREATE INDEX layout_document_lookup ON layout_documents(document_id,layout_id);
CREATE TABLE layout_views(layout_id INTEGER NOT NULL REFERENCES layouts(id) ON DELETE CASCADE, ordinal INTEGER NOT NULL CHECK(ordinal>=0), id INTEGER NOT NULL CHECK(id>0), buffer_id INTEGER NOT NULL CHECK(buffer_id>0), show_line_numbers INTEGER NOT NULL CHECK(show_line_numbers IN (0,1)), zoom_factor REAL NOT NULL, primary_index INTEGER, primary_next INTEGER CHECK(primary_next IN (0,1)), secondary_index INTEGER, secondary_next INTEGER CHECK(secondary_next IN (0,1)), CHECK((primary_index IS NULL)=(primary_next IS NULL) AND (primary_index IS NULL)=(secondary_index IS NULL) AND (primary_index IS NULL)=(secondary_next IS NULL)), PRIMARY KEY(layout_id,ordinal), UNIQUE(layout_id,id)) STRICT;
CREATE INDEX layout_views_id ON layout_views(id);";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct StateRow {
    pub tab_count: usize,
    pub active_tab_index: usize,
    pub active_surface: String,
}
#[derive(Clone, PartialEq)]
pub(in crate::app::services::session_store) struct LayoutRow {
    pub id: u64,
    pub active_view_id: u64,
    pub root_pane: String,
    pub documents: Vec<(u64, String)>,
    pub views: Vec<SessionView>,
}
impl LayoutRow {
    pub fn capture(tab: &SessionTab) -> io::Result<Self> {
        validation::validate_tab(tab)?;
        if tab.layout_id == 0 {
            return Err(invalid("missing layout identity"));
        }
        Ok(Self {
            id: tab.layout_id,
            active_view_id: tab.active_view_id,
            root_pane: serde_json::to_string(&tab.root_pane).map_err(invalid)?,
            documents: tab
                .buffers
                .iter()
                .map(|b| (b.id, b.temp_id.clone()))
                .collect(),
            views: tab.views.clone(),
        })
    }
    pub fn estimated_bytes(&self) -> usize {
        32 + self.root_pane.len()
            + self
                .documents
                .iter()
                .map(|(_, id)| 32 + id.len())
                .sum::<usize>()
            + 80 * self.views.len()
    }
}
impl Database {
    pub(super) async fn read_layouts(&self, ids: &[u64]) -> io::Result<HashMap<u64, SessionTab>> {
        let ids = serde_json::to_string(
            &ids.iter()
                .map(|&id| signed(id))
                .collect::<io::Result<Vec<_>>>()?,
        )
        .map_err(invalid)?;
        let mut tabs = HashMap::new();
        for (id, active_view_id, root_pane) in self.query_collect("SELECT id,active_view_id,root_pane FROM layouts WHERE id IN (SELECT value FROM json_each(?))", [ids.as_str()], |r| Ok((unsigned(r.get(0).map_err(error)?)?, unsigned(r.get(1).map_err(error)?)?, r.get::<String>(2).map_err(error)?))).await? {
            tabs.insert(id, SessionTab { layout_id:id, buffers:Vec::new(), active_view_id, root_pane:serde_json::from_str(&root_pane).map_err(invalid)?, views:Vec::new() });
        }
        let columns = super::document_rows::COLUMNS
            .split(',')
            .map(|s| format!("d.{s}"))
            .collect::<Vec<_>>()
            .join(",");
        let mut documents: HashMap<_,_> = self.query_collect(&format!("SELECT {columns} FROM documents d WHERE id IN (SELECT document_id FROM layout_documents WHERE layout_id IN (SELECT value FROM json_each(?)))"),[ids.as_str()],super::document_rows::DocumentRow::read).await?.into_iter().map(|r|(r.id.clone(),r)).collect();
        for (id, ordinal, buffer_id, document_id) in self.query_collect("SELECT layout_id,ordinal,buffer_id,document_id FROM layout_documents WHERE layout_id IN (SELECT value FROM json_each(?)) ORDER BY layout_id,ordinal",[ids.as_str()],|r| Ok((unsigned(r.get(0).map_err(error)?)?,unsigned(r.get(1).map_err(error)?)?,unsigned(r.get(2).map_err(error)?)?,r.get::<String>(3).map_err(error)?))).await? {
            let tab = tabs.get_mut(&id).ok_or_else(||invalid("document has no layout"))?;
            if ordinal != tab.buffers.len() as u64 { return Err(invalid("noncontiguous layout documents")); }
            let mut buffer = documents.get_mut(&document_id).ok_or_else(||invalid("missing layout document"))?.clone().into_buffer()?;
            buffer.id = buffer_id;
            tab.buffers.push(buffer);
        }
        for (id, ordinal, view) in self.query_collect("SELECT layout_id,ordinal,id,buffer_id,show_line_numbers,zoom_factor,primary_index,primary_next,secondary_index,secondary_next FROM layout_views WHERE layout_id IN (SELECT value FROM json_each(?)) ORDER BY layout_id,ordinal",[ids.as_str()],|r| {
            let selection = match r.get::<Option<i64>>(6).map_err(error)? {
                None => None,
                Some(primary) => Some(CursorRange { primary: CharCursor{index:usize::try_from(primary).map_err(invalid)?, prefer_next_row:boolean(r.get(7).map_err(error)?)?}, secondary: CharCursor{index:usize::try_from(r.get::<i64>(8).map_err(error)?).map_err(invalid)?, prefer_next_row:boolean(r.get(9).map_err(error)?)?} }),
            };
            let zoom = r.get::<f64>(5).map_err(error)? as f32;
            if !zoom.is_finite() || zoom <= 0.0 { return Err(invalid("invalid view zoom")); }
            Ok((unsigned(r.get(0).map_err(error)?)?, unsigned(r.get(1).map_err(error)?)?, SessionView {id:unsigned(r.get(2).map_err(error)?)?,buffer_id:unsigned(r.get(3).map_err(error)?)?,show_line_numbers:boolean(r.get(4).map_err(error)?)?,zoom_factor:zoom,selection}))
        }).await? {
            let tab=tabs.get_mut(&id).ok_or_else(||invalid("view has no layout"))?;
            if ordinal != tab.views.len() as u64 { return Err(invalid("noncontiguous layout views")); }
            tab.views.push(view);
        }
        for tab in tabs.values() {
            validation::validate_tab(tab)?;
        }
        Ok(tabs)
    }

    pub(super) async fn read_stored_tab(&self, id: u64) -> io::Result<SessionTab> {
        self.read_layouts(&[id])
            .await?
            .remove(&id)
            .ok_or_else(|| invalid("missing workspace layout"))
    }

    pub(super) async fn write_layout(&self, row: &LayoutRow) -> io::Result<()> {
        let old = self.layout_cache.get(&row.id);
        if old.is_none_or(|old| {
            old.active_view_id != row.active_view_id || old.root_pane != row.root_pane
        }) {
            self.execute_cached("INSERT INTO layouts VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET active_view_id=excluded.active_view_id,root_pane=excluded.root_pane",params![signed(row.id)?,signed(row.active_view_id)?,row.root_pane.as_str()]).await?;
        }
        if old.is_none_or(|old| old.documents != row.documents) {
            self.execute_cached(
                "DELETE FROM layout_documents WHERE layout_id=?",
                [signed(row.id)?],
            )
            .await?;
            for (index, (buffer, id)) in row.documents.iter().enumerate() {
                self.execute_cached(
                    "INSERT INTO layout_documents VALUES(?,?,?,?)",
                    params![
                        signed(row.id)?,
                        i64::try_from(index).map_err(invalid)?,
                        signed(*buffer)?,
                        id.as_str()
                    ],
                )
                .await?;
            }
        }
        // Remove changed rows first: view IDs are unique within a layout, including reorder.
        for index in 0..old
            .map_or(row.views.len(), |old| old.views.len())
            .max(row.views.len())
        {
            if old.and_then(|old| old.views.get(index)).map(|v| v.id)
                == row.views.get(index).map(|v| v.id)
            {
                continue;
            }
            self.execute_cached(
                "DELETE FROM layout_views WHERE layout_id=? AND ordinal=?",
                params![signed(row.id)?, i64::try_from(index).map_err(invalid)?],
            )
            .await?;
        }
        for (index, view) in row.views.iter().enumerate() {
            if old.and_then(|old| old.views.get(index)) == Some(view) {
                continue;
            }
            let selection = view.selection;
            let primary = selection
                .map(|s| i64::try_from(s.primary.index).map_err(invalid))
                .transpose()?;
            let secondary = selection
                .map(|s| i64::try_from(s.secondary.index).map_err(invalid))
                .transpose()?;
            self.execute_cached(
                "INSERT INTO layout_views VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(layout_id,ordinal) DO UPDATE SET id=excluded.id,buffer_id=excluded.buffer_id,show_line_numbers=excluded.show_line_numbers,zoom_factor=excluded.zoom_factor,primary_index=excluded.primary_index,primary_next=excluded.primary_next,secondary_index=excluded.secondary_index,secondary_next=excluded.secondary_next",
                params![
                    signed(row.id)?,
                    i64::try_from(index).map_err(invalid)?,
                    signed(view.id)?,
                    signed(view.buffer_id)?,
                    i64::from(view.show_line_numbers),
                    f64::from(view.zoom_factor),
                    primary,
                    selection.map(|s| i64::from(s.primary.prefer_next_row)),
                    secondary,
                    selection.map(|s| i64::from(s.secondary.prefer_next_row))
                ],
            )
            .await?;
        }
        Ok(())
    }
}
