use super::BufferState;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiskFileState {
    pub modified_millis: Option<u64>,
    pub len: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BufferFreshness {
    #[default]
    InSync,
    AutoReloaded,
    StaleOnDisk,
    ConflictOnDisk,
    MissingOnDisk,
}

impl BufferState {
    pub fn sync_to_disk_state(&mut self, disk_state: Option<DiskFileState>) {
        self.set_disk_state(disk_state, BufferFreshness::InSync);
    }

    pub fn mark_auto_reloaded_from_disk(&mut self, disk_state: Option<DiskFileState>) {
        self.set_disk_state(disk_state, BufferFreshness::AutoReloaded);
    }

    pub fn mark_stale_on_disk(&mut self, disk_state: Option<DiskFileState>) {
        self.set_disk_state(disk_state, BufferFreshness::StaleOnDisk);
    }

    pub fn mark_conflict_on_disk(&mut self, disk_state: Option<DiskFileState>) {
        self.set_disk_state(disk_state, BufferFreshness::ConflictOnDisk);
    }

    pub fn mark_missing_on_disk(&mut self) {
        self.freshness = BufferFreshness::MissingOnDisk;
    }

    pub fn mark_dirty_after_local_edit(&mut self) {
        self.is_dirty = true;
        if self.freshness == BufferFreshness::AutoReloaded {
            self.freshness = BufferFreshness::InSync;
        }
    }

    #[must_use]
    pub fn disk_status_label(&self) -> Option<&'static str> {
        match self.freshness {
            BufferFreshness::InSync => None,
            BufferFreshness::AutoReloaded => Some("Auto reloaded"),
            BufferFreshness::StaleOnDisk => Some("On disk changed"),
            BufferFreshness::ConflictOnDisk => Some("Disk conflict"),
            BufferFreshness::MissingOnDisk => Some("File missing"),
        }
    }

    #[must_use]
    pub fn disk_status_message(&self) -> Option<String> {
        let path_label = self
            .path
            .as_ref()
            .map_or_else(|| self.name.clone(), |path| path.display().to_string());

        match self.freshness {
            BufferFreshness::InSync => None,
            BufferFreshness::AutoReloaded => Some(format!(
                "{path_label} was reloaded after it changed on disk."
            )),
            BufferFreshness::StaleOnDisk => Some(format!("{path_label} changed on disk.")),
            BufferFreshness::ConflictOnDisk => Some(format!(
                "{path_label} changed on disk. Your tab has unsaved edits."
            )),
            BufferFreshness::MissingOnDisk => Some(format!("{path_label} is missing on disk.")),
        }
    }

    fn set_disk_state(&mut self, disk_state: Option<DiskFileState>, freshness: BufferFreshness) {
        self.disk_state = disk_state;
        self.freshness = freshness;
    }
}
