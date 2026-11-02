use super::{
    AppThemeMode, EditorAppearanceSource, EditorFontPreset, FONT_SIZE_OPTIONS, IndentationStyle,
    ScratchpadApp, SettingsUi, TabDisplayMode, egui,
    widgets::{
        dropdown, group_heading, inner_select_row, numeric_slider, record_settings_control_box,
        render_preview_panel, segmented_row, toggle_select_row,
    },
};
use crate::app::app_state::settings_controller as settings;
use crate::app::ui::widget_ids;

pub(super) fn render_appearance(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let mut source = app.state.app_settings.editor_appearance_source();
    if segmented_row(
        ui,
        "Use appearance from",
        &mut source,
        &[
            (EditorAppearanceSource::App, "Scratchpad"),
            (EditorAppearanceSource::System, "System"),
        ],
    ) {
        settings::set_editor_appearance_source(app, source);
    }
    let mut font = app.state.app_settings.editor_font();
    inner_select_row(ui, "Font", None, |ui| {
        if dropdown(
            ui,
            "Font",
            &mut font,
            &EditorFontPreset::ALL,
            |font| font.label().into(),
            180.0,
            "",
        ) {
            use_app_appearance(app);
            settings::set_editor_font(app, font);
        }
    });
    font_size_row(ui, app);
    let custom_id = egui::Id::unique("settings.custom_palette");
    let custom = ui
        .data_mut(|data| data.get_temp::<bool>(custom_id))
        .unwrap_or(false);
    let mut mode = if custom || app.state.app_settings.has_custom_editor_palette() {
        None
    } else {
        Some(app.state.app_settings.theme_mode())
    };
    if segmented_row(
        ui,
        "Colors",
        &mut mode,
        &[
            (Some(AppThemeMode::System), "System"),
            (Some(AppThemeMode::Light), "Light"),
            (Some(AppThemeMode::Dark), "Dark"),
            (None, "Custom"),
        ],
    ) {
        use_app_appearance(app);
        ui.data_mut(|data| data.insert_temp(custom_id, mode.is_none()));
        if let Some(mode) = mode {
            settings::apply_theme_mode_preset(app, mode, ui.ctx().system_theme());
        }
    }
    // These controls remain visible even when following the operating system.
    color_row(
        ui,
        "Text",
        app.state.app_settings.editor_text_color(),
        settings::set_editor_text_color,
        app,
    );
    color_row(
        ui,
        "Background",
        app.state.app_settings.editor_background_color(),
        settings::set_editor_background_color,
        app,
    );
    color_row(
        ui,
        "Search highlight",
        app.state.app_settings.editor_text_highlight_color(),
        settings::set_editor_text_highlight_color,
        app,
    );
    render_preview_panel(ui, app, false);
}

fn font_size_row(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let mut size = app.state.app_settings.font_size();
    inner_select_row(ui, "Size", None, |ui| {
        let slider_changed = font_size_slider(ui, &mut size);
        let options = FONT_SIZE_OPTIONS.map(|size| size as f32);
        let preset_changed = dropdown(
            ui,
            "Size",
            &mut size,
            &options,
            |size| format!("{size:.0}"),
            96.0,
            "pt",
        );
        if slider_changed || preset_changed {
            use_app_appearance(app);
            settings::set_font_size(app, size);
        }
    });
}

fn font_size_slider(ui: &mut egui::Ui, size: &mut f32) -> bool {
    let lane = ui.max_rect();
    let unit_width = ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(
                "pt".into(),
                SettingsUi::font(ui, 12.5),
                super::text_muted(ui),
            )
            .size()
            .x
    });
    let width = (lane.width() - 96.0 - unit_width - ui.spacing().item_spacing.x - 10.0).max(20.0);
    let rect = egui::Rect::from_min_size(lane.min, egui::vec2(width, lane.height()));
    let response = widget_ids::rect_scope_with_layout(
        ui,
        rect,
        "settings.font_size.slider.layout",
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().slider_width = width;
            SettingsUi::apply_slider_style(ui);
            widget_ids::surface_response(ui, "settings.font_size", "slider", |ui| {
                ui.add(
                    egui::Slider::new(size, 8.0..=72.0)
                        .step_by(1.0)
                        .clamping(egui::SliderClamping::Edits)
                        .handle_shape(egui::style::HandleShape::Circle)
                        .trailing_fill(true)
                        .show_value(false),
                )
            })
        },
    )
    .inner;
    record_settings_control_box("slider.Size", response.rect);
    let changed = response.changed();
    response
        .accessible_name("Size")
        .on_hover_text("Font size in points (8–72).");
    changed
}

fn use_app_appearance(app: &mut ScratchpadApp) {
    if app.state.app_settings.uses_system_editor_appearance() {
        settings::set_editor_appearance_source(app, EditorAppearanceSource::App);
    }
}

fn color_row(
    ui: &mut egui::Ui,
    label: &str,
    initial: egui::Color32,
    set: fn(&mut ScratchpadApp, egui::Color32),
    app: &mut ScratchpadApp,
) {
    inner_select_row(ui, label, None, |ui| {
        let mut color = initial;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let response = ui.color_edit_button_srgba(&mut color);
            record_settings_control_box(format!("color.{label}"), response.rect);
            if response.changed() {
                use_app_appearance(app);
                set(app, color);
                ui.data_mut(|data| {
                    data.insert_temp(egui::Id::unique("settings.custom_palette"), true)
                });
            }
        });
    });
}

pub(super) fn render_editing(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    toggle_select_row(
        ui,
        "Word wrap",
        None,
        "settings.wrap",
        app.state.app_settings.word_wrap(),
        |value| settings::set_word_wrap(app, value),
    );
    let mut padding = u64::from(app.state.app_settings.editor_gutter());
    if numeric_slider(ui, "Editor padding", &mut padding, 0..=32, "px") {
        settings::set_editor_gutter(app, padding as u8);
    }
    group_heading(ui, "Indentation");
    let mut style = app.state.app_settings.indentation_style();
    if segmented_row(
        ui,
        "Indent with",
        &mut style,
        &[
            (IndentationStyle::Spaces, "Spaces"),
            (IndentationStyle::TabCharacter, "Tabs"),
        ],
    ) {
        settings::set_indentation_style(app, style);
    }
    let mut width = u64::from(app.state.app_settings.editor_tab_width());
    if numeric_slider(ui, "Indent width", &mut width, 1..=16, "col") {
        settings::set_editor_tab_width(app, width as u8);
    }
    let mut mode = app.state.app_settings.tab_display();
    if segmented_row(
        ui,
        "Show tabs",
        &mut mode,
        &[
            (TabDisplayMode::Hidden, "None"),
            (TabDisplayMode::Character, "Characters"),
            (TabDisplayMode::Tablines, "Guides"),
        ],
    ) {
        settings::set_tab_display(app, mode);
    }
    render_preview_panel(ui, app, true);
}
