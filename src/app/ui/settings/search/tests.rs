use super::*;
use crate::app::ui::settings::{render_page_body, widgets};
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn highlighting_requires_a_selective_query_and_matches_labels_and_aliases() {
    for query in ["", " ", "---", "a", "e", "or", "to", "ce", "not-a-setting"] {
        let search = Search::new(query);
        assert!(
            SETTINGS
                .iter()
                .all(|(_, label, _)| !search.highlights(label)),
            "{query}"
        );
    }
    for (query, expected) in [
        ("FONT size", "Size"),
        ("gutter", "Editor padding"),
        ("memory per file", "Memory per file"),
        ("colour background", "Background"),
        ("OS", "Use appearance from"),
    ] {
        let search = Search::new(query);
        let labels: Vec<_> = SETTINGS
            .iter()
            .filter(|(_, label, _)| search.highlights(label))
            .map(|(_, label, _)| *label)
            .collect();
        assert_eq!(labels, [expected], "{query}");
    }
}

#[test]
fn matching_rows_are_highlighted_without_changing_settings_or_layout() {
    for width in [360.0, 1100.0] {
        let root = tempfile::tempdir().unwrap();
        let mut app = crate::app::app_state::test_support::test_app(root.path());
        app.state.app_settings.workspace.auto_hide_tab_list = false;
        let original = app.state.app_settings.clone();
        let mut harness = Harness::builder()
            .with_size(egui::vec2(width + 20.0, 6000.0))
            .with_context_setup(|ctx| {
                crate::app::fonts::apply_editor_fonts(
                    ctx,
                    &crate::app::fonts::EditorFontSelection::scratchpad(
                        crate::app::fonts::EditorFontPreset::Mono,
                    ),
                    4.0,
                )
                .unwrap();
            })
            .build_ui_state(
                move |ui, app| {
                    widgets::reset_settings_layout_measurements();
                    ui.set_width(width);
                    render_page_body(ui, app, width);
                },
                app,
            );
        harness.run();
        for (query, expected) in [
            ("f", None),
            ("or", None),
            ("font size", Some("Size")),
            ("gutter", Some("Editor padding")),
            ("delay", Some("Hide after")),
            ("no-such-setting", None),
            ("", None),
        ] {
            harness.ctx.data_mut(|data| {
                data.insert_temp(egui::Id::unique("settings.search"), query.to_owned())
            });
            harness.run_steps(2);
            let measurements = widgets::settings_control_measurements();
            let highlighted: Vec<_> = measurements
                .iter()
                .filter(|m| m.label.starts_with("search_match."))
                .collect();
            assert_eq!(
                highlighted.len(),
                usize::from(expected.is_some()),
                "{query}"
            );
            if let Some(label) = expected {
                assert_eq!(highlighted[0].label, format!("search_match.{label}"));
                for prefix in ["label", "inner_select_row"] {
                    let control = measurements
                        .iter()
                        .find(|m| m.label == format!("{prefix}.{label}"))
                        .unwrap();
                    assert!(
                        highlighted[0].rect.contains_rect(control.rect),
                        "{query}: {prefix}: {:?} in {:?}",
                        control.rect,
                        highlighted[0].rect
                    );
                }
            }
            if query == "delay" {
                harness.get_by_role_and_label(egui::Role::ComboBox, "Hide after");
            }
            assert_eq!(harness.state().state.app_settings, original, "{query}");
        }
        assert!(
            harness
                .query_by_role_and_label(egui::Role::ComboBox, "Hide after")
                .is_none()
        );
    }
}
