use super::{
    CategoryCard, ComboSelectRow, ScratchpadApp, TabListPosition, category_card, egui,
    inner_divider, inner_select_row, nearest_option_index, toggle_select_row,
    u32_slider_value_control,
};
use crate::app::services::settings_store::NewTabPlacement;

const AUTO_HIDE_DELAY_OPTIONS: [f32; 13] = [
    0.1, 0.3, 0.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0,
];
const TAB_LIST_POSITIONS: [TabListPosition; 4] = [
    TabListPosition::Top,
    TabListPosition::Bottom,
    TabListPosition::Left,
    TabListPosition::Right,
];
const NEW_TAB_PLACEMENT_OPTIONS: [NewTabPlacement; 4] = [
    NewTabPlacement::Start,
    NewTabPlacement::End,
    NewTabPlacement::AfterSelection,
    NewTabPlacement::BeforeSelection,
];

pub(super) fn render_tab_position_category(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    category_card(
        ui,
        CategoryCard {
            heading: "Tab Position",
            id_source: "settings_tab_position_card",
            icon: egui_phosphor::regular::TEXT_OUTDENT,
            title: "Tab list",
            description: "Tab placement and visibility.",
            default_open: true,
        },
        |ui| {
            render_tab_list_row(ui, app);
            inner_divider(ui);
            render_new_tab_placement_row(ui, app);
            inner_divider(ui);
            render_auto_hide_row(ui, app);
            inner_divider(ui);
            render_auto_hide_delay_row(ui, app);
            inner_divider(ui);
            render_status_bar_row(ui, app);
        },
    );
}

fn render_tab_list_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    super::combo_select_row(
        ui,
        ComboSelectRow {
            label: "Tab list",
            description: Some("Strip or side list."),
            combo_id: "settings_tab_list_position",
            record_label: "combo.Tab list",
            current: app.state.app_settings.tab_list_position(),
            options: &TAB_LIST_POSITIONS,
            selected_label: tab_list_position_label,
            option_label: tab_list_position_label,
            on_change: |position| {
                crate::app::app_state::settings_controller::set_tab_list_position(app, position);
            },
        },
    );
}

fn render_new_tab_placement_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    super::combo_select_row(
        ui,
        ComboSelectRow {
            label: "New tabs",
            description: Some("Placement for new tabs."),
            combo_id: "settings_new_tab_placement",
            record_label: "combo.New tabs",
            current: app.state.app_settings.new_tab_placement(),
            options: &NEW_TAB_PLACEMENT_OPTIONS,
            selected_label: new_tab_placement_pill_label,
            option_label: new_tab_placement_label,
            on_change: |placement| {
                crate::app::app_state::settings_controller::set_new_tab_placement(app, placement);
            },
        },
    );
}

fn tab_list_position_label(position: TabListPosition) -> &'static str {
    match position {
        TabListPosition::Top => "Top",
        TabListPosition::Bottom => "Bottom",
        TabListPosition::Left => "Left",
        TabListPosition::Right => "Right",
    }
}

fn new_tab_placement_label(placement: NewTabPlacement) -> &'static str {
    match placement {
        NewTabPlacement::Start => "Start of list",
        NewTabPlacement::End => "End of list",
        NewTabPlacement::BeforeSelection => "Before selection",
        NewTabPlacement::AfterSelection => "After selection",
    }
}

fn new_tab_placement_pill_label(placement: NewTabPlacement) -> &'static str {
    match placement {
        NewTabPlacement::Start => "Start",
        NewTabPlacement::End => "End",
        NewTabPlacement::BeforeSelection => "Before",
        NewTabPlacement::AfterSelection => "After",
    }
}

fn render_auto_hide_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    toggle_select_row(
        ui,
        "Auto-hide tab list",
        Some("Collapse until pointer is near."),
        "settings.workspace.auto_hide_tab_list",
        app.state.app_settings.auto_hide_tab_list(),
        |enabled| crate::app::app_state::settings_controller::set_auto_hide_tab_list(app, enabled),
    );
}

fn render_auto_hide_delay_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    inner_select_row(
        ui,
        "Auto-hide delay",
        Some("Grace period before collapse."),
        |ui| {
            let current_index = nearest_option_index(
                app.state.app_settings.tab_list_auto_hide_delay_seconds(),
                &AUTO_HIDE_DELAY_OPTIONS,
                |seconds| seconds,
            );
            let mut selected_index = current_index as u32;
            let delay_label =
                auto_hide_delay_label(AUTO_HIDE_DELAY_OPTIONS[selected_index as usize]);
            u32_slider_value_control(
                ui,
                "settings.auto_hide_delay.slider",
                "slider.Auto-hide delay",
                &mut selected_index,
                0..=(AUTO_HIDE_DELAY_OPTIONS.len() - 1) as u32,
                52.0,
                delay_label,
            );

            if selected_index as usize != current_index {
                crate::app::app_state::settings_controller::set_tab_list_auto_hide_delay_seconds(
                    app,
                    AUTO_HIDE_DELAY_OPTIONS[selected_index as usize],
                );
            }
        },
    );
}

fn render_status_bar_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let ctx = ui.ctx().clone();
    toggle_select_row(
        ui,
        "Status bar",
        Some("Show the bottom status strip."),
        "settings.ui.status_bar_visible",
        app.state.app_settings.status_bar_visible(),
        |visible| {
            crate::app::app_state::settings_controller::defer_status_bar_visible(
                app, visible, &ctx,
            );
        },
    );
}

fn auto_hide_delay_label(seconds: f32) -> String {
    if seconds.fract().abs() < f32::EPSILON {
        format!("{seconds:.0} s")
    } else {
        format!("{seconds:.1} s")
    }
}
