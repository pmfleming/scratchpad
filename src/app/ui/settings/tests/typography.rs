use super::*;

#[test]
fn labels_tabs_and_both_previews_share_the_configured_font() {
    for preset in [EditorFontPreset::Mono, EditorFontPreset::Serif] {
        for size in [8.0, 16.0, 32.0, 72.0] {
            let mut app = app();
            app.state.app_settings.editor.appearance_source = EditorAppearanceSource::App;
            app.state.app_settings.editor.editor_font = preset;
            app.state.app_settings.editor.font_size = size;
            app.state.app_settings.editor.editor_text_color = "#e4c7a2".into();
            app.state.app_settings.editor.editor_background_color = "#241837".into();
            app.state.app_settings.editor.editor_text_highlight_color = "#68aeee".into();
            let ctx = egui::Context::default();
            crate::app::app_state::prepare_context_before_first_frame(&mut app, &ctx);
            let settings = &app.state.app_settings;
            let font = crate::app::chrome::tab_label_font_id(size);
            let mut tab_rect = egui::Rect::NOTHING;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 6000.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    SettingsUi::apply_appearance(ui, settings);
                    ui.label("Settings label");
                    tab_rect = crate::app::chrome::tab_button_with_actions(
                        ui,
                        "typography.tab",
                        "Tab label",
                        false,
                        false,
                        crate::app::chrome::TabButtonOptions::new(1000.0)
                            .with_label_font_id(font.clone()),
                    )
                    .0
                    .rect;
                    widgets::render_preview_panel(ui, &app, false);
                    widgets::render_preview_panel(ui, &app, true);
                },
            );
            output.textures_delta.clear();
            let mut matched = 0;
            for shape in &output.shapes {
                let egui::Shape::Text(text) = &shape.shape else {
                    continue;
                };
                let value = text.galley.text();
                let preview = value == PREVIEW_QUOTES[app.state.settings_preview_quote_index].0
                    || value.starts_with("Think you're escaping");
                if !preview && value != "Settings label" && value != "Tab label" {
                    continue;
                }
                matched += 1;
                for section in &text.galley.job.sections {
                    assert_eq!(section.format.font_id, font);
                    let highlighted = section.format.background != egui::Color32::TRANSPARENT;
                    assert_eq!(
                        if section.format.color == egui::Color32::PLACEHOLDER {
                            text.fallback_color
                        } else {
                            section.format.color
                        },
                        if value == "Tab label" {
                            crate::app::theme::text_primary_for_visuals(&ctx.global_style().visuals)
                        } else if highlighted {
                            settings.editor_text_highlight_text_color()
                        } else {
                            settings.editor_text_color()
                        }
                    );
                    if highlighted {
                        assert_eq!(
                            section.format.background,
                            settings.editor_text_highlight_color()
                        );
                    }
                }
                if value == "Tab label" {
                    let bounds = egui::Rect::from_min_size(text.pos, text.galley.size());
                    assert!(
                        tab_rect.contains_rect(bounds),
                        "large tab text must fit its row"
                    );
                }
            }
            assert_eq!(
                matched, 4,
                "both previews, Settings label and tab label must render"
            );
            assert!(output.shapes.iter().any(|shape| matches!(
                &shape.shape,
                egui::Shape::Rect(rect) if rect.fill == settings.editor_background_color()
            )));
        }
    }
}

#[test]
fn large_tab_labels_fit_in_each_tab_list_position() {
    use crate::app::domain::{BufferState, WorkspaceTab};
    use crate::app::services::settings_store::TabListPosition;
    use crate::app::ui::tab_strip;

    for position in [
        TabListPosition::Top,
        TabListPosition::Bottom,
        TabListPosition::Left,
        TabListPosition::Right,
    ] {
        let mut app = app();
        app.state.app_settings.editor.appearance_source = EditorAppearanceSource::App;
        app.state.app_settings.editor.font_size = 72.0;
        app.state.app_settings.editor.theme_mode = super::AppThemeMode::Light;
        app.state.app_settings.editor.editor_text_color = "#e4c7a2".into();
        app.state.app_settings.workspace.tab_list_position = position;
        app.state.app_settings.workspace.auto_hide_tab_list = false;
        app.tab_manager.set_tabs(
            ["A", "B", "C"]
                .into_iter()
                .map(|name| WorkspaceTab::new(BufferState::new(name.into(), String::new(), None)))
                .collect(),
            0,
        );
        let mut h = Harness::builder()
            .with_size(egui::vec2(3000.0, 900.0))
            .build_ui_state(
                |ui, app| {
                    crate::app::app_state::prepare_context_before_first_frame(app, ui.ctx());
                    match position {
                        TabListPosition::Top => tab_strip::show_header(ui, app),
                        TabListPosition::Bottom => tab_strip::show_bottom_tab_list(ui, app),
                        _ => tab_strip::show_vertical_tab_list(ui, app),
                    }
                },
                app,
            );
        h.run();
        let mut count = 0;
        for shape in &h.output().shapes {
            if let egui::Shape::Text(text) = &shape.shape
                && ["A", "B", "C"].contains(&text.galley.text())
            {
                count += 1;
                assert_eq!(text.galley.job.sections[0].format.font_id.size, 72.0);
                assert_eq!(
                    if text.galley.job.sections[0].format.color == egui::Color32::PLACEHOLDER {
                        text.fallback_color
                    } else {
                        text.galley.job.sections[0].format.color
                    },
                    crate::app::theme::text_primary_for_visuals(&h.ctx.global_style().visuals)
                );
                let bounds = egui::Rect::from_min_size(text.pos, text.galley.size());
                assert!(
                    shape.clip_rect.contains_rect(bounds),
                    "{position:?}: {bounds:?} must fit {:?}",
                    shape.clip_rect
                );
            }
        }
        assert_eq!(count, 3, "{position:?}");
    }
}

#[test]
fn horizontal_tab_width_keeps_typical_names_readable_at_large_sizes() {
    for size in [14.0, 48.0, 72.0] {
        let ctx = egui::Context::default();
        setup(&ctx);
        let mut truncated = true;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(2000.0, 500.0),
                )),
                ..Default::default()
            },
            |ui| {
                truncated = crate::app::chrome::tab_button_with_actions(
                    ui,
                    "large-tab",
                    "Notes.txt",
                    true,
                    false,
                    crate::app::chrome::TabButtonOptions::new(
                        crate::app::chrome::tab_button_width(size),
                    )
                    .with_label_font_id(crate::app::chrome::tab_label_font_id(size)),
                )
                .3;
            },
        );
        output.textures_delta.clear();
        assert!(!truncated, "Typical name truncated at {size}pt");
        assert!(
            output.shapes.iter().any(
                |s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "Notes.txt")
            )
        );
    }
}
