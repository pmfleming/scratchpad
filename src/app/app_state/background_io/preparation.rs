use super::{BackgroundIoRequest, PendingActionKind, PendingBackgroundAction, ScratchpadApp};
use crate::app::domain::{BufferState, CursorRevealMode, WorkspaceTab, buffer::PreparedText};
use eframe::egui;

impl ScratchpadApp {
    pub(crate) fn queue_text_preparations(&mut self) {
        let mut pending = false;
        for index in 0..self.tab_manager.tabs.len() {
            let ids: Vec<_> = self.tab_manager.tabs.as_slice()[index]
                .buffers()
                .filter(|b| b.is_preparing_text())
                .map(|b| b.id)
                .collect();
            pending |= !ids.is_empty();
            for buffer_id in ids {
                let Some(request) = self.tab_manager.tabs.as_mut_slice()[index]
                    .buffer_by_id_mut(buffer_id)
                    .and_then(BufferState::take_text_preparation)
                else {
                    continue;
                };
                let request_id = self.state.background_io.allocate_background_request_id();
                let request = BackgroundIoRequest::PrepareText {
                    request_id,
                    buffer_id,
                    session_store: self.state.persistence.session_store.clone(),
                    request: Box::new(request),
                };
                match self.state.background_io.tx.send(request) {
                    Ok(()) => self.state.background_io.insert_pending_background_action(
                        request_id,
                        PendingBackgroundAction::PrepareText,
                    ),
                    Err(error) => {
                        let retry = error.reason() == "full";
                        if let BackgroundIoRequest::PrepareText { request, .. } =
                            error.into_request()
                            && let Some(buffer) = self.tab_manager.tabs.as_mut_slice()[index]
                                .buffer_by_id_mut(buffer_id)
                        {
                            if retry {
                                buffer.retry_text_preparation(*request);
                            } else {
                                buffer.cancel_text_preparation();
                                self.state.status.set_error_status_in_domain(
                                    super::super::StatusDomain::File,
                                    "Could not prepare text. Current text was kept.",
                                );
                            }
                        }
                    }
                }
            }
        }
        self.state.background_io.preparations_pending = pending;
    }
    pub(super) fn apply_text_prepared(
        &mut self,
        request_id: u64,
        buffer_id: u64,
        result: Result<Box<PreparedText>, String>,
    ) {
        if self
            .take_pending_action(request_id, PendingActionKind::PrepareText, |action| {
                matches!(action, PendingBackgroundAction::PrepareText).then_some(())
            })
            .is_none()
        {
            return;
        }
        let location = self
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .position(|tab| tab.buffer_by_id(buffer_id).is_some());
        let prepared = match result {
            Ok(prepared) => prepared,
            Err(cause) => {
                if let Some(index) = location
                    && let Some(buffer) =
                        self.tab_manager.tabs.as_mut_slice()[index].buffer_by_id_mut(buffer_id)
                {
                    buffer.cancel_text_preparation();
                    if !matches!(
                        self.state.persistence.session_store.storage_health(),
                        crate::app::services::chunk_store::Health::Healthy
                    ) {
                        buffer.storage_refusal =
                            Some(format!("Large paste or replacement refused. {cause}"));
                    }
                }
                self.state.status.set_error_status_with_detail(
                    super::super::StatusDomain::File,
                    "Could not prepare text. Current text was kept.",
                    cause,
                );
                return;
            }
        };
        let Some(index) = location else {
            self.recover_prepared_text(*prepared);
            return;
        };
        let tab = &mut self.tab_manager.tabs.as_mut_slice()[index];
        let views: Vec<_> = tab
            .layout
            .views()
            .iter()
            .filter(|v| v.buffer_id == buffer_id)
            .map(|view| {
                (
                    view.id,
                    prepared.transformed_selection(
                        view.id,
                        view.cursor_range.or(view.pending_cursor_range),
                    ),
                )
            })
            .collect();
        tab.clear_transient_view_state();
        let Some(buffer) = tab.buffer_by_id_mut(buffer_id) else {
            self.recover_prepared_text(*prepared);
            return;
        };
        match buffer.install_prepared_text(*prepared) {
            Ok(_) => {
                for (id, selection) in views {
                    if let Some((buffer, view)) = tab.buffer_and_view_mut(id) {
                        view.layout_cache.clear();
                        view.set_cursor_range_anchored(buffer, selection);
                        view.set_pending_cursor_range_anchored(buffer, selection);
                        view.request_cursor_reveal(CursorRevealMode::KeepVisible);
                    }
                }
                self.tab_manager.mark_session_dirty();
                crate::app::app_state::search_runtime::mark_search_dirty(self);
                self.note_settings_toml_edit(index);
            }
            Err(prepared) => self.recover_prepared_text(*prepared),
        }
    }
    fn recover_prepared_text(&mut self, prepared: PreparedText) {
        let mut buffer = BufferState::with_document_text_metadata_refresh_state(
            "Recovered prepared text".into(),
            prepared.document,
            None,
            prepared.format,
            prepared.metadata,
            false,
        );
        buffer.is_dirty = true;
        crate::app::app_state::workspace::lifecycle::append_created_tab(
            self,
            WorkspaceTab::new(buffer),
        );
        self.state.status.set_warning_status_in_domain(
            super::super::StatusDomain::File,
            "The target changed while preparing text. Kept the prepared result in a new tab.",
        );
    }
}

impl ScratchpadApp {
    /// egui clones RawInput text at begin-pass. Move large editor payloads to
    /// preparation before that boundary, while preserving other widgets' input.
    pub(crate) fn intercept_bulk_text_input(
        &mut self,
        ctx: &egui::Context,
        input: &mut egui::RawInput,
    ) {
        if self.state.persistence.session_reset.is_some() {
            if let Some(tab) = self.tab_manager.active_tab_mut() {
                let id = tab.layout.active_view_id();
                let editor = crate::app::ui::widget_ids::surface_role(
                    ("native_editor", id),
                    crate::app::ui::widget_ids::WidgetRole::TextEdit,
                );
                if ctx.memory(|memory| memory.has_focus(editor))
                    && let Some((_, view)) = tab.buffer_and_view_mut(id)
                {
                    let mut remaining = Vec::new();
                    for event in std::mem::take(&mut input.events) {
                        match event {
                            egui::Event::Text(_)
                            | egui::Event::Paste(_)
                            | egui::Event::Copy
                            | egui::Event::Cut
                            | egui::Event::Ime(_)
                            | egui::Event::Key { pressed: true, .. } => {
                                view.deferred_editor_events.push(event);
                            }
                            _ => remaining.push(event),
                        }
                    }
                    input.events = remaining;
                }
            }
            return;
        }
        let bulk=input.events.iter().any(|event|matches!(event,egui::Event::Paste(text)|egui::Event::Text(text)|egui::Event::Ime(egui::ImeEvent::Commit(text)) if text.len()>=crate::app::domain::buffer::BULK_TEXT_BYTES));
        if !bulk
            || self.state.dialogs.any_modal_open()
            || crate::app::app_state::settings_state::showing_settings(self)
            || crate::app::app_state::workspace::accessors::pending_action(self).is_some()
        {
            return;
        }
        // Pointer focus changes and global shortcuts keep their normal routing.
        if input.events.iter().any(|event| match event {
            egui::Event::PointerButton { pressed: true, .. } => true,
            egui::Event::Key {
                key,
                modifiers,
                pressed: true,
                ..
            } => {
                let paste = (*key == egui::Key::V && (modifiers.command || modifiers.ctrl))
                    || (*key == egui::Key::Insert && modifiers.shift);
                !paste && crate::app::shortcut_tooltips::is_app_shortcut(ctx, *modifiers, *key)
            }
            _ => false,
        }) {
            return;
        }
        let Some(tab) = self.tab_manager.active_tab_mut() else {
            return;
        };
        let id = tab.layout.active_view_id();
        let editor_id = crate::app::ui::widget_ids::surface_role(
            ("native_editor", id),
            crate::app::ui::widget_ids::WidgetRole::TextEdit,
        );
        if !ctx.memory(|memory| memory.has_focus(editor_id)) {
            return;
        }
        let Some((_, view)) = tab.buffer_and_view_mut(id) else {
            return;
        };
        let mut remaining = Vec::new();
        for event in std::mem::take(&mut input.events) {
            match event {
                egui::Event::Text(_)
                | egui::Event::Paste(_)
                | egui::Event::Copy
                | egui::Event::Cut
                | egui::Event::Ime(egui::ImeEvent::Preedit { .. } | egui::ImeEvent::Commit(_))
                | egui::Event::Key { pressed: true, .. } => view.deferred_editor_events.push(event),
                _ => remaining.push(event),
            }
        }
        input.events = remaining;
        ctx.request_repaint();
    }
}
