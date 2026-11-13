use super::{
    ScratchpadApp, SettingsUi, TabListPosition, egui,
    widgets::{dropdown, group_heading, inner_select_row, toggle_select_row},
};
use crate::app::{
    app_state::settings_controller as settings, services::settings_store::NewTabPlacement,
};

const DELAYS: [f32; 13] = [
    0.1, 0.3, 0.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0,
];
const POSITIONS: [TabListPosition; 4] = [
    TabListPosition::Top,
    TabListPosition::Bottom,
    TabListPosition::Left,
    TabListPosition::Right,
];
const PLACEMENTS: [NewTabPlacement; 4] = [
    NewTabPlacement::Start,
    NewTabPlacement::End,
    NewTabPlacement::BeforeSelection,
    NewTabPlacement::AfterSelection,
];
const POSITION_CHOICE_SIZE: egui::Vec2 = egui::vec2(62.0, 54.0);
const POSITION_CHOICE_GAP: f32 = 4.0;

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
    if app.state.app_settings.auto_hide_tab_list()
        || super::search::is_highlighted(ui, "Hide after")
    {
        hide_delay_row(ui, app);
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
    new_tab_placement_row(ui, app);
    toggle_select_row(
        ui,
        "Show recent files",
        None,
        "settings.recent_files",
        app.state.app_settings.recent_files_enabled(),
        |value| settings::set_recent_files_enabled(app, value),
    );
}

fn hide_delay_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let mut delay = app.state.app_settings.tab_list_auto_hide_delay_seconds();
    inner_select_row(ui, "Hide after", None, |ui| {
        let width = 96.0 * SettingsUi::scale(ui).max(1.0);
        let label = |delay| format!("{delay}");
        if dropdown(ui, "Hide after", &mut delay, &DELAYS, label, width, "s") {
            settings::set_tab_list_auto_hide_delay_seconds(app, delay);
        }
    });
}

fn new_tab_placement_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let mut placement = app.state.app_settings.new_tab_placement();
    inner_select_row(ui, "Place new tabs", None, |ui| {
        let width = 210.0 * SettingsUi::scale(ui).max(1.0);
        let label = |value| placement_label(value).into();
        if dropdown(
            ui,
            "Place new tabs",
            &mut placement,
            &PLACEMENTS,
            label,
            width,
            "",
        ) {
            settings::set_new_tab_placement(app, placement);
        }
    });
}

fn placement_label(placement: NewTabPlacement) -> &'static str {
    match placement {
        NewTabPlacement::Start => "At the start",
        NewTabPlacement::End => "At the end",
        NewTabPlacement::BeforeSelection => "Before active tab",
        NewTabPlacement::AfterSelection => "After active tab",
    }
}

/// Large UI scales use a plain segmented row; otherwise each choice shows a
/// small window with the tab list drawn on the matching side.
fn tab_position_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let mut selected = app.state.app_settings.tab_list_position();
    if SettingsUi::scale(ui) > 1.3 {
        let options = POSITIONS.map(|position| (position, position.label()));
        if super::widgets::segmented_row(ui, "Tabs", &mut selected, &options) {
            settings::set_tab_list_position(app, selected);
        }
        return;
    }
    inner_select_row(ui, "Tabs", None, |ui| {
        let lane = ui.max_rect();
        let width = 4.0 * POSITION_CHOICE_SIZE.x + 3.0 * POSITION_CHOICE_GAP;
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
                ui.spacing_mut().item_spacing.x = POSITION_CHOICE_GAP;
                for position in POSITIONS {
                    if tab_position_choice(ui, position, position == selected) {
                        settings::set_tab_list_position(app, position);
                    }
                }
            },
        );
    });
}

fn tab_position_choice(ui: &mut egui::Ui, position: TabListPosition, selected: bool) -> bool {
    let response = ui.add_sized(
        POSITION_CHOICE_SIZE,
        egui::Button::new("").selected(selected),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::RadioButton, true, selected, position.label())
    });
    let window = egui::Rect::from_center_size(
        response.rect.center_top() + egui::vec2(0.0, 17.0),
        egui::vec2(28.0, 20.0),
    );
    let color = super::text_muted(ui);
    let painter = ui.painter();
    painter.rect_stroke(
        window,
        3.0,
        egui::Stroke::new(1.0, color),
        egui::StrokeKind::Inside,
    );
    painter.rect_filled(tab_list_bar(window, position), 1.0, color);
    painter.text(
        response.rect.center_bottom() - egui::vec2(0.0, 5.0),
        egui::Align2::CENTER_BOTTOM,
        position.label(),
        SettingsUi::font(ui, 12.0),
        super::text_primary(ui),
    );
    response.clicked()
}

/// The tab list's bar inside the miniature `window`, on `position`'s side.
fn tab_list_bar(window: egui::Rect, position: TabListPosition) -> egui::Rect {
    let (corner, size) = match position {
        TabListPosition::Top => (window.min + egui::vec2(3.0, 3.0), egui::vec2(22.0, 4.0)),
        TabListPosition::Bottom => (
            window.left_bottom() + egui::vec2(3.0, -7.0),
            egui::vec2(22.0, 4.0),
        ),
        TabListPosition::Left => (window.min + egui::vec2(3.0, 3.0), egui::vec2(5.0, 14.0)),
        TabListPosition::Right => (
            window.right_top() + egui::vec2(-8.0, 3.0),
            egui::vec2(5.0, 14.0),
        ),
    };
    egui::Rect::from_min_size(corner, size)
}
