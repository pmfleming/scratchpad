use super::*;

fn custom_app(size: f32, light: bool) -> ScratchpadApp {
    let mut app = app();
    let editor = &mut app.state.app_settings.editor;
    editor.appearance_source = EditorAppearanceSource::App;
    editor.font_size = size;
    editor.editor_font = EditorFontPreset::Mono;
    editor.editor_text_color = if light { "#302347" } else { "#f4eacb" }.into();
    editor.editor_background_color = if light { "#f8f1dc" } else { "#211933" }.into();
    editor.editor_text_highlight_color = "#68aeee".into();
    app
}

#[test]
fn appearance_is_resolved_from_the_selected_source_and_stays_scoped() {
    for source in [EditorAppearanceSource::App, EditorAppearanceSource::System] {
        for light in [false, true] {
            let mut app = custom_app(32.0, light);
            app.state.app_settings.editor.appearance_source = source;
            let settings = &app.state.app_settings;
            let ctx = egui::Context::default();
            setup(&ctx);
            let original = ctx.global_style();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let inherited = ui.style().clone();
                widget_ids::scope(ui, "appearance-test", |ui| {
                    SettingsUi::apply_appearance(ui, settings);
                    SettingsUi::apply_controls(ui);
                    let body = egui::TextStyle::Body.resolve(ui.style());
                    assert_eq!(
                        body.family,
                        egui::FontFamily::Name(EDITOR_FONT_FAMILY.into())
                    );
                    assert_eq!(body.size, settings.font_size());
                    assert_eq!(text_primary(ui), settings.editor_text_color());
                    assert_eq!(ui.visuals().panel_fill, settings.editor_background_color());
                    assert_eq!(
                        SettingsUi::accent(ui),
                        settings.editor_text_highlight_color()
                    );
                    assert_eq!(SettingsUi::font(ui, 28.0).size, body.size * 2.0);
                    assert_eq!(SettingsUi::font(ui, 12.5).family, body.family);
                    assert_ne!(SettingsUi::field_bg(ui), ui.visuals().panel_fill);
                });
                assert_eq!(ui.style(), &inherited);
            });
            output.textures_delta.clear();
            assert_eq!(ctx.global_style(), original);
        }
    }
}

fn label_format(h: &Harness<'_, ScratchpadApp>, label: &str) -> egui::TextFormat {
    h.output()
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(t) if t.galley.text() == label => {
                Some(t.galley.job.sections[0].format.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("Missing text: {label}"))
}

#[test]
fn page_and_menus_update_live_when_switching_appearance_source() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 9000.0))
        .allow_missing_glyphs()
        .build_ui_state(
            |ui, app| {
                crate::app::app_state::prepare_context_before_first_frame(app, ui.ctx());
                show_page(ui, app);
            },
            custom_app(20.0, false),
        );
    h.run_steps(3);
    let font = label_format(&h, "Font");
    assert_eq!(font.font_id.size, 20.0);
    assert_eq!(
        label_format(&h, "Scratchpad").color,
        h.state()
            .state
            .app_settings
            .editor_text_highlight_text_color()
    );
    assert_eq!(font.color, h.state().state.app_settings.editor_text_color());
    h.get_all_by_role_and_label(egui::Role::RadioButton, "System")
        .next()
        .unwrap()
        .click_accesskit();
    h.run_steps(3);
    assert_eq!(
        h.state().state.app_settings.editor_appearance_source(),
        EditorAppearanceSource::System
    );
    assert_eq!(
        label_format(&h, "Font").font_id.size,
        h.state().state.app_settings.font_size()
    );
    assert_eq!(
        label_format(&h, "Font").color,
        h.state().state.app_settings.editor_text_color()
    );
    h.get_by_role_and_label(egui::Role::RadioButton, "Scratchpad")
        .click_accesskit();
    h.run_steps(3);
    assert_eq!(label_format(&h, "Font"), font);
    h.get_by_role_and_label(egui::Role::ComboBox, "Font")
        .click_accesskit();
    h.run_steps(3);
    let option = label_format(&h, "Serif");
    assert_eq!(option.font_id, font.font_id);
    assert_eq!(option.color, font.color);
    h.key_press(egui::Key::Escape);
    h.state_mut().state.app_settings.editor.font_size = 32.0;
    h.state_mut().state.app_settings.editor.editor_text_color = "#cdeeff".into();
    h.run_steps(3);
    assert_eq!(label_format(&h, "Font").font_id.size, 32.0);
    assert_eq!(
        label_format(&h, "Font").color,
        h.state().state.app_settings.editor_text_color()
    );
    h.get_by_label("Edit Text color").click_accesskit();
    h.run_steps(3);
    let channel = h
        .output()
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text().starts_with("R ") => {
                Some(&t.galley.job.sections[0].format)
            }
            _ => None,
        })
        .expect("Color picker channel");
    assert_eq!(channel.font_id, label_format(&h, "Font").font_id);
    assert_eq!(
        channel.color,
        h.state().state.app_settings.editor_text_color()
    );
}

#[test]
fn large_settings_fonts_fit_narrow_and_wide_layouts() {
    for (width, size) in [
        (360.0, 8.0),
        (360.0, 32.0),
        (360.0, 72.0),
        (1100.0, 32.0),
        (1100.0, 72.0),
    ] {
        for light in [false, true] {
            reset_settings_layout_measurements();
            let mut app = custom_app(size, light);
            let ctx = egui::Context::default();
            crate::app::app_state::prepare_context_before_first_frame(&mut app, &ctx);
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 100_000.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    ui.set_width(width);
                    render_page_body(ui, &mut app, width);
                },
            );
            output.textures_delta.clear();
            let measurements = settings_control_measurements();
            for control in &measurements {
                if [
                    "combo.", "button.", "segment.", "number.", "slider.", "label.",
                ]
                .iter()
                .any(|prefix| control.label.starts_with(prefix))
                {
                    assert!(
                        control.rect.left() >= 0.0 && control.rect.right() <= width + 1.0,
                        "{} overflows at width={width}, size={size}: {:?}",
                        control.label,
                        control.rect
                    );
                }
            }
            let find = |name: &str| measurements.iter().find(|m| m.label == name).unwrap().rect;
            assert!(
                !find("slider.Size").intersects(find("dropdown.Size")),
                "Font controls overlap at {width}/{size}"
            );
            for name in ["Editor padding", "Memory per file", "Indent width"] {
                assert!(
                    !find(&format!("slider.{name}")).intersects(find(&format!("number.{name}"))),
                    "Numeric controls overlap: {name} at {width}/{size}"
                );
            }
        }
    }
}

#[test]
fn dropdown_popups_stay_near_their_controls_instead_of_filling_the_window() {
    for (width, size) in [
        (360.0, 16.0),
        (1100.0, 32.0),
        (2000.0, 16.0),
        (1100.0, 72.0),
    ] {
        let mut h = Harness::builder()
            .with_size(egui::vec2(width, 900.0))
            .with_context_setup(setup)
            .build_ui_state(
                |ui, app| {
                    SettingsUi::apply_appearance(ui, &app.state.app_settings);
                    SettingsUi::apply_controls(ui);
                    ui.set_width(300.0);
                    widgets::inner_select_row(ui, "Size", None, |ui| {
                        let mut value = app.state.app_settings.editor.font_size as u32;
                        widgets::dropdown(
                            ui,
                            "Size",
                            &mut value,
                            &FONT_SIZE_OPTIONS,
                            |size| size.to_string(),
                            96.0,
                            "pt",
                        );
                        app.state.app_settings.editor.font_size = value as f32;
                    });
                },
                custom_app(size, false),
            );
        h.run_steps(3);
        let control = h.get_by_role_and_label(egui::Role::ComboBox, "Size").rect();
        h.get_by_role_and_label(egui::Role::ComboBox, "Size")
            .click_accesskit();
        h.run_steps(3);
        let option = h.get_by_label("11").rect();
        assert!(
            option.width() <= control.width() + 20.0,
            "Popup stretches beyond its control at {width}/{size}: {option:?} vs {control:?}"
        );
        assert!(option.left() >= 0.0 && option.right() <= width);
        h.get_by_label("11").click_accesskit();
        h.run_steps(3);
        assert_eq!(h.state().state.app_settings.font_size(), 11.0);
    }
}
