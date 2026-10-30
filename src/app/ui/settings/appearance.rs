use super::{
    ScratchpadApp, TabListPosition, egui,
    widgets::{DropdownPlacement, dropdown, group_heading, inner_select_row, toggle_select_row},
};
use crate::app::{
    app_state::settings_controller as settings, services::settings_store::NewTabPlacement,
};

const DELAYS: [f32; 13] = [
    0.1, 0.3, 0.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0,
];

pub(super) fn render_workspace(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    tab_position_row(ui, app);
    toggle_select_row(
        ui,
        "Auto-hide tabs",
        None,
        "settings.auto_hide",
        app.state.app_settings.auto_hide_tab_list(),
        |value| settings::set_auto_hide_tab_list(app, value),
    );
    if app.state.app_settings.auto_hide_tab_list() {
        let mut delay = app.state.app_settings.tab_list_auto_hide_delay_seconds();
        inner_select_row(ui, "Hide after", None, |ui| {
            if dropdown(
                ui,
                "Hide after",
                &mut delay,
                &DELAYS,
                |delay| format!("{delay}"),
                DropdownPlacement::Trailing(96.0),
                "s",
            ) {
                settings::set_tab_list_auto_hide_delay_seconds(app, delay);
            }
        });
    }
    let ctx = ui.ctx().clone();
    toggle_select_row(
        ui,
        "Status bar",
        None,
        "settings.status_bar",
        app.state.app_settings.status_bar_visible(),
        |value| settings::defer_status_bar_visible(app, value, &ctx),
    );
    group_heading(ui, "Opening files");
    super::opening::render_opening(ui, app);
    let mut placement = app.state.app_settings.new_tab_placement();
    inner_select_row(ui, "Place new tabs", None, |ui| {
        if dropdown(
            ui,
            "Place new tabs",
            &mut placement,
            &[
                NewTabPlacement::Start,
                NewTabPlacement::End,
                NewTabPlacement::BeforeSelection,
                NewTabPlacement::AfterSelection,
            ],
            |value| {
                match value {
                    NewTabPlacement::Start => "At the start",
                    NewTabPlacement::End => "At the end",
                    NewTabPlacement::BeforeSelection => "Before active tab",
                    NewTabPlacement::AfterSelection => "After active tab",
                }
                .into()
            },
            DropdownPlacement::Trailing(210.0),
            "",
        ) {
            settings::set_new_tab_placement(app, placement);
        }
    });
    toggle_select_row(
        ui,
        "Show recent files",
        None,
        "settings.recent_files",
        app.state.app_settings.recent_files_enabled(),
        |value| settings::set_recent_files_enabled(app, value),
    );
}

fn tab_position_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    inner_select_row(ui, "Tabs", None, |ui| {
        let selected = app.state.app_settings.tab_list_position();
        let lane = ui.max_rect();
        let width: f32 = 4.0 * 62.0 + 3.0 * 4.0;
        let rect = egui::Rect::from_min_max(
            egui::pos2(lane.right() - width.min(lane.width()), lane.top()),
            lane.max,
        );
        super::widget_ids::rect_scope_with_layout(
            ui,
            rect,
            "tab_position_choices",
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for position in [
                    TabListPosition::Top,
                    TabListPosition::Bottom,
                    TabListPosition::Left,
                    TabListPosition::Right,
                ] {
                    let response = ui.add_sized(
                        egui::vec2(62.0, 54.0),
                        egui::Button::new("").selected(position == selected),
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::Role::RadioButton,
                            true,
                            selected == position,
                            position.label(),
                        )
                    });
                    let rect = egui::Rect::from_center_size(
                        response.rect.center_top() + egui::vec2(0.0, 17.0),
                        egui::vec2(28.0, 20.0),
                    );
                    let color = super::text_muted(ui);
                    ui.painter().rect_stroke(
                        rect,
                        3.0,
                        egui::Stroke::new(1.0, color),
                        egui::StrokeKind::Inside,
                    );
                    let fill = match position {
                        TabListPosition::Top => egui::Rect::from_min_size(
                            rect.min + egui::vec2(3.0, 3.0),
                            egui::vec2(22.0, 4.0),
                        ),
                        TabListPosition::Bottom => egui::Rect::from_min_size(
                            rect.left_bottom() + egui::vec2(3.0, -7.0),
                            egui::vec2(22.0, 4.0),
                        ),
                        TabListPosition::Left => egui::Rect::from_min_size(
                            rect.min + egui::vec2(3.0, 3.0),
                            egui::vec2(5.0, 14.0),
                        ),
                        TabListPosition::Right => egui::Rect::from_min_size(
                            rect.right_top() + egui::vec2(-8.0, 3.0),
                            egui::vec2(5.0, 14.0),
                        ),
                    };
                    ui.painter().rect_filled(fill, 1.0, color);
                    ui.painter().text(
                        response.rect.center_bottom() - egui::vec2(0.0, 5.0),
                        egui::Align2::CENTER_BOTTOM,
                        position.label(),
                        egui::FontId::proportional(12.0),
                        super::text_primary(ui),
                    );
                    if response.clicked() {
                        settings::set_tab_list_position(app, position);
                    }
                }
            },
        );
    });
}
