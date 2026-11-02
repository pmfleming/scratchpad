use super::*;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use widgets::{
    reset_settings_layout_measurements, settings_card_measurements, settings_control_measurements,
};

fn app() -> ScratchpadApp {
    let root = tempfile::tempdir().unwrap().keep();
    crate::app::app_state::test_support::test_app(&root)
}
fn setup(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    fonts.families.insert(
        egui::FontFamily::Name(EDITOR_FONT_FAMILY.into()),
        fonts.families[&egui::FontFamily::Monospace].clone(),
    );
    ctx.set_fonts(fonts);
}

#[test]
fn settings_have_five_independent_categories_without_nested_disclosures() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 6000.0))
        .with_context_setup(setup)
        .build_ui_state(
            |ui, app| {
                render_page_body(ui, app, 1100.0);
            },
            app(),
        );
    harness.run();
    assert_eq!(
        harness
            .get_all_by_role(egui::Role::DisclosureTriangle)
            .count(),
        5
    );
    for label in [
        "Appearance",
        "Editors",
        "Workspace",
        "History",
        "Application",
    ] {
        harness.get_by_role_and_label(egui::Role::DisclosureTriangle, label);
    }
    assert_eq!(harness.get_all_by_role(egui::Role::Slider).count(), 9);
    for label in [
        "Memory per file",
        "Memory across all files",
        "Saved undo per file",
    ] {
        harness.get_by_role_and_label(egui::Role::Slider, label);
    }
    assert!(harness.query_by_label("Reset section").is_none());
    assert!(
        harness
            .query_by_label("Maximum detailed entries per file")
            .is_none()
    );
    harness
        .get_by_role_and_label(egui::Role::DisclosureTriangle, "Appearance")
        .click_accesskit();
    harness.run_steps(2);
    assert!(harness.query_by_label("Text preview").is_none());
    harness.get_by_label("Tab preview");
    harness
        .get_by_role_and_label(egui::Role::DisclosureTriangle, "History")
        .click_accesskit();
    harness.run_steps(2);
    assert!(
        harness
            .query_by_role_and_label(egui::Role::Slider, "Memory per file")
            .is_none()
    );
}

#[test]
fn search_reveals_matching_groups_without_losing_collapse_state() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 6000.0))
        .with_context_setup(setup)
        .build_ui_state(|ui, app| render_page_body(ui, app, 1100.0), app());
    harness.run();
    harness.get_by_label("Collapse all").click_accesskit();
    harness.run_steps(2);
    assert!(harness.query_by_role(egui::Role::Slider).is_none());
    let ctx = harness.ctx.clone();
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::unique("settings.search"), "persisted".to_owned())
    });
    harness.run_steps(2);
    assert_eq!(
        harness
            .get_all_by_role(egui::Role::DisclosureTriangle)
            .count(),
        1
    );
    harness.get_by_role_and_label(egui::Role::Slider, "Saved undo per file");
    ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::unique("settings.search"),
            "nothing-matches".to_owned(),
        )
    });
    harness.run_steps(2);
    harness.get_by_label("No matching settings. Try tabs, font or history.");
    ctx.data_mut(|data| data.insert_temp(egui::Id::unique("settings.search"), String::new()));
    harness.run_steps(2);
    assert_eq!(
        harness
            .get_all_by_role(egui::Role::DisclosureTriangle)
            .count(),
        5
    );
    assert!(harness.query_by_role(egui::Role::Slider).is_none());
    harness.get_by_label("Expand all").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.get_all_by_role(egui::Role::Slider).count(), 9);
}

#[test]
fn appearance_controls_share_right_edge_and_all_sliders_share_columns() {
    for width in [1200.0, 900.0, 500.0, 360.0] {
        reset_settings_layout_measurements();
        let ctx = egui::Context::default();
        setup(&ctx);
        let mut app = app();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.set_width(width);
            ui.set_min_height(9000.0);
            render_page_body(ui, &mut app, width);
        });
        output.textures_delta.clear();
        let measurements = settings_control_measurements();
        let find = |name: &str| {
            measurements
                .iter()
                .find(|m| m.label == name)
                .unwrap_or_else(|| panic!("missing {name}"))
        };
        let left = |m: &widgets::SettingsControlMeasurement| m.center_x - m.width / 2.0;
        for label in ["dropdown.Font", "dropdown.Size", "segments.Colors"] {
            assert!(
                (find(label).right_x - find("segments.Use appearance from").right_x).abs() < 1.0,
                "{label} at {width}"
            );
        }
        assert!(find("unit.Size").rect.left() > find("combo.Size").right_x);
        assert!((find("unit.Size").center_y - find("combo.Size").center_y).abs() < 1.0);
        let reference = find("slider.Editor padding");
        if width >= 900.0 {
            assert!((find("label.Editor padding").center_y - reference.center_y).abs() < 1.0);
            assert!((left(find("label.Editor padding")) - left(find("label.Font"))).abs() < 1.0);
        }
        for label in [
            "Indent width",
            "Closed-tab budget",
            "Recovery-point budget",
            "Memory per file",
            "Memory across all files",
            "Saved undo per file",
        ] {
            let slider = find(&format!("slider.{label}"));
            assert!(
                (left(reference) - left(slider)).abs() < 1.0,
                "{label} at {width}"
            );
            assert!(
                (reference.right_x - slider.right_x).abs() < 1.0,
                "{label} at {width}"
            );
            let number = find(&format!("number.{label}"));
            assert!((number.center_y - slider.center_y).abs() < 1.0);
        }
        let cards = settings_card_measurements();
        assert_eq!(cards.len(), 5, "no nested cards");
        for card in &cards {
            assert!(
                card.right_x <= width + 1.0,
                "card overflow at {width}: {card:?}"
            );
            assert!(
                (card.center_x - width / 2.0).abs() < 1.0,
                "card centering at {width}: {card:?}"
            );
        }
    }
}

#[test]
fn styled_controls_fit_and_align_with_production_fonts_in_both_themes() {
    for dark in [true, false] {
        for width in [360.0, 600.0, 1100.0] {
            for preset in [
                EditorFontPreset::Standard,
                EditorFontPreset::Mono,
                EditorFontPreset::Serif,
            ] {
                reset_settings_layout_measurements();
                let ctx = egui::Context::default();
                ctx.set_visuals(if dark {
                    egui::Visuals::dark()
                } else {
                    egui::Visuals::light()
                });
                crate::app::fonts::apply_editor_fonts(
                    &ctx,
                    &crate::app::fonts::EditorFontSelection::scratchpad(preset),
                    4.0,
                )
                .unwrap();
                let mut app = app();
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    ui.set_width(width);
                    ui.set_min_height(9000.0);
                    render_page_body(ui, &mut app, width);
                });
                output.textures_delta.clear();
                let measurements = settings_control_measurements();
                let find = |name: &str| measurements.iter().find(|m| m.label == name).unwrap();
                for (control, row) in [
                    ("segments.Use appearance from", "Use appearance from"),
                    ("segments.Colors", "Colors"),
                    ("dropdown.Font", "Font"),
                    ("dropdown.Size", "Size"),
                    ("combo.Place new tabs", "Place new tabs"),
                    ("combo.Retention", "Keep closed tabs for"),
                    ("button.Open file", "Settings file"),
                ] {
                    let control = find(control);
                    let lane = find(&format!("inner_select_row.{row}"));
                    assert!(
                        (control.right_x - lane.right_x).abs() < 1.0,
                        "{} {:?} at {width}",
                        control.label,
                        preset
                    );
                }
                assert!(find("unit.Size").rect.left() > find("combo.Size").right_x);
                let slider = find("slider.Size");
                let combo = find("combo.Size");
                assert!(slider.right_x + 9.0 <= combo.rect.left());
                assert!((slider.center_y - combo.center_y).abs() < 1.0);
                assert!(slider.rect.left() >= find("inner_select_row.Size").rect.left());
                for control in &measurements {
                    if ["combo.", "button.", "segment.", "number."]
                        .iter()
                        .any(|prefix| control.label.starts_with(prefix))
                    {
                        assert!(
                            control.rect.height() >= 35.5,
                            "{}: {:?}",
                            control.label,
                            control.rect
                        );
                        assert!(
                            control.right_x <= width,
                            "{} overflows at {width}",
                            control.label
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn long_segment_labels_stack_without_overlapping_the_next_row() {
    reset_settings_layout_measurements();
    let ctx = egui::Context::default();
    let mut selected = 0;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.set_width(300.0);
        SettingsUi::apply_typography(ui);
        SettingsUi::apply_controls(ui);
        widgets::segmented_row(
            ui,
            "Long choices",
            &mut selected,
            &[
                (0, "Follow the operating system"),
                (1, "Use application preferences"),
            ],
        );
        widgets::action_row(ui, "Next row", "Action", None);
    });
    output.textures_delta.clear();
    let measurements = settings_control_measurements();
    let find = |name: &str| measurements.iter().find(|m| m.label == name).unwrap();
    let group = find("segments.Long choices").rect;
    assert!(group.height() > 72.0);
    assert!(find("label.Next row").rect.top() >= group.bottom());
    for row in measurements
        .iter()
        .filter(|m| m.label.starts_with("segment."))
    {
        assert!(group.contains_rect(row.rect));
    }
}

#[test]
fn appearance_controls_stay_visible_for_system_source_and_editing_switches_source() {
    let mut app = app();
    crate::app::app_state::settings_controller::set_editor_appearance_source(
        &mut app,
        EditorAppearanceSource::System,
    );
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 1800.0))
        .with_context_setup(setup)
        .build_ui_state(
            |ui, app| {
                ui.set_width(880.0);
                text_formatting::render_appearance(ui, app);
            },
            app,
        );
    harness.run();
    for label in ["Font", "Size", "Text", "Background", "Search highlight"] {
        harness.get_by_role_and_label(egui::Role::Label, label);
    }
    harness
        .get_by_role_and_label(egui::Role::ComboBox, "Font")
        .click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("Mono").click_accesskit();
    harness.run_steps(2);
    assert_eq!(
        harness
            .state()
            .state
            .app_settings
            .editor_appearance_source(),
        EditorAppearanceSource::App
    );
    assert_eq!(
        harness.state().state.app_settings.editor_font(),
        EditorFontPreset::Mono
    );
}

#[test]
fn font_size_slider_and_presets_stay_in_sync_and_persist() {
    let mut app = app();
    app.state.app_settings.editor.appearance_source = EditorAppearanceSource::System;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 1800.0))
        .with_context_setup(setup)
        .build_ui_state(
            |ui, app| {
                reset_settings_layout_measurements();
                ui.set_width(880.0);
                text_formatting::render_appearance(ui, app);
            },
            app,
        );
    harness.run();
    harness
        .get_by_role_and_label(egui::Role::Slider, "Size")
        .set_value_accesskit(33.0);
    harness.run_steps(2);
    assert_eq!(harness.state().state.app_settings.font_size(), 33.0);
    assert_eq!(
        harness
            .state()
            .state
            .app_settings
            .editor_appearance_source(),
        EditorAppearanceSource::App
    );
    assert_eq!(
        harness
            .get_by_role_and_label(egui::Role::ComboBox, "Size")
            .accesskit_node()
            .value(),
        Some("33".into())
    );
    harness
        .get_by_role_and_label(egui::Role::ComboBox, "Size")
        .click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("20").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.state().state.app_settings.font_size(), 20.0);
    let slider = settings_control_measurements()
        .iter()
        .find(|m| m.label == "slider.Size")
        .unwrap()
        .rect;
    let mut pos = egui::pos2(slider.left() + 1.0, slider.center().y);
    harness.event(egui::Event::PointerMoved(pos));
    harness.event(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(2);
    assert_eq!(harness.state().state.app_settings.font_size(), 8.0);
    pos.x = slider.right() - 1.0;
    harness.event(egui::Event::PointerMoved(pos));
    harness.run_steps(2);
    assert_eq!(harness.state().state.app_settings.font_size(), 72.0);
    assert_eq!(
        harness
            .get_by_role_and_label(egui::Role::ComboBox, "Size")
            .accesskit_node()
            .value(),
        Some("72".into())
    );
    harness.event(egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(2);
    let saved = harness
        .state()
        .state
        .persistence
        .settings_store
        .load()
        .unwrap()
        .unwrap();
    assert_eq!(saved.editor.font_size, 72.0);
}

#[test]
fn global_reset_requires_confirmation_and_cancel_does_not_change_settings() {
    let mut app = app();
    app.state.app_settings.editor.editor_gutter = 23;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 1400.0))
        .with_context_setup(setup)
        .build_ui_state(
            |ui, app| {
                ui.set_width(880.0);
                opening::render_application(ui, app);
                opening::reset_confirmation(ui.ctx(), app);
            },
            app,
        );
    harness.run();
    harness.get_by_label("Reset settings…").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.state().state.app_settings.editor_gutter(), 23);
    harness.get_by_label("Cancel").click_accesskit();
    harness.run_steps(2);
    assert_eq!(harness.state().state.app_settings.editor_gutter(), 23);
    harness.get_by_label("Reset settings…").click_accesskit();
    harness.run_steps(2);
    harness
        .get_by_role_and_label(egui::Role::Button, "Reset all settings")
        .click_accesskit();
    harness.run_steps(2);
    assert_eq!(
        harness.state().state.app_settings.editor_gutter(),
        crate::app::services::settings_store::AppSettings::default().editor_gutter()
    );
}

fn has_button_border(
    shapes: &[egui::epaint::ClippedShape],
    target: egui::Rect,
    color: egui::Color32,
) -> bool {
    fn matches(shape: &egui::Shape, target: egui::Rect, color: egui::Color32) -> bool {
        match shape {
            egui::Shape::Rect(rect) => {
                (rect.rect.center() - target.center()).length() < 1.0
                    && rect.stroke.color == color
                    && rect.stroke.width >= 1.0
            }
            egui::Shape::Vec(shapes) => shapes.iter().any(|shape| matches(shape, target, color)),
            _ => false,
        }
    }
    shapes
        .iter()
        .any(|shape| matches(&shape.shape, target, color))
}

#[test]
fn both_reset_actions_are_destructive_without_restyling_ordinary_buttons() {
    for dark in [true, false] {
        let border = SettingsUi::destructive_colors(dark)[1];
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1000.0, 1400.0))
            .with_context_setup(move |ctx| {
                setup(ctx);
                ctx.set_visuals(if dark {
                    egui::Visuals::dark()
                } else {
                    egui::Visuals::light()
                });
            })
            .build_ui_state(
                |ui, app| {
                    ui.set_width(880.0);
                    SettingsUi::apply_typography(ui);
                    SettingsUi::apply_controls(ui);
                    opening::render_application(ui, app);
                    opening::reset_confirmation(ui.ctx(), app);
                },
                app(),
            );
        harness.run();
        let reset = harness
            .get_by_role_and_label(egui::Role::Button, "Reset settings…")
            .rect();
        assert!(reset.height() >= 36.0);
        assert!(has_button_border(&harness.output().shapes, reset, border));
        assert!(!has_button_border(
            &harness.output().shapes,
            harness.get_by_label("Open file").rect(),
            border
        ));
        harness.get_by_label("Reset settings…").click_accesskit();
        harness.run_steps(2);
        // Focus Cancel so the destructive action is in its resting state.
        harness.get_by_label("Cancel").focus();
        harness.run_steps(2);
        let confirm = harness
            .get_by_role_and_label(egui::Role::Button, "Reset all settings")
            .rect();
        assert!(confirm.height() >= 36.0);
        assert!(has_button_border(&harness.output().shapes, confirm, border));
        assert!(!has_button_border(
            &harness.output().shapes,
            harness.get_by_label("Cancel").rect(),
            border
        ));
        harness
            .get_by_role_and_label(egui::Role::Button, "Reset all settings")
            .focus();
        harness.run_steps(2);
        assert!(
            !has_button_border(&harness.output().shapes, confirm, border),
            "focus must stand out from the resting red border"
        );
    }
}

#[test]
fn both_previews_are_right_aligned_and_vertically_centered_without_clipping() {
    for width in [300.0, 880.0] {
        for wrap in [true, false] {
            for (size, padding) in [(20.0, 0), (32.0, 16)] {
                for (quote, quote_text, tabs) in PREVIEW_QUOTES
                    .iter()
                    .enumerate()
                    .map(|(index, &(text, _))| (index, text, false))
                    .chain(std::iter::once((0, "Think you're escaping", true)))
                {
                    reset_settings_layout_measurements();
                    let ctx = egui::Context::default();
                    crate::app::fonts::apply_editor_fonts(
                        &ctx,
                        &crate::app::fonts::EditorFontSelection::scratchpad(EditorFontPreset::Mono),
                        4.0,
                    )
                    .unwrap();
                    let mut app = app();
                    app.state.settings_preview_quote_index = quote;
                    app.state.app_settings.editor.font_size = size;
                    app.state.app_settings.editor.editor_gutter = padding;
                    app.state.app_settings.editor.word_wrap = wrap;
                    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                        ui.set_width(width);
                        ui.set_min_height(1600.0);
                        widgets::render_preview_panel(ui, &app, tabs);
                    });
                    output.textures_delta.clear();
                    let measurements = settings_control_measurements();
                    let find =
                        |name: &str| measurements.iter().find(|m| m.label == name).unwrap().rect;
                    let key = if tabs { "preview.tabs" } else { "preview.text" };
                    let content = find(&format!("{key}.content"));
                    let text = find(&format!("{key}.label"));
                    let panel = find(key);
                    assert!((text.right() - content.right()).abs() < 1.0);
                    assert!((text.center().y - content.center().y).abs() < 1.0);
                    assert!(content.height() >= 96.0);
                    assert!(text.top() >= panel.top() && text.bottom() <= panel.bottom());
                    assert!(panel.right() <= width + 1.0);
                    if wrap {
                        assert!(text.left() >= panel.left() && text.right() <= panel.right());
                    }
                    let shape = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text)
                                if text.galley.job.text.starts_with(quote_text) =>
                            {
                                Some(text)
                            }
                            _ => None,
                        })
                        .expect("quote text remains painted");
                    assert_eq!(shape.galley.job.halign, egui::Align::RIGHT);
                    assert!(
                        shape
                            .galley
                            .job
                            .sections
                            .iter()
                            .any(|section| section.format.background
                                == app.state.app_settings.editor_text_highlight_color())
                    );
                }
            }
        }
    }
}

#[test]
fn padding_slider_keeps_dragging_while_preview_resize_moves_the_row() {
    let mut state = app();
    state.state.app_settings.editor.editor_gutter = 0;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 6000.0))
        .with_context_setup(setup)
        .build_ui_state(
            |ui, app| {
                reset_settings_layout_measurements();
                render_page_body(ui, app, 1100.0);
            },
            state,
        );
    harness.run();
    let slider_rect = || {
        settings_control_measurements()
            .into_iter()
            .find(|m| m.label == "slider.Editor padding")
            .unwrap()
            .rect
    };
    let initial = slider_rect();
    let start = egui::pos2(initial.left() + 1.0, initial.center().y);
    harness.event(egui::Event::PointerMoved(start));
    harness.event(egui::Event::PointerButton {
        pos: start,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(2);
    let mut previous = harness.state().state.app_settings.editor_gutter();
    for fraction in [0.25, 0.5, 0.75, 0.99] {
        // Keep the button held and pointer on its original horizontal path,
        // even when the changing preview padding moves the entire row.
        let pos = egui::pos2(initial.left() + initial.width() * fraction, start.y);
        harness.event(egui::Event::PointerMoved(pos));
        harness.run_steps(2);
        let current = harness.state().state.app_settings.editor_gutter();
        assert!(
            current > previous,
            "drag stopped at {current}, fraction {fraction}"
        );
        previous = current;
    }
    assert_eq!(previous, 32);
    assert!(slider_rect().top() > initial.top());
    harness.event(egui::Event::PointerButton {
        pos: initial.right_center(),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run_steps(2);
}

#[test]
fn rendering_byte_budgets_does_not_round_or_clamp_stored_values() {
    let ctx = egui::Context::default();
    let mut bytes = 64 * 1024 * 1024 + 123;
    let original = bytes;
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.set_width(880.0);
        sections::byte_budget_row(ui, "Test budget", &mut bytes, 1, 1024);
    });
    output.textures_delta.clear();
    assert_eq!(bytes, original);
}

#[test]
fn auto_hide_reveals_delay_and_keeps_the_saved_value_when_disabled() {
    let mut app = app();
    app.state.app_settings.workspace.auto_hide_tab_list = false;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 1800.0))
        .with_context_setup(setup)
        .build_ui_state(
            |ui, app| {
                ui.set_width(880.0);
                appearance::render_workspace(ui, app);
            },
            app,
        );
    harness.run();
    assert!(
        harness
            .query_by_role_and_label(egui::Role::ComboBox, "Hide after")
            .is_none()
    );
    harness
        .get_by_role_and_label(egui::Role::Switch, "Auto-hide tabs")
        .click_accesskit();
    harness.run_steps(2);
    harness
        .get_by_role_and_label(egui::Role::ComboBox, "Hide after")
        .click_accesskit();
    harness.run_steps(2);
    harness.get_by_label("3").click_accesskit();
    harness.run_steps(2);
    assert_eq!(
        harness
            .state()
            .state
            .app_settings
            .tab_list_auto_hide_delay_seconds(),
        3.0
    );
    harness
        .get_by_role_and_label(egui::Role::Switch, "Auto-hide tabs")
        .click_accesskit();
    harness.run_steps(2);
    assert!(
        harness
            .query_by_role_and_label(egui::Role::ComboBox, "Hide after")
            .is_none()
    );
    assert_eq!(
        harness
            .state()
            .state
            .app_settings
            .tab_list_auto_hide_delay_seconds(),
        3.0
    );
}

#[test]
fn exact_value_inputs_and_recommended_limits_keep_slider_values_in_sync() {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 1800.0))
        .with_context_setup(setup)
        .build_ui_state(
            |ui, app| {
                ui.set_width(880.0);
                recovery::render_policy(ui, app);
            },
            app(),
        );
    harness.run();
    for (label, value) in [
        ("Memory per file", 83.0),
        ("Memory across all files", 1024.0),
        ("Saved undo per file", 0.0),
    ] {
        harness
            .get_by_role_and_label(egui::Role::SpinButton, label)
            .set_value_accesskit(value);
        harness.run_steps(2);
        assert_eq!(
            harness
                .get_by_role_and_label(egui::Role::Slider, label)
                .accesskit_node()
                .numeric_value(),
            Some(value)
        );
    }
    let budget = harness.state().state.app_settings.history.budget;
    assert_eq!(budget.per_file_byte_budget, 83 * 1024 * 1024);
    assert_eq!(budget.aggregate_byte_budget, 1024 * 1024 * 1024);
    assert_eq!(budget.persisted_payload_budget, 0);
    harness.get_by_label("Use recommended").click_accesskit();
    harness.run_steps(2);
    assert!(
        harness
            .state()
            .state
            .app_settings
            .history
            .budget
            .derived_from_memory
    );
    let saved = harness
        .state()
        .state
        .persistence
        .settings_store
        .load()
        .unwrap()
        .unwrap();
    assert_eq!(
        saved.history.budget,
        harness.state().state.app_settings.history.budget
    );
}
