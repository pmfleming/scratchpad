use crate::app::app_state::ScratchpadApp;
use crate::app::chrome::phosphor_button;
use crate::app::fonts::{EDITOR_FONT_FAMILY, EditorFontPreset, EditorFontSource};
use crate::app::services::settings_store::{AppThemeMode, EditorAppearanceSource, TabListPosition};
use crate::app::theme::{action_bg, action_hover_bg, border, text_muted, text_primary};
use crate::app::ui::widget_ids;
use eframe::egui;

mod appearance;
mod opening;
mod sections;
mod style;
mod text_formatting;
mod widgets;

use sections::render_settings_categories;
use style::SettingsUi;
use widgets::{
    CategoryCard, ComboSelectRow, action_card, available_width_control, card_header, category_card,
    combo_control, combo_select_row, expandable_card, inner_divider, inner_select_row,
    nearest_option_index, radio_option_row, record_settings_control_box, render_preview_panel,
    settings_card_frame, settings_file_card, slider_value_control, toggle_card, toggle_select_row,
    u32_slider_value_control,
};

const FONT_SIZE_OPTIONS: [u32; 9] = [11, 12, 14, 16, 18, 20, 24, 28, 32];
pub(crate) const PREVIEW_QUOTES: [(&str, &str); 3] = [
    (
        "A man of genius makes no mistakes. His errors are volitional and are the portals of discovery.",
        "His errors are volitional",
    ),
    (
        "To learn one must be humble. But life is the great teacher.",
        "life is the great teacher",
    ),
    (
        "Hold to the now, the here, through which all future plunges to the past.",
        "the now, the here",
    ),
];

pub(crate) fn show_page(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        widget_ids::feature_scope(ui, "settings", |ui| {
            with_settings_page(ui, |ui, viewport_width| {
                render_page_body(ui, app, viewport_width);
            });
        });
    });
}

fn with_settings_page(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui, f32)) {
    widget_ids::scope(ui, "settings_page", |ui| {
        SettingsUi::apply_typography(ui);
        let viewport_size = SettingsUi::page_viewport_size(ui);
        let surface_size = SettingsUi::page_surface_size(ui);
        egui::ScrollArea::both()
            .id_salt(widget_ids::scroll_id(ui, "settings_page_scroll"))
            .scroll_source(egui::scroll_area::ScrollSource {
                drag: false,
                ..Default::default()
            })
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_size(surface_size);
                ui.set_width(surface_size.x);
                ui.set_max_width(surface_size.x);
                add_contents(ui, viewport_size.x);
            });
    });
}

fn render_page_body(ui: &mut egui::Ui, app: &mut ScratchpadApp, viewport_width: f32) {
    let content_width = SettingsUi::page_content_width();
    let horizontal_margin =
        SettingsUi::page_horizontal_margin_for_viewport(viewport_width, content_width);

    ui.add_space(SettingsUi::LAYOUT.body_top_space);
    ui.horizontal(|ui| {
        ui.add_space(horizontal_margin);
        ui.vertical(|ui| {
            ui.set_width(content_width);
            render_page_heading(ui);
            render_settings_categories(ui, app);
        });
    });
    ui.add_space(SettingsUi::LAYOUT.body_bottom_space);
}

fn render_page_heading(ui: &mut egui::Ui) {
    let heading_width = SettingsUi::card_width(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(SettingsUi::card_leading_inset(ui));
        ui.vertical(|ui| {
            ui.set_width(heading_width);
            ui.label(
                egui::RichText::new("Settings")
                    .size(SettingsUi::TYPOGRAPHY.title)
                    .strong()
                    .color(text_primary(ui)),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Editor appearance, runtime behavior, and stored configuration.",
                )
                .size(SettingsUi::TYPOGRAPHY.description)
                .color(text_muted(ui)),
            );
        });
    });
}

fn category_heading(ui: &mut egui::Ui, heading: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(SettingsUi::card_leading_inset(ui));
        ui.label(
            egui::RichText::new(heading)
                .size(SettingsUi::TYPOGRAPHY.category)
                .strong()
                .color(text_primary(ui)),
        );
    });
    ui.add_space(12.0);
}

pub(crate) fn apply_dialog_typography(ui: &mut egui::Ui) {
    SettingsUi::apply_typography(ui);
}

pub(crate) fn dialog_card_frame(ui: &egui::Ui) -> egui::Frame {
    SettingsUi::card_frame(ui)
}

pub(crate) fn dialog_card_border(ui: &egui::Ui) -> egui::Color32 {
    SettingsUi::card_border(ui)
}

#[cfg(test)]
mod tests {
    use super::widgets::{
        reset_settings_layout_measurements, settings_card_measurements,
        settings_control_measurements,
    };
    use super::{ScratchpadApp, SettingsUi, egui, render_page_body, widgets};
    use crate::app::services::session_store::SessionStore;
    use crate::app::services::settings_store::SettingsStore;
    use crate::app::startup::StartupOptions;

    #[test]
    fn settings_cards_render_at_exact_card_width() {
        render_settings_layout_for_test(1600.0, 1600.0);

        let card_measurements = settings_card_measurements();
        assert!(
            !card_measurements.is_empty(),
            "settings card width test did not collect any cards"
        );

        let card_width = SettingsUi::LAYOUT.card_max_width;
        let card_offenders: Vec<String> = card_measurements
            .iter()
            .filter(|measurement| (measurement.width - card_width).abs() > f32::EPSILON)
            .map(|measurement| format!("{} = {:.1}px", measurement.label, measurement.width))
            .collect();

        assert!(
            card_offenders.is_empty(),
            "settings cards must be exactly {:.1}px wide:\n{}",
            card_width,
            card_offenders.join("\n")
        );
    }

    #[test]
    fn settings_cards_are_centered_in_editor_window() {
        render_settings_layout_for_test(1600.0, 1600.0);

        let card_measurements = settings_card_measurements();
        assert!(
            !card_measurements.is_empty(),
            "settings card centering test did not collect any cards"
        );

        let expected_center = 800.0;
        let card_offenders: Vec<String> = card_measurements
            .iter()
            .filter(|measurement| (measurement.center_x - expected_center).abs() > f32::EPSILON)
            .map(|measurement| {
                format!(
                    "{} center = {:.1}px",
                    measurement.label, measurement.center_x
                )
            })
            .collect();

        assert!(
            card_offenders.is_empty(),
            "settings cards must be centered at {:.1}px:\n{}",
            expected_center,
            card_offenders.join("\n")
        );
    }

    #[test]
    fn settings_cards_center_on_resized_visible_window() {
        render_settings_layout_for_test(1180.0, 900.0);

        let card_measurements = settings_card_measurements();
        assert!(
            !card_measurements.is_empty(),
            "resized settings centering test did not collect any cards"
        );

        let expected_center = 450.0;
        let card_offenders: Vec<String> = card_measurements
            .iter()
            .filter(|measurement| (measurement.center_x - expected_center).abs() > f32::EPSILON)
            .map(|measurement| {
                format!(
                    "{} center = {:.1}px",
                    measurement.label, measurement.center_x
                )
            })
            .collect();

        assert!(
            card_offenders.is_empty(),
            "settings cards must stay centered in the visible 900px viewport at {:.1}px:\n{}",
            expected_center,
            card_offenders.join("\n")
        );
    }

    #[test]
    fn settings_cards_keep_width_when_visible_window_is_narrow() {
        render_settings_layout_for_test(1180.0, 500.0);

        let card_measurements = settings_card_measurements();
        assert!(
            !card_measurements.is_empty(),
            "narrow settings width test did not collect any cards"
        );

        let expected_width = SettingsUi::LAYOUT.card_max_width;
        let expected_left = SettingsUi::LAYOUT.page_resize_gutter;
        let card_offenders: Vec<String> = card_measurements
            .iter()
            .filter(|measurement| {
                let left = measurement.center_x - measurement.width * 0.5;
                (measurement.width - expected_width).abs() > f32::EPSILON
                    || (left - expected_left).abs() > f32::EPSILON
            })
            .map(|measurement| {
                format!(
                    "{} width = {:.1}px left = {:.1}px",
                    measurement.label,
                    measurement.width,
                    measurement.center_x - measurement.width * 0.5
                )
            })
            .collect();

        assert!(
            card_offenders.is_empty(),
            "settings cards must keep their width and clamp to the resize gutter:\n{}",
            card_offenders.join("\n")
        );
    }

    #[test]
    fn settings_font_card_controls_align_to_row_right_edge() {
        render_settings_layout_for_test(1600.0, 1600.0);

        let control_measurements = settings_control_measurements();
        assert!(
            !control_measurements.is_empty(),
            "settings control alignment test did not collect any controls"
        );

        for (row_label, control_label) in [
            ("inner_select_row.Family", "combo.Font family"),
            ("inner_select_row.Size", "slider.Font size"),
            ("inner_select_row.Gutter", "slider.Gutter"),
        ] {
            let row = measurement_for(&control_measurements, row_label);
            let control = measurement_for(&control_measurements, control_label);
            assert!(
                (control.right_x - row.right_x).abs() <= f32::EPSILON,
                "{control_label} right edge = {:.1}px, expected {:.1}px from {row_label}; row width {:.1}px center {:.1}px, control width {:.1}px center {:.1}px",
                control.right_x,
                row.right_x,
                row.width,
                row.center_x,
                control.width,
                control.center_x,
            );
        }
    }

    #[test]
    fn settings_toggle_labels_align_with_switch_centers() {
        render_settings_layout_for_test(1600.0, 1600.0);

        let control_measurements = settings_control_measurements();
        let label = measurement_for(&control_measurements, "toggle_control.label");
        let switch = measurement_for(&control_measurements, "toggle_control.switch");
        assert!(
            (label.center_y - switch.center_y).abs() <= 0.5,
            "toggle label center y = {:.1}px, expected switch center y {:.1}px",
            label.center_y,
            switch.center_y,
        );
    }

    fn render_settings_layout_for_test(surface_width: f32, viewport_width: f32) {
        let mut app = test_app();
        reset_settings_layout_measurements();

        let ctx = egui::Context::default();
        crate::app::fonts::apply_editor_fonts(
            &ctx,
            &app.state.app_settings.editor_font_selection(),
        )
        .expect("install editor fonts for settings layout test");
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.set_min_size(egui::vec2(surface_width, 4200.0));
            ui.set_width(surface_width);
            render_page_body(ui, &mut app, viewport_width);
        });
    }

    fn measurement_for<'a>(
        measurements: &'a [widgets::SettingsControlMeasurement],
        label: &str,
    ) -> &'a widgets::SettingsControlMeasurement {
        measurements
            .iter()
            .find(|measurement| measurement.label == label)
            .unwrap_or_else(|| panic!("missing settings measurement for {label}"))
    }

    fn test_app() -> ScratchpadApp {
        const MIB: u64 = 1024 * 1024;
        let temp_dir = tempfile::tempdir().expect("create temp app root");
        let root = temp_dir.keep();
        let mut app = ScratchpadApp::with_stores_and_startup(
            SessionStore::new(root.clone()),
            SettingsStore::new(root),
            StartupOptions::default(),
        );
        app.set_session_persist_on_drop(false);
        app.state.app_settings.editor.font_size = 32.0;
        app.state.app_settings.editor.editor_gutter = 32;
        app.state
            .app_settings
            .workspace
            .tab_list_auto_hide_delay_seconds = 10.0;
        app.state.app_settings.history.budget.per_file_byte_budget = 1024 * MIB;
        app.state.app_settings.history.budget.aggregate_byte_budget = 4096 * MIB;
        app.state
            .app_settings
            .history
            .budget
            .persisted_payload_budget = 1024 * MIB;
        app
    }
}
