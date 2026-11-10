use super::{
    RAIL_X, ROW_HEIGHT, TimelineKind, TimelineRow, egui, presentation, style, theme, widget_ids,
};

#[derive(Default)]
pub(super) struct RowState {
    pub selected: bool,
    pub closed: bool,
    pub current: bool,
    pub previous_time: Option<u64>,
}

pub(super) fn paint_row(ui: &mut egui::Ui, row: &TimelineRow, state: &RowState) -> Option<bool> {
    let selected = state.selected;
    let closed = state.closed;
    let rect = widget_ids::allocate_exact_rect(ui, egui::vec2(ui.available_width(), ROW_HEIGHT));
    if row.node().is_none() {
        let response = widget_ids::interact(
            ui,
            rect,
            egui::Id::unique(("history.timeline.information", row.id())),
            egui::Sense::hover(),
            "timeline-information",
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Label,
                true,
                format!("{} · {}", row.summary, row.name),
            )
        });
        describe_row(ui, &response, row, state);
        paint_labels(ui, row, &response, state);
        response.on_hover_text(format!(
            "{}\n{}\n{}",
            row.summary,
            row.label(),
            presentation::full_time(row.timestamp)
        ));
        return None;
    }
    let response = widget_ids::interact(
        ui,
        rect,
        egui::Id::unique(("history.timeline.row", row.id())),
        egui::Sense::click(),
        "timeline-row",
    );
    let change = row_change(ui, row);
    let title = change
        .as_ref()
        .map_or(row.summary.as_str(), |c| c.title.as_str());
    let label = format!(
        "{} · {}{}",
        title,
        row.name,
        if closed { " · Closed" } else { "" }
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::ListBoxOption, true, selected, &label)
    });
    describe_row(ui, &response, row, state);
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            0,
            if selected {
                theme::tab_selected_bg(ui)
            } else {
                theme::action_bg(ui)
            },
        );
    }
    paint_labels(ui, row, &response, state);
    let right = rect.right() - 80.0;
    let button = widget_ids::scope(ui, ("history.timeline.file", row.id()), |ui| {
        ui.put(
            egui::Rect::from_min_max(egui::pos2(right, rect.top() + 21.0), rect.max),
            egui::Button::new(egui::RichText::new("This file").size(10.5)).frame(false),
        )
    })
    .inner;
    button.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            true,
            format!("Inspect branches for {}", row.name),
        )
    });
    if button.clicked() {
        return Some(true);
    }
    response
        .on_hover_text(format!(
            "{}\n{}\n{}",
            title,
            row.label(),
            presentation::full_time(row.timestamp)
        ))
        .clicked()
        .then_some(false)
}

fn describe_row(ui: &egui::Ui, response: &egui::Response, row: &TimelineRow, state: &RowState) {
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_description(format!(
            "{} · {}{}{}",
            row.label(),
            presentation::full_time(row.timestamp),
            if state.closed { " · Closed" } else { "" },
            if state.current { " · Now" } else { "" }
        ));
    });
}

pub(super) fn row_change(ui: &egui::Ui, row: &TimelineRow) -> Option<presentation::Change> {
    let TimelineKind::Edit(source) = row.kind else {
        return None;
    };
    Some(presentation::edit_change(
        ui,
        source,
        &row.summary,
        row.timestamp,
        row.preview.as_ref(),
    ))
}

fn paint_labels(ui: &egui::Ui, row: &TimelineRow, response: &egui::Response, state: &RowState) {
    let rect = response.rect;
    let closed = state.closed;
    paint_rail(ui, rect, row.node().is_some(), state);
    let change = row_change(ui, row);
    let icon = match row.kind {
        TimelineKind::Edit(_) => change.as_ref().map_or("", |c| c.icon),
        TimelineKind::Activity(
            crate::app::services::session_store::activity::ActivityKind::Created,
        ) => egui_phosphor::regular::FILE_PLUS,
        TimelineKind::Activity(
            crate::app::services::session_store::activity::ActivityKind::Opened,
        ) => egui_phosphor::regular::FOLDER_OPEN,
        TimelineKind::Activity(
            crate::app::services::session_store::activity::ActivityKind::Closed,
        ) => egui_phosphor::regular::X,
        TimelineKind::Activity(_) => egui_phosphor::regular::WARNING,
    };
    style::text(
        ui,
        egui::Rect::from_min_size(rect.min + egui::vec2(26.0, 0.0), egui::vec2(20.0, 25.0)),
        icon,
        14.0,
        change
            .as_ref()
            .map_or_else(|| theme::tab_selected_accent(ui), |c| c.color),
    );
    let right = rect.right() - 80.0;
    let title_right = metadata(ui, row, response, state);
    let title_rect = egui::Rect::from_min_max(
        rect.min + egui::vec2(50.0, 2.0),
        egui::pos2(title_right, rect.top() + 23.0),
    );
    if let Some(change) = change {
        change.paint_title(ui, title_rect, theme::text_primary(ui));
    } else {
        style::text(ui, title_rect, &row.summary, 13.0, theme::text_primary(ui));
    }
    let file = format!("{}{}", row.name, if closed { " · Closed" } else { "" });
    style::text_end(
        ui,
        egui::Rect::from_min_max(
            rect.min + egui::vec2(50.0, 22.0),
            egui::pos2(right, rect.bottom() - 2.0),
        ),
        &file,
        11.0,
        theme::text_muted(ui),
    );
}

fn paint_rail(ui: &egui::Ui, rect: egui::Rect, edit: bool, state: &RowState) {
    let center = egui::pos2(rect.left() + RAIL_X, rect.top() + 12.5);
    let accent = theme::tab_selected_accent(ui);
    let top = rect.top();
    let bottom = rect.bottom();
    ui.painter().line_segment(
        [egui::pos2(center.x, top), egui::pos2(center.x, bottom)],
        egui::Stroke::new(1.5, accent),
    );
    if state.current || state.selected {
        ui.painter().circle(
            center,
            5.0,
            theme::text_primary(ui),
            egui::Stroke::new(2.5, theme::tab_selected_accent(ui)),
        );
    } else {
        ui.painter().circle(
            center,
            3.0,
            if edit {
                accent
            } else {
                ui.visuals().panel_fill
            },
            egui::Stroke::new(1.5, accent),
        );
    }
}

fn metadata(ui: &egui::Ui, row: &TimelineRow, response: &egui::Response, state: &RowState) -> f32 {
    let rect = response.rect;
    let right = rect.right() - 8.0;
    if state.current {
        let tag = egui::Rect::from_center_size(
            egui::pos2(right - 18.0, rect.top() + 12.5),
            egui::vec2(36.0, 17.0),
        );
        style::now_badge(ui, tag);
        widget_ids::interact(
            ui,
            tag,
            response.id.with("now"),
            egui::Sense::hover(),
            "timeline-now",
        )
        .widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Label, true, format!("Now for {}", row.name))
        });
        return right - 44.0;
    }
    let time = presentation::clock(row.timestamp);
    let width = ui
        .painter()
        .layout_no_wrap(time.clone(), style::font(ui, 11.0), theme::text_muted(ui))
        .size()
        .x;
    if presentation::show_time(row.timestamp, state.previous_time, response.hovered()) {
        style::text_right(
            ui,
            egui::Rect::from_min_max(
                egui::pos2(right - width, rect.top()),
                egui::pos2(right, rect.top() + 25.0),
            ),
            &time,
            11.0,
            theme::text_muted(ui),
        );
    }
    right - width - 6.0
}
