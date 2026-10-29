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
        ..StorageUsage::default()
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
    let stored = &mut usage.stored;
    stored.live_text = 1;
    stored.recovery_text = 2;
    stored.shared_text = 3;
    stored.undo = 4;
    stored.session_metadata = 5;
    stored.archive_metadata = 6;
    stored.snapshot_metadata = 7;
    stored.unreferenced_text = 8;
    stored.reusable_pages = 999;
    stored.archive_referenced_bytes = 888;
    assert_eq!(payload_total(&usage), 36);
    usage.stored.shared_text = u64::MAX;
    assert_eq!(payload_total(&usage), u64::MAX);
}

#[test]
fn storage_summary_is_compact_and_disclosures_expand_without_shifting_values() {
    for dark in [true, false] {
        let mut usage = sample_usage();
        usage.stored.archives = 6;
        usage.stored.recovery_points = 10;
        usage.stored.live_text = 58;
        usage.stored.reusable_pages = 4096;
        usage.stored.archive_referenced_bytes = 4096;
        let mut harness = Harness::builder()
            .with_size(egui::vec2(740.0, 1200.0))
            .with_context_setup(move |ctx| {
                setup(ctx);
                ctx.set_visuals(if dark { egui::Visuals::dark() } else { egui::Visuals::light() });
            })
            .build_ui_state(|ui, usage| {
                SettingsUi::apply_typography(ui);
                ui.set_width(634.0);
                summary(ui, usage);
                details(ui, usage);
                location(ui, Path::new("/home/example/a/very/long/data/directory/that/should/not/widen/the/settings/card/scratchpad"));
            }, usage);
        harness.run();
        for label in [
            "Files on disk",
            "9.77 MiB",
            "6 closed tabs · 10 recovery points · 58 B payloads",
            "Disk breakdown",
            "Recovery details",
        ] {
            harness.get_by_label(label);
        }
        harness.get_by_role(egui::Role::Image);
        for hidden in [
            "Data files",
            "Session database",
            "Undo history",
            "Archive references",
            "Allocated on disk",
        ] {
            assert!(harness.query_by_label(hidden).is_none());
        }
        assert!(
            harness.get_by_label("Copy data directory").rect().bottom() < 280.0,
            "collapsed storage should not dominate the policy card"
        );
        harness.get_by_label("Disk breakdown").click_accesskit();
        harness.run();
        for label in [
            "Session database",
            "4.00 KiB",
            "Settings files",
            "1.92 KiB",
            "Allocated on disk",
        ] {
            harness.get_by_label(label);
        }
        assert_eq!(
            harness.get_all_by_role(egui::Role::Image).count(),
            5,
            "overview plus one bar per physical file category"
        );
        let database = harness.get_by_label("4.00 KiB").rect();
        let settings = harness.get_by_label("1.92 KiB").rect();
        assert!((database.right() - settings.right()).abs() < 1.0);
        assert!(database.right() <= 650.0);
        harness.get_by_label("Recovery details").click_accesskit();
        harness.run();
        for label in [
            "Undo history",
            "Session metadata",
            "Archive metadata",
            "Recovery point metadata",
            "Current & archived text",
            "Historical snapshot text",
            "Shared text",
            "Awaiting cleanup",
            "Reusable database space",
            "Archive references",
            "Recovery payload total",
        ] {
            harness.get_by_label(label);
        }
        assert_eq!(harness.get_all_by_label("58 B").count(), 2);
        assert_eq!(
            harness.get_all_by_role(egui::Role::Image).count(),
            13,
            "four disk bars and eight payload bars; overlapping figures stay outside the graphs"
        );
        harness.get_by_role_and_label(
            egui::Role::Image,
            "Current & archived text: 58 B (100.00% of breakdown total)",
        );
        harness.get_by_role_and_label(
            egui::Role::Image,
            "Shared text: 0 B (0.00% of breakdown total)",
        );
        harness
            .get_by_label("Copy data directory")
            .click_accesskit();
        harness.run_steps(1);
        assert!(harness.output().platform_output.commands.iter().any(|command| {
            matches!(command, egui::OutputCommand::CopyText(text) if text.ends_with("/scratchpad"))
        }));
    }
}

#[test]
fn unavailable_database_details_do_not_masquerade_as_empty_recovery() {
    let mut usage = sample_usage();
    usage.stored_error = Some("database is unavailable".into());
    usage.allocated_bytes = None;
    let mut harness = Harness::builder()
        .with_context_setup(setup)
        .with_size(egui::vec2(740.0, 700.0))
        .build_ui_state(
            |ui, usage| {
                ui.set_width(634.0);
                summary(ui, usage);
                details(ui, usage);
            },
            usage,
        );
    harness.run();
    harness.get_by_label("9.77 MiB");
    harness.get_by_label("Recovery counts and payload sizes unavailable.");
    harness.get_by_label("Recovery details unavailable");
    harness.get_by_label("database is unavailable");
    assert!(harness.query_by_label("Recovery details").is_none());
    harness.get_by_label("Disk breakdown").click_accesskit();
    harness.run();
    harness.get_by_label("Session database");
    assert!(harness.query_by_label("Allocated on disk").is_none());
}

#[test]
fn empty_usage_renders_zero_counts_and_a_finite_empty_chart() {
    let mut harness = Harness::builder()
        .with_context_setup(setup)
        .with_size(egui::vec2(740.0, 400.0))
        .build_ui(|ui| {
            ui.set_width(634.0);
            summary(ui, &StorageUsage::default());
        });
    harness.run();
    harness.get_by_label("0 B");
    harness.get_by_label("0 closed tabs · 0 recovery points · 0 B payloads");
    assert!(harness.get_by_role(egui::Role::Image).rect().is_finite());
    assert_eq!(counted(1, "closed tab", "closed tabs"), "1 closed tab");
    assert_eq!(
        counted(1, "recovery point", "recovery points"),
        "1 recovery point"
    );
}
