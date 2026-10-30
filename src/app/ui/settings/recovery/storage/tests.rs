use super::*;
use egui_kittest::{Harness, kittest::Queryable};

fn sample_usage() -> StorageUsage {
    StorageUsage {
        database: 4096,
        journal: 3_786_312,
        other: 6_457_453,
        settings: 1963,
        total_file_bytes: 10_249_824,
        allocated_bytes: Some(10_527_264),
        ..Default::default()
    }
}
fn setup(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
}

#[test]
fn byte_units_keep_small_values_readable_and_preserve_large_sizes() {
    for (bytes, expected) in [
        (0, "0 B"),
        (58, "58 B"),
        (1023, "1023 B"),
        (1024, "1.00 KiB"),
        (4096, "4.00 KiB"),
        (1963, "1.92 KiB"),
        (10_249_824, "9.77 MiB"),
        (1 << 30, "1.00 GiB"),
        (1 << 40, "1.00 TiB"),
        (u64::MAX, "16.00 EiB"),
    ] {
        assert_eq!(format_bytes(bytes), expected);
    }
}
#[test]
fn payload_total_counts_exclusive_categories_once_not_overlapping_references() {
    let mut usage = sample_usage();
    let s = &mut usage.stored;
    s.live_text = 1;
    s.recovery_text = 2;
    s.shared_text = 3;
    s.undo = 4;
    s.session_metadata = 5;
    s.archive_metadata = 6;
    s.snapshot_metadata = 7;
    s.unreferenced_text = 8;
    s.reusable_pages = 999;
    s.archive_referenced_bytes = 888;
    assert_eq!(payload_total(&usage), 36);
    usage.stored.shared_text = u64::MAX;
    assert_eq!(payload_total(&usage), u64::MAX);
}
#[test]
fn both_pies_and_exact_values_are_visible_without_nested_disclosures() {
    for dark in [true, false] {
        let mut usage = sample_usage();
        usage.stored.archives = 6;
        usage.stored.recovery_points = 10;
        usage.stored.live_text = 58;
        let mut harness = Harness::builder()
            .with_size(egui::vec2(800.0, 1600.0))
            .with_context_setup(move |ctx| {
                setup(ctx);
                ctx.set_visuals(if dark {
                    egui::Visuals::dark()
                } else {
                    egui::Visuals::light()
                });
            })
            .build_ui_state(
                |ui, usage| {
                    SettingsUi::apply_typography(ui);
                    ui.set_width(634.0);
                    summary(ui, usage);
                    details(ui, usage);
                },
                usage,
            );
        harness.run();
        assert!(
            harness
                .query_by_role(egui::Role::DisclosureTriangle)
                .is_none()
        );
        assert_eq!(
            harness.get_all_by_role(egui::Role::Image).count(),
            3,
            "overview strip and two pies"
        );
        for label in [
            "Files on disk",
            "9.77 MiB",
            "Disk breakdown",
            "Recovery payloads",
            "Session database",
            "Settings files",
            "Allocated on disk",
            "Shared text",
            "Undo history",
            "Archive references",
            "Recovery payload total",
        ] {
            harness.get_by_label(label);
        }
        assert_eq!(harness.get_all_by_label("58 B").count(), 2);
        let database = harness.get_by_label("4.00 KiB").rect();
        let settings = harness.get_by_label("1.92 KiB").rect();
        assert!((database.right() - settings.right()).abs() < 1.0);
        assert!(database.right() <= 650.0);
        assert!(harness.query_by_label("Copy data directory").is_none());
    }
}
#[test]
fn storage_summary_and_legend_columns_keep_their_hierarchy_when_resized() {
    use crate::app::ui::settings::widgets::{
        reset_settings_layout_measurements, settings_control_measurements,
    };
    for dark in [true, false] {
        for width in [300.0, 634.0, 900.0] {
            reset_settings_layout_measurements();
            let ctx = egui::Context::default();
            ctx.set_visuals(if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            crate::app::fonts::apply_editor_fonts(
                &ctx,
                &crate::app::fonts::EditorFontSelection::scratchpad(
                    crate::app::fonts::EditorFontPreset::Mono,
                ),
                4.0,
            )
            .unwrap();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.set_width(width);
                ui.set_min_height(2400.0);
                SettingsUi::apply_typography(ui);
                summary(ui, &sample_usage());
                details(ui, &sample_usage());
            });
            output.textures_delta.clear();
            let measurements = settings_control_measurements();
            let find = |name: &str| measurements.iter().find(|m| m.label == name).unwrap().rect;
            let total = find("storage.total");
            assert!(total.height() > 1.8 * find("usage.value.Session database").height());
            for labels in [
                vec![
                    "Other data files",
                    "Session database",
                    "Write-ahead log & shared memory",
                    "Settings files",
                ],
                payload_rows(&sample_usage())
                    .iter()
                    .map(|row| row.0)
                    .collect(),
            ] {
                let reference = find(&format!("usage.value.{}", labels[0]));
                let mut previous_bottom = 0.0_f32;
                for label in labels {
                    let marker = find(&format!("legend.marker.{label}"));
                    let name = find(&format!("usage.label.{label}"));
                    let value = find(&format!("usage.value.{label}"));
                    assert!((name.left() - marker.right() - 9.0).abs() < 1.0, "{label}");
                    assert!((value.right() - reference.right()).abs() < 1.0);
                    assert!((name.center().y - value.center().y).abs() < 1.0);
                    assert!(
                        name.right() + 11.0 <= value.left(),
                        "{label} overlaps at {width}"
                    );
                    assert!(name.top() >= previous_bottom);
                    assert!(value.right() <= width + 1.0);
                    previous_bottom = name.bottom();
                }
            }
            assert!((find("usage.label.Archive references").left() - total.left()).abs() < 1.0);
        }
    }
}

#[test]
fn unavailable_recovery_is_not_an_empty_chart() {
    let mut usage = sample_usage();
    usage.stored_error = Some("database unavailable".into());
    usage.allocated_bytes = None;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(800.0, 1000.0))
        .with_context_setup(setup)
        .build_ui_state(
            |ui, usage| {
                ui.set_width(634.0);
                summary(ui, usage);
                details(ui, usage);
            },
            usage,
        );
    harness.run();
    harness.get_by_label("Recovery details unavailable");
    assert!(harness.query_by_label("Recovery payloads").is_none());
    assert!(harness.query_by_label("Allocated on disk").is_none());
    assert_eq!(harness.get_all_by_role(egui::Role::Image).count(), 2);
}
#[test]
fn empty_pie_keeps_zero_categories_at_narrow_and_wide_widths() {
    for width in [300.0, 634.0] {
        let mut harness = Harness::builder()
            .with_size(egui::vec2(width + 16.0, 900.0))
            .build_ui(move |ui| {
                ui.set_width(width);
                chart::breakdown(ui, "empty.pie", &chart::parts(ui, &StorageUsage::default()));
            });
        harness.run();
        let rect = harness.get_by_role(egui::Role::Image).rect();
        assert!(rect.is_finite());
        assert_eq!(rect.width(), rect.height());
        assert_eq!(harness.get_all_by_label("0 B").count(), 4);
    }
    assert_eq!(counted(1, "closed tab", "closed tabs"), "1 closed tab");
}
