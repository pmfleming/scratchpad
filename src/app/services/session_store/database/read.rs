use super::{
    Database, StoredSession, decode, error, invalid,
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
        let Some(mut manifest) = self.read_manifest().await? else {
            return Ok(None);
        };
        self.read_tabs(&mut manifest, scope).await?;
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
        Ok(contents)
    }
    async fn read_manifest(&self) -> io::Result<Option<SessionManifest>> {
        let mut rows = self
            .conn
            .query("SELECT payload FROM session_state WHERE id=1", ())
            .await
            .map_err(error)?;
        let Some(row) = rows.next().await.map_err(error)? else {
            drop(rows);
            if self.integer("SELECT (SELECT count(*) FROM tabs)+(SELECT count(*) FROM snapshots)+(SELECT count(*) FROM parts)+(SELECT count(*) FROM blobs)+(SELECT count(*) FROM history)+(SELECT count(*) FROM buffer_ids)+(SELECT count(*) FROM quarantine)+(SELECT count(*) FROM closed_tabs)+(SELECT count(*) FROM buffer_versions)+(SELECT count(*) FROM version_parts)").await? != 0 {
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
            validation::validate_tab(&tab)?;
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
        let mut ids = HashSet::new();
        if manifest
            .tabs
            .iter()
            .flat_map(|tab| &tab.buffers)
            .any(|buffer| !ids.insert(&buffer.temp_id))
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
    pub(super) async fn validate_snapshots(&mut self, tabs: &[SessionTab]) -> io::Result<()> {
        let ids = self.string_set("SELECT id FROM snapshots").await?;
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
