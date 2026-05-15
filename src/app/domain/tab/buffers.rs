use crate::app::domain::{BufferId, BufferState};
use std::collections::HashSet;

#[derive(Clone)]
pub struct WorkspaceTabBuffers {
    pub buffer: BufferState,
    pub extra_buffers: Vec<BufferState>,
}

impl WorkspaceTabBuffers {
    #[must_use]
    pub fn new(buffer: BufferState) -> Self {
        Self {
            buffer,
            extra_buffers: Vec::new(),
        }
    }

    #[must_use]
    pub fn restored(buffer: BufferState, extra_buffers: Vec<BufferState>) -> Self {
        Self {
            buffer,
            extra_buffers,
        }
    }

    #[must_use]
    pub fn active(&self) -> &BufferState {
        &self.buffer
    }

    pub fn active_mut(&mut self) -> &mut BufferState {
        &mut self.buffer
    }

    pub fn all(&self) -> impl Iterator<Item = &BufferState> {
        std::iter::once(&self.buffer).chain(self.extra_buffers.iter())
    }

    pub fn all_mut(&mut self) -> impl Iterator<Item = &mut BufferState> {
        std::iter::once(&mut self.buffer).chain(self.extra_buffers.iter_mut())
    }

    #[must_use]
    pub fn by_id(&self, buffer_id: BufferId) -> Option<&BufferState> {
        self.active_matches_id(buffer_id)
            .then_some(&self.buffer)
            .or_else(|| {
                self.extra_buffers
                    .iter()
                    .find(|buffer| buffer.id == buffer_id)
            })
    }

    pub fn by_id_mut(&mut self, buffer_id: BufferId) -> Option<&mut BufferState> {
        if self.active_matches_id(buffer_id) {
            Some(&mut self.buffer)
        } else {
            self.extra_buffers
                .iter_mut()
                .find(|buffer| buffer.id == buffer_id)
        }
    }

    pub fn push_if_missing(&mut self, buffer: BufferState) {
        if self.by_id(buffer.id).is_none() {
            self.extra_buffers.push(buffer);
        }
    }

    pub fn push_extra(&mut self, buffer: BufferState) {
        self.extra_buffers.push(buffer);
    }

    pub fn sync_active_buffer_to(&mut self, active_buffer_id: BufferId) -> bool {
        if self.buffer.id == active_buffer_id {
            return true;
        }

        let Some(buffer_index) = Self::extra_buffer_index(&self.extra_buffers, active_buffer_id)
        else {
            return false;
        };

        std::mem::swap(&mut self.buffer, &mut self.extra_buffers[buffer_index]);
        true
    }

    pub fn prune_to_buffer_ids(&mut self, referenced_buffer_ids: &HashSet<BufferId>) {
        self.extra_buffers
            .retain(|buffer| referenced_buffer_ids.contains(&buffer.id));
    }

    pub fn take_by_id(
        &mut self,
        buffer_id: BufferId,
        replacement_buffer_id: BufferId,
    ) -> Option<BufferState> {
        if self.buffer.id == buffer_id {
            let replacement_index = self
                .extra_buffers
                .iter()
                .position(|buffer| buffer.id == replacement_buffer_id)?;
            let replacement = self.extra_buffers.swap_remove(replacement_index);
            Some(std::mem::replace(&mut self.buffer, replacement))
        } else {
            let buffer_index = self
                .extra_buffers
                .iter()
                .position(|buffer| buffer.id == buffer_id)?;
            Some(self.extra_buffers.swap_remove(buffer_index))
        }
    }

    #[must_use]
    pub fn into_parts(self) -> (BufferState, Vec<BufferState>) {
        (self.buffer, self.extra_buffers)
    }

    fn active_matches_id(&self, buffer_id: BufferId) -> bool {
        self.buffer.id == buffer_id
    }

    pub(super) fn extra_buffer_index(
        extra_buffers: &[BufferState],
        buffer_id: BufferId,
    ) -> Option<usize> {
        extra_buffers
            .iter()
            .position(|buffer| buffer.id == buffer_id)
    }
}
