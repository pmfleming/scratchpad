use super::{Database, StoredSession, decode, error, invalid};
use crate::app::services::session_store::{
    legacy,
    model::{SessionManifest, SessionTab},
};
use std::collections::{HashMap, HashSet};
use std::io;

fn validate_required(tabs: &[SessionTab], contains: impl Fn(&str) -> bool) -> io::Result<()> {
    if tabs
        .iter()
        .flat_map(|tab| &tab.buffers)
        .any(|buffer| legacy::needs_snapshot(buffer) && !contains(&buffer.temp_id))
    {
        return Err(invalid(
            "required recovery content is missing; session protected from overwrite",
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
        let result = self.read_session(scope).await;
        if let Err(error) = &result {
            self.blocked = Some(error.to_string());
        } else if !matches!(scope, ReadScope::Active) {
            self.verified = true;
        }
        result
    }
    async fn read_session(&self, scope: ReadScope) -> io::Result<Option<StoredSession>> {
        let Some(mut manifest) = self.read_manifest().await? else {
            return Ok(None);
        };
        self.read_tabs(&mut manifest, scope).await?;
        let contents = match scope {
            ReadScope::All => self.read_all_contents().await?,
            ReadScope::Active => self.read_tab_contents(&manifest.tabs).await?,
            ReadScope::Shells => {
                self.validate_snapshots(&manifest.tabs).await?;
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
            validate_required(&manifest.tabs, |id| contents.contains_key(id))?;
        }
        if matches!(scope, ReadScope::Active) {
            manifest.active_tab_index = 0;
        }
        Ok(Some(StoredSession { manifest, contents }))
    }
    async fn read_manifest(&self) -> io::Result<Option<SessionManifest>> {
        let mut rows = self
            .conn
            .query("SELECT payload FROM session_state WHERE id=1", ())
            .await
            .map_err(error)?;
        let Some(row) = rows.next().await.map_err(error)? else {
            drop(rows);
            if self.integer("SELECT (SELECT count(*) FROM tabs)+(SELECT count(*) FROM snapshots)+(SELECT count(*) FROM chunks)").await? != 0 {
                return Err(invalid("missing session commit marker"));
            }
            return Ok(None);
        };
        let manifest: SessionManifest = decode(&row.get::<Vec<u8>>(0).map_err(error)?)?;
        if manifest.version != super::super::SESSION_VERSION
            || !manifest.tabs.is_empty()
            || manifest.active_tab_index >= manifest.tab_count.max(1)
        {
            return Err(invalid("unsupported or invalid session payload"));
        }
        Ok(Some(manifest))
    }
    async fn read_tabs(&self, manifest: &mut SessionManifest, scope: ReadScope) -> io::Result<()> {
        let active_only = matches!(scope, ReadScope::Active);
        let mut rows = if active_only {
            self.conn
                .query(
                    "SELECT position, payload FROM tabs WHERE position=?",
                    [manifest.active_tab_index as i64],
                )
                .await
        } else {
            self.conn
                .query("SELECT position, payload FROM tabs ORDER BY position", ())
                .await
        }
        .map_err(error)?;
        while let Some(row) = rows.next().await.map_err(error)? {
            let position = if active_only {
                manifest.active_tab_index
            } else {
                manifest.tabs.len()
            };
            if row.get::<i64>(0).map_err(error)? != position as i64 {
                return Err(invalid("noncontiguous session tab order"));
            }
            let tab: SessionTab = decode(&row.get::<Vec<u8>>(1).map_err(error)?)?;
            legacy::validate_tab(&tab)?;
            manifest.tabs.push(tab);
        }
        drop(rows);
        let expected = if active_only {
            manifest.tab_count.min(1)
        } else {
            manifest.tab_count
        };
        if manifest.tabs.len() != expected {
            return Err(invalid("missing session tabs"));
        }
        Ok(())
    }
    pub async fn hydrate(
        &mut self,
        tab: SessionTab,
    ) -> io::Result<(SessionTab, HashMap<String, String>)> {
        let result = self.hydrate_contents(&tab).await;
        if let Err(error) = &result {
            self.blocked = Some(error.to_string());
        }
        Ok((tab, result?))
    }
    async fn hydrate_contents(&self, tab: &SessionTab) -> io::Result<HashMap<String, String>> {
        legacy::validate_tab(tab)?;
        let tabs = std::slice::from_ref(tab);
        self.read_tab_contents(tabs).await
    }
    pub(super) async fn validate_snapshots(&self, tabs: &[SessionTab]) -> io::Result<()> {
        let mut rows = self
            .conn
            .query("SELECT id FROM snapshots", ())
            .await
            .map_err(error)?;
        let mut ids = HashSet::new();
        while let Some(row) = rows.next().await.map_err(error)? {
            ids.insert(row.get::<String>(0).map_err(error)?);
        }
        validate_required(tabs, |id| ids.contains(id))
    }
}
