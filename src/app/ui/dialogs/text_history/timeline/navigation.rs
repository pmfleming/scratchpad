use super::{RAIL_X, ROW_HEIGHT, ScratchpadApp, State, TimelineId, egui, theme, widget_ids};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum End {
    Newest,
    Oldest,
}

pub(super) struct Now {
    pub id: TimelineId,
    baseline: Option<&'static str>,
}

pub(super) fn active_now(app: &ScratchpadApp) -> Option<Now> {
    if app
        .tab_manager
        .cold_session_tabs()
        .contains_key(&app.tab_manager.active_tab_index)
    {
        return None;
    }
    let buffer = app.tab_manager.active_tab()?.active_buffer();
    if buffer.is_loading_preview {
        return None;
    }
    let document = buffer.document();
    let node = document.history_now();
    let baseline = if node == 0 {
        Some("Empty document")
    } else {
        document
            .history_entries()
            .binary_search_by_key(&node, |e| e.id)
            .ok()
            .and_then(|i| {
                (document.history_entries()[i].summary == "Initial contents")
                    .then_some("Initial contents")
            })
    };
    Some(Now {
        id: TimelineId::Edit {
            document: buffer.temp_id.clone(),
            node,
        },
        baseline,
    })
}

pub(super) fn header(
    ui: &mut egui::Ui,
    app: &ScratchpadApp,
    state: &mut State,
    now: Option<&Now>,
) -> egui::Rect {
    ui.horizontal(|ui| {
        let end = end_button(ui, state, End::Newest);
        now_button(ui, state, now);
        if state.pending.is_some() {
            ui.spinner();
            ui.label("Loading history…");
        }
        if let Some(error) = state.error.clone() {
            ui.colored_label(ui.visuals().error_fg_color, error);
            if ui.button("Retry").clicked() {
                state.fetch(
                    app.state.persistence.session_store.clone(),
                    ui.ctx().clone(),
                );
            }
        }
        end
    })
    .inner
}

pub(super) fn end_button(ui: &mut egui::Ui, state: &mut State, end: End) -> egui::Rect {
    let (icon, label) = match end {
        End::Newest => (
            egui_phosphor::regular::CARET_DOUBLE_UP,
            "Go to latest event",
        ),
        End::Oldest => (
            egui_phosphor::regular::CARET_DOUBLE_DOWN,
            "Go to oldest event",
        ),
    };
    let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(RAIL_X * 2.0, 26.0));
    let enabled = !state.rows.is_empty();
    let response = widget_ids::interact(
        ui,
        rect,
        egui::Id::unique(("timeline.end", end)),
        if enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        },
        "timeline-end",
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    if response.hovered() && enabled {
        ui.painter().rect_filled(rect, 0, theme::action_bg(ui));
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui_phosphor::font_id(16.0),
        if enabled {
            theme::tab_selected_accent(ui)
        } else {
            theme::text_muted(ui)
        },
    );
    if response.on_hover_text(label).clicked() && enabled {
        state.jump_end = Some(end);
        state.anchor = None;
        state.reveal_now = None;
        state.scroll_to_selection = false;
        if end == End::Oldest {
            state.rebuild();
        }
        ui.ctx().request_repaint();
    }
    rect
}

fn index(state: &State, id: &TimelineId) -> Option<usize> {
    state.rows.iter().position(|row| row.id() == *id)
}

pub(super) fn now_button(ui: &mut egui::Ui, state: &mut State, now: Option<&Now>) {
    let Some(now) = now else {
        return;
    };
    if let Some(baseline) = now.baseline {
        ui.label(
            egui::RichText::new(format!("Now: {baseline}"))
                .size(11.0)
                .weak(),
        );
        return;
    }
    let visible = visible(state, &now.id);
    // A search can exclude the active file. Do not offer a jump to a missing row.
    let available = state.live.contains(&now.id);
    if !visible && available && ui.button("Go to Now").clicked() {
        state.jump_end = None;
        state.reveal_now = Some(now.id.clone());
        state.anchor = Some((now.id.clone(), 0.0));
        state.scroll_to_selection = false;
        state.rebuild();
        ui.ctx().request_repaint();
    }
}

pub(super) fn reveal(
    ctx: &egui::Context,
    app: &ScratchpadApp,
    state: &mut State,
    now: Option<&Now>,
) {
    let Some(target) = state.reveal_now.clone() else {
        return;
    };
    if now.is_none_or(|now| now.id != target) {
        state.reveal_now = None;
        return;
    }
    if index(state, &target).is_none()
        && state.next.is_some()
        && state.pending.is_none()
        && state.error.is_none()
    {
        state.limit += super::TIMELINE_PAGE_SIZE;
        state.fetch(app.state.persistence.session_store.clone(), ctx.clone());
    }
}

fn visible(state: &State, target: &TimelineId) -> bool {
    index(state, target).is_some_and(|i| {
        let top = i as f32 * ROW_HEIGHT;
        top >= state.offset - 0.5 && top + ROW_HEIGHT <= state.offset + state.viewport_height + 0.5
    })
}

pub(super) fn scroll_target(ui: &egui::Ui, state: &State, first: usize) {
    let (target, align) = if let Some(end) = state.jump_end {
        match end {
            End::Newest => (0, egui::Align::Min),
            End::Oldest => (state.rows.len().saturating_sub(1), egui::Align::Max),
        }
    } else if let Some(index) = state.reveal_now.as_ref().and_then(|id| index(state, id)) {
        (index, egui::Align::Center)
    } else {
        return;
    };
    // Request against the current content bounds. Overriding an offset beyond
    // last frame's bounds invokes rubber-banding while a page is being added.
    let top = ui.cursor().top() + (target as f32 - first as f32) * ROW_HEIGHT;
    let rect = egui::Rect::from_min_size(
        egui::pos2(ui.max_rect().left(), top),
        egui::vec2(ui.available_width(), ROW_HEIGHT),
    );
    ui.scroll_to_rect_animation(rect, Some(align), egui::style::ScrollAnimation::none());
}

pub(super) fn finish_scroll(state: &mut State) {
    if state
        .reveal_now
        .as_ref()
        .is_some_and(|id| visible(state, id))
    {
        state.reveal_now = None;
    }
    let reached = match state.jump_end {
        Some(End::Newest) => state.offset <= 0.5,
        Some(End::Oldest) => {
            state.fetched
                && state.next.is_none()
                && state.pending.is_none()
                && state
                    .rows
                    .last()
                    .is_none_or(|row| visible(state, &row.id()))
        }
        None => false,
    };
    if reached {
        state.limit = state.limit.max(state.rows.len());
        state.jump_end = None;
    }
}
