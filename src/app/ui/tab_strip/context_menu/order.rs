use crate::app::app_state::ScratchpadApp;
use crate::app::services::settings_store::{TabListPosition, TabOrderDirection, TabOrderMode};
use eframe::egui;
use egui_phosphor::regular::TABS;

use super::menu_ui::{
    ORDER_DIRECTION_BUTTON_SIZE, ORDER_SUBMENU_WIDTH, SUBMENU_WIDTH, menu_button,
    order_direction_button, primary_menu_button, selectable_menu_button, submenu_button,
    submenu_button_sized, tab_list_position_icon, tab_list_position_label, tab_order_mode_label,
};

pub(super) fn render_tab_list_actions(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    toggle_label: &str,
    toggle_icon: &str,
) -> bool {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        let toggle_clicked = render_tab_list_primary_button(ui, toggle_label, toggle_icon);
        render_tab_list_submenu(ui, app);

        toggle_clicked
    })
    .inner
}

pub(super) fn render_tab_order_submenu(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        render_tab_order_primary_button(ui, app);
        render_tab_order_caret(ui, app);
    });
}

fn render_tab_order_primary_button(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    if primary_menu_button(ui, "tab_context.order_primary", "Order Tabs", TABS) {
        app.set_tab_order_mode(TabOrderMode::FileName);
        ui.close();
    }
}

fn render_tab_list_primary_button(ui: &mut egui::Ui, label: &str, icon: &str) -> bool {
    primary_menu_button(ui, ("tab_context.tab_list_primary", label), label, icon)
}

fn render_tab_order_caret(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    submenu_button_sized(ui, "tab_context.order_caret", ORDER_SUBMENU_WIDTH, |ui| {
        render_tab_order_direction_controls(ui, app);
        ui.separator();
        for mode in [
            TabOrderMode::Custom,
            TabOrderMode::FileName,
            TabOrderMode::FileSize,
            TabOrderMode::FileAge,
            TabOrderMode::RecentEdit,
        ] {
            let selected = app.state.app_settings.tab_order_mode() == mode;
            if selectable_menu_button(
                ui,
                ORDER_SUBMENU_WIDTH,
                tab_order_mode_label(mode),
                selected,
            ) {
                app.set_tab_order_mode(mode);
                ui.close();
            }
        }
    });
}

fn render_tab_order_direction_controls(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    ui.horizontal(|ui| {
        let spacing = 4.0;
        ui.spacing_mut().item_spacing.x = spacing;
        let icons_width = ORDER_DIRECTION_BUTTON_SIZE.x * 2.0 + spacing;
        ui.add_space(((ui.available_width() - icons_width) * 0.5).max(0.0));
        for direction in [TabOrderDirection::Ascending, TabOrderDirection::Descending] {
            let selected = app.state.app_settings.tab_order_direction() == direction;
            if order_direction_button(ui, direction, selected) {
                app.set_tab_order_direction(direction);
            }
        }
    });
}

fn render_tab_list_submenu(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    submenu_button(ui, "tab_context.tab_list_caret", |ui| {
        for position in [
            TabListPosition::Top,
            TabListPosition::Bottom,
            TabListPosition::Left,
            TabListPosition::Right,
        ] {
            if menu_button(
                ui,
                SUBMENU_WIDTH,
                tab_list_position_label(position),
                Some(tab_list_position_icon(position)),
                true,
            ) {
                crate::app::app_state::settings_controller::set_tab_list_position(app, position);
                ui.close();
            }
        }
    });
}
