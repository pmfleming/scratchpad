use crate::app::app_state::ScratchpadApp;
use crate::app::services::settings_store::{TabListPosition, TabOrderDirection, TabOrderMode};
use crate::app::shortcut_keymap::ShortcutAction;
use crate::app::shortcut_tooltips;
use eframe::egui;
use egui_phosphor::regular::{PUSH_PIN, TABS};

use super::menu_ui::{
    CARET_WIDTH, ICON_CHOICE_BUTTON_SIZE, ORDER_SUBMENU_WIDTH, ROW_HEIGHT, SUBMENU_WIDTH, WIDTH,
    menu_button, order_direction_button, primary_menu_button, selectable_menu_button,
    submenu_button, submenu_button_sized, tab_list_position_icon, tab_order_mode_label,
};

/// "Tab list" with a Pinned │ Auto-hide switch; the caret picks its position.
/// Returns the auto-hide setting the user chose.
pub(super) fn render_tab_list_actions(ui: &mut egui::Ui, app: &mut ScratchpadApp) -> Option<bool> {
    let auto_hide = app.state.app_settings.auto_hide_tab_list();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let rect = crate::app::ui::widget_ids::allocate_exact_rect(
            ui,
            egui::vec2(WIDTH - CARET_WIDTH, ROW_HEIGHT),
        );
        crate::app::ui::panel_style::paint_menu_row(
            ui,
            rect,
            Some(PUSH_PIN),
            "Tab list",
            None,
            true,
        );
        let mut choice = auto_hide;
        crate::app::ui::widget_ids::rect_scope_with_layout(
            ui,
            rect.shrink2(egui::vec2(6.0, 3.0)),
            "tab_context.tab_list_mode",
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                ui.spacing_mut().button_padding = egui::vec2(6.0, 1.0);
                ui.selectable_value(&mut choice, true, "Auto-hide")
                    .on_hover_text(shortcut_tooltips::action(
                        ui.ctx(),
                        ShortcutAction::ToggleTabListAutoHide,
                        "Hide the tab list until the pointer reaches it",
                    ));
                ui.selectable_value(&mut choice, false, "Pinned")
                    .on_hover_text("Always show the tab list");
            },
        );
        render_tab_list_submenu(ui, app);
        (choice != auto_hide).then_some(choice)
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
    if primary_menu_button(ui, "tab_context.order_primary", "Order tabs", TABS) {
        app.set_tab_order_mode(TabOrderMode::FileName);
        ui.close();
    }
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
        let icons_width = ICON_CHOICE_BUTTON_SIZE.x * 2.0 + spacing;
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
                position.label(),
                Some(tab_list_position_icon(position)),
                true,
            ) {
                crate::app::app_state::settings_controller::set_tab_list_position(app, position);
                ui.close();
            }
        }
    });
}
