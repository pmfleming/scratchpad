use super::{HistoryPreview, Preview, Target};
use crate::app::{
    app_state::{ScratchpadApp, StatusDomain},
    domain::{TextDocument, buffer::DocumentRevision},
};
use eframe::egui;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct Pending {
    buffer_id: u64,
    version_id: i64,
    revision: DocumentRevision,
    history_revision: u64,
    confirm: bool,
}

fn key() -> egui::Id {
    egui::Id::unique("history.preview.snapshot")
}

pub(super) fn cancel(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<Pending>(key()));
}

pub(in super::super) fn confirming(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Pending>(key()))
        .is_some_and(|p| p.confirm)
}

pub(super) fn request(
    ctx: &egui::Context,
    app: &mut ScratchpadApp,
    id: u64,
    version_id: i64,
    confirm: bool,
) {
    if let Some(mut pending) = ctx.data(|d| d.get_temp::<Pending>(key())) {
        if pending.buffer_id == id && pending.version_id == version_id {
            pending.confirm |= confirm;
            ctx.data_mut(|d| d.insert_temp(key(), pending));
            return;
        }
        if pending.confirm && !confirm {
            return;
        }
    }
    if super::snapshot_id(ctx, id) == Some(version_id) {
        if confirm {
            super::commit_snapshot(ctx, app);
        }
        return;
    }
    let Some(buffer) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.buffer_by_id(id))
    else {
        return;
    };
    let pending = Pending {
        buffer_id: id,
        version_id,
        revision: buffer.document().revision_key(),
        history_revision: buffer.document().history_revision_counter(),
        confirm,
    };
    ctx.data_mut(|d| d.insert_temp(key(), pending));
    ctx.request_repaint();
}

pub(in super::super) fn poll(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let Some(pending) = ui.ctx().data(|d| d.get_temp::<Pending>(key())) else {
        return;
    };
    let Some(buffer) = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .find_map(|tab| tab.buffer_by_id(pending.buffer_id))
    else {
        cancel(ui.ctx());
        return;
    };
    if buffer.document().revision_key() != pending.revision
        || buffer.document().history_revision_counter() != pending.history_revision
    {
        cancel(ui.ctx());
        return;
    }
    let store = app.state.persistence.session_store.clone();
    let temp_id = buffer.temp_id.clone();
    let version_id = pending.version_id;
    let result = crate::app::ui::recovery::query(
        ui,
        (
            "history.preview.snapshot",
            store.root().to_owned(),
            temp_id.clone(),
            version_id,
        ),
        move || store.read_history_version(temp_id, version_id),
    );
    let Some(result) = result else {
        return;
    };
    cancel(ui.ctx());
    match result {
        Err(error) => app.state.status.set_error_status_in_domain(
            StatusDomain::History,
            format!("Could not preview snapshot: {error}"),
        ),
        Ok(result) => {
            let mut projection = buffer.clone();
            projection.install_history_document(TextDocument::with_preferred_line_ending(
                result.text.clone(),
                buffer.format.preferred_line_ending_style(),
            ));
            projection.mark_as_loading_preview();
            let preview = Preview {
                buffer_id: buffer.id,
                revision: pending.revision,
                history_revision: pending.history_revision,
                target: Target::Snapshot(version_id),
                tab: Arc::new(Mutex::new(HistoryPreview::new(projection))),
            };
            ui.ctx().data_mut(|d| d.insert_temp(super::key(), preview));
            if super::super::read_follow_focus(ui.ctx()) {
                super::super::navigation::activate_file(app, pending.buffer_id);
            }
            ui.ctx()
                .memory_mut(|m| m.request_focus(super::super::list::focus_id()));
            super::super::navigation::retain_tool_focus(ui.ctx(), app);
            if pending.confirm {
                super::commit_snapshot(ui.ctx(), app);
            }
            ui.ctx().request_repaint();
        }
    }
}
