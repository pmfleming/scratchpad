use super::{
    Database, StoredSession, error, invalid,
    quarantine::{damaged, damaged_id},
};
use crate::app::services::session_store::{
    model::{SessionManifest, SessionTab},
    validation,
};
use std::collections::{HashMap, HashSet};
use std::io;

fn validate_required(tabs: &[SessionTab], contains: impl Fn(&str) -> bool) -> io::Result<()> {
    if let Some(buffer) = tabs
        .iter()
        .flat_map(|tab| &tab.buffers)
        .find(|buffer| validation::needs_snapshot(buffer) && !contains(&buffer.temp_id))
    {
        return Err(damaged(
            &buffer.temp_id,
            "required recovery content is missing",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(in crate::app::services::session_store) enum ReadScope {
    All,
    Active,
    Shells,
}

impl Database {
    pub async fn load(&mut self, scope: ReadScope) -> io::Result<Option<StoredSession>> {
        self.ensure_healthy()?;
        let store = self.chunk_store()?;
        store.epoch().ensure_open()?;
        store.set_bulk_stopped(false);
        let result = self
            .read_session(scope)
            .await
            .map_err(|cause| self.quarantine_error(cause));
        if let Err(error) = &result {
            if damaged_id(error).is_none() {
                self.blocked = Some(error.to_string());
            }
        } else if !matches!(scope, ReadScope::Active) {
            self.verified = true;
        }
        result
    }
    async fn read_session(&mut self, scope: ReadScope) -> io::Result<Option<StoredSession>> {
        if matches!(scope, ReadScope::All)
            && let Some(id) = self.quarantined.keys().next()
        {
            return Err(damaged(id, "original recovery data is protected"));
        }
        if self.integer("SELECT (SELECT count(*) FROM document_text t LEFT JOIN documents d ON t.temp_id=d.id WHERE d.id IS NULL)+(SELECT count(*) FROM document_tail t LEFT JOIN documents d ON t.temp_id=d.id WHERE d.id IS NULL)+(SELECT count(*) FROM document_pieces t LEFT JOIN documents d ON t.id=d.id WHERE d.id IS NULL)").await?!=0 {return Err(invalid("orphan text manifest"));}
        let Some(mut manifest) = self.read_manifest().await? else {
            return Ok(None);
        };
        self.read_tabs(&mut manifest, scope).await?;
        if !matches!(scope, ReadScope::Shells)
            && let Some(buffer) = manifest
                .tabs
                .iter()
                .flat_map(|t| &t.buffers)
                .find(|b| self.quarantined.contains_key(&b.temp_id))
        {
            return Err(damaged(
                &buffer.temp_id,
                "original recovery data is protected",
            ));
        }
        let contents = match async {
            let contents = self.read_contents(&manifest, scope).await?;
            let tabs = if matches!(scope, ReadScope::Shells) {
                let active = manifest.active_tab_index;
                manifest
                    .tabs
                    .get_mut(active)
                    .map(std::slice::from_mut)
                    .unwrap_or_default()
            } else {
                &mut manifest.tabs
            };
            self.read_histories(tabs).await?;
            Ok(contents)
        }
        .await
        {
            Ok(contents) => contents,
            Err(cause) => {
                self.protect_damaged(&cause, &manifest.tabs).await?;
                if !matches!(scope, ReadScope::Shells) || damaged_id(&cause).is_none() {
                    return Err(cause);
                }
                // Keep a failed active tab cold, with its original recovery payload.
                HashMap::new()
            }
        };
        if matches!(scope, ReadScope::Active) {
            manifest.active_tab_index = 0;
        }
        Ok(Some(StoredSession {
            manifest,
            contents,
            quarantined: self.quarantined.keys().cloned().collect(),
        }))
    }
    async fn read_contents(
        &mut self,
        manifest: &SessionManifest,
        scope: ReadScope,
    ) -> io::Result<HashMap<String, String>> {
        let contents = match scope {
            ReadScope::All => {
                self.validate_document_pieces(&manifest.tabs).await?;
                HashMap::new()
            }
            ReadScope::Active => self.read_tab_contents(&manifest.tabs).await?,
            ReadScope::Shells => {
                self.validate_document_pieces(&manifest.tabs).await?;
                self.read_tab_contents(
                    manifest
                        .tabs
                        .get(manifest.active_tab_index)
                        .map(std::slice::from_ref)
                        .unwrap_or_default(),
                )
                .await?
            }
        };
        if matches!(scope, ReadScope::All) {
            let stored = self
                .string_set("SELECT id FROM document_pieces WHERE part=0")
                .await?;
            validate_required(&manifest.tabs, |id| stored.contains(id))?;
        }
        Ok(contents)
    }
    async fn read_manifest(&self) -> io::Result<Option<SessionManifest>> {
        let states = self
            .query_collect(
                "SELECT tab_count,active_tab_index,active_surface FROM session_state WHERE id=1",
                (),
                |r| {
                    Ok(super::layouts::StateRow {
                        tab_count: usize::try_from(r.get::<i64>(0).map_err(error)?)
                            .map_err(invalid)?,
                        active_tab_index: usize::try_from(r.get::<i64>(1).map_err(error)?)
                            .map_err(invalid)?,
                        active_surface: r.get(2).map_err(error)?,
                    })
                },
            )
            .await?;
        let Some(state) = states.into_iter().next() else {
            if self.integer("SELECT (SELECT count(*) FROM tabs)+(SELECT count(*) FROM document_pieces)+(SELECT count(*) FROM document_text)+(SELECT count(*) FROM document_tail)+(SELECT count(*) FROM history)+(SELECT count(*) FROM documents)+(SELECT count(*) FROM quarantine)+(SELECT count(*) FROM closed_tabs)+(SELECT count(*) FROM layouts)+(SELECT count(*) FROM layout_documents)+(SELECT count(*) FROM layout_views)+(SELECT count(*) FROM document_history)+(SELECT count(*) FROM activity)").await? != 0 { return Err(invalid("missing session commit marker")); }
            return Ok(None);
        };
        if state.active_tab_index >= state.tab_count.max(1) {
            return Err(invalid("invalid active tab"));
        }
        Ok(Some(SessionManifest {
            version: super::super::SESSION_VERSION,
            tab_count: state.tab_count,
            active_tab_index: state.active_tab_index,
            active_surface: super::document_rows::parse_enum(state.active_surface)?,
            tabs: Vec::new(),
        }))
    }
    async fn read_tabs(&self, manifest: &mut SessionManifest, scope: ReadScope) -> io::Result<()> {
        if self
            .integer("SELECT count(*) FROM tabs t JOIN closed_tabs c ON t.layout_id=c.layout_id")
            .await?
            != 0
        {
            return Err(invalid("layout has multiple owners"));
        }
        let active_only = matches!(scope, ReadScope::Active);
        let rows = self
            .query_collect(
                "SELECT position,layout_id FROM tabs WHERE ?1=0 OR position=?2 ORDER BY position",
                turso::params![
                    i64::from(active_only),
                    i64::try_from(manifest.active_tab_index).map_err(invalid)?
                ],
                |r| {
                    Ok((
                        usize::try_from(r.get::<i64>(0).map_err(error)?).map_err(invalid)?,
                        super::document_rows::unsigned(r.get(1).map_err(error)?)?,
                    ))
                },
            )
            .await?;
        let ids: Vec<_> = rows.iter().map(|r| r.1).collect();
        let mut layouts = self.read_layouts(&ids).await?;
        for (position, id) in rows {
            let expected = if active_only {
                manifest.active_tab_index
            } else {
                manifest.tabs.len()
            };
            if position != expected {
                return Err(invalid("noncontiguous session tab order"));
            }
            manifest.tabs.push(
                layouts
                    .remove(&id)
                    .ok_or_else(|| invalid("missing workspace layout"))?,
            );
        }
        let expected = if active_only {
            manifest.tab_count.min(1)
        } else {
            manifest.tab_count
        };
        if manifest.tabs.len() != expected {
            return Err(invalid("missing session tabs"));
        }
        let mut ids = HashSet::new();
        if manifest
            .tabs
            .iter()
            .flat_map(|t| &t.buffers)
            .any(|b| !ids.insert(&b.temp_id))
        {
            return Err(invalid("duplicate recovery content identity across tabs"));
        }
        Ok(())
    }

    pub async fn hydrate(
        &mut self,
        mut tab: SessionTab,
    ) -> io::Result<(SessionTab, HashMap<String, String>)> {
        self.ensure_healthy()?;
        let result = match async {
            let contents = self.hydrate_contents(&tab).await?;
            self.read_histories(std::slice::from_mut(&mut tab)).await?;
            Ok(contents)
        }
        .await
        {
            Ok(contents) => Ok(contents),
            Err(cause) => {
                let protected = self.protect_current_buffer(&cause).await;
                if matches!(&protected, Ok(false)) {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        "cold buffer was closed before hydration",
                    ));
                }
                protected.and(Err(cause))
            }
        };
        if let Err(error) = &result
            && damaged_id(error).is_none()
        {
            self.blocked = Some(error.to_string());
        }
        Ok((tab, result.map_err(|cause| self.quarantine_error(cause))?))
    }
    // Jobs can outlive a tab or carry older metadata. Archive the committed
    // metadata, not the queued payload; false means no current buffer reference.
    pub(super) async fn protect_current_buffer(&mut self, cause: &io::Error) -> io::Result<bool> {
        let Some(id) = damaged_id(cause) else {
            return Ok(true);
        };
        if self.quarantined.contains_key(id) {
            return Ok(true);
        }
        let Some(mut manifest) = self.read_manifest().await? else {
            return Ok(false);
        };
        self.read_tabs(&mut manifest, ReadScope::All).await?;
        if !manifest
            .tabs
            .iter()
            .flat_map(|tab| &tab.buffers)
            .any(|buffer| buffer.temp_id == id)
        {
            return Ok(false);
        }
        self.protect_damaged(cause, &manifest.tabs).await?;
        Ok(true)
    }
    async fn hydrate_contents(&self, tab: &SessionTab) -> io::Result<HashMap<String, String>> {
        validation::validate_tab(tab)?;
        let tabs = std::slice::from_ref(tab);
        self.read_tab_contents(tabs).await
    }
    pub(super) async fn validate_document_pieces(&mut self, tabs: &[SessionTab]) -> io::Result<()> {
        let ids = self
            .string_set("SELECT id FROM document_pieces WHERE part=0")
            .await?;
        for buffer in tabs
            .iter()
            .flat_map(|tab| &tab.buffers)
            .filter(|buffer| validation::needs_snapshot(buffer) && !ids.contains(&buffer.temp_id))
        {
            self.protect_damaged(
                &damaged(&buffer.temp_id, "required recovery content is missing"),
                tabs,
            )
            .await?;
        }
        Ok(())
    }
}
