use super::{BackgroundIoRequest, PendingActionKind, PendingBackgroundAction, ScratchpadApp};
use crate::app::domain::{BufferState, CursorRevealMode, WorkspaceTab, buffer::PreparedText};
use eframe::egui;

impl ScratchpadApp {
    pub(crate) fn queue_text_preparations(&mut self) {
        let mut pending = false;
        for tab in self.tab_manager.tabs.as_mut_slice() {
            for buffer in tab.buffers_mut() {
                pending |= buffer.is_preparing_text();
                let Some(request) = buffer.take_text_preparation() else {
                    continue;
                };
                let buffer_id = buffer.id;
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
        if !prepared.changes_document {
            if let Some(index) = location
                && let Some(buffer) =
                    self.tab_manager.tabs.as_mut_slice()[index].buffer_by_id_mut(buffer_id)
            {
                buffer.cancel_text_preparation();
            }
            return;
        }
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
        let resetting = self.state.persistence.session_reset.is_some();
        if !resetting
            && (!input.events.iter().any(|event| {
                event_text(event).len() >= crate::app::domain::buffer::BULK_TEXT_BYTES
            }) || self.state.dialogs.any_modal_open()
                || crate::app::app_state::settings_state::showing_settings(self)
                || crate::app::app_state::workspace::accessors::pending_action(self).is_some()
                || input
                    .events
                    .iter()
                    .any(|event| changes_input_target(ctx, event)))
        {
            return;
        }
        let store = self.state.persistence.session_store.opened_chunk_store();
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
        let Some((buffer, view)) = tab.buffer_and_view_mut(id) else {
            return;
        };
        defer_editor_events(input, buffer, view, resetting, store.as_ref());
        if !resetting {
            ctx.request_repaint();
        }
    }
}

fn event_text(event: &egui::Event) -> &str {
    match event {
        egui::Event::Text(text)
        | egui::Event::Paste(text)
        | egui::Event::Ime(egui::ImeEvent::Commit(text)) => text,
        _ => "",
    }
}

fn changes_input_target(ctx: &egui::Context, event: &egui::Event) -> bool {
    match event {
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
    }
}

fn deferred_event(event: &egui::Event, resetting: bool) -> bool {
    match event {
        egui::Event::Text(_)
        | egui::Event::Paste(_)
        | egui::Event::Copy
        | egui::Event::Cut
        | egui::Event::Key { pressed: true, .. } => true,
        egui::Event::Ime(ime) => {
            resetting
                || matches!(
                    ime,
                    egui::ImeEvent::Preedit { .. } | egui::ImeEvent::Commit(_)
                )
        }
        _ => false,
    }
}

fn defer_editor_events(
    input: &mut egui::RawInput,
    buffer: &mut BufferState,
    view: &mut crate::app::domain::EditorViewState,
    resetting: bool,
    store: Option<&crate::app::services::chunk_store::ChunkStoreHandle>,
) {
    let mut accepted = 0usize;
    for event in input
        .events
        .extract_if(.., |event| deferred_event(event, resetting))
    {
        let bytes = event_text(&event).len();
        if !resetting
            && bytes >= crate::app::domain::buffer::BULK_TEXT_BYTES
            && let Some(store) = store
            && let Err(cause) = store.check_bulk_capacity(accepted.saturating_add(bytes))
        {
            buffer.storage_refusal = Some(format!("Large paste refused. {cause}"));
            continue;
        }
        accepted = accepted.saturating_add(bytes);
        view.deferred_editor_events.push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::{BufferState, defer_editor_events, egui};
    use crate::app::domain::{EditorViewState, buffer::BULK_TEXT_BYTES};
    use crate::app::services::chunk_store::ChunkStoreHandle;

    #[test]
    fn deferred_input_preserves_order_and_widget_events_while_enforcing_admission() {
        for resetting in [false, true] {
            for refusing in [false, true] {
                let store = ChunkStoreHandle::unavailable("test unavailable".into());
                store.set_failure_limit(1);
                let mut buffer = BufferState::new("input".into(), "base".into(), None);
                let mut view = EditorViewState::new(buffer.id);
                let surrounding = egui::Event::Ime(egui::ImeEvent::DeleteSurrounding {
                    before_chars: 1,
                    after_chars: 0,
                });
                let pointer = egui::Event::PointerMoved(egui::Pos2::ZERO);
                let mut input = egui::RawInput {
                    events: vec![
                        egui::Event::Copy,
                        pointer.clone(),
                        egui::Event::Paste("λ".repeat(BULK_TEXT_BYTES / 2)),
                        egui::Event::Ime(egui::ImeEvent::Preedit {
                            text: "候".into(),
                            active_range_chars: None,
                        }),
                        egui::Event::Ime(egui::ImeEvent::Commit("補".into())),
                        surrounding.clone(),
                        egui::Event::Cut,
                        egui::Event::Text("typed".into()),
                    ],
                    ..Default::default()
                };
                defer_editor_events(
                    &mut input,
                    &mut buffer,
                    &mut view,
                    resetting,
                    refusing.then_some(&store),
                );
                assert_eq!(
                    input.events,
                    if resetting {
                        vec![pointer]
                    } else {
                        vec![pointer, surrounding]
                    }
                );
                let events = &view.deferred_editor_events;
                let rejected = refusing && !resetting;
                assert_eq!(buffer.storage_refusal.is_some(), rejected);
                assert_eq!(
                    events.len(),
                    6 + usize::from(resetting) - usize::from(rejected)
                );
                assert_eq!(events.first(), Some(&egui::Event::Copy));
                assert_eq!(
                    &events[events.len() - 2..],
                    &[egui::Event::Cut, egui::Event::Text("typed".into())]
                );
                assert_eq!(
                    events
                        .iter()
                        .filter(|event| matches!(event, egui::Event::Paste(_)))
                        .count(),
                    usize::from(!rejected)
                );
                assert_eq!(buffer.text(), "base");
            }
        }
    }
}
