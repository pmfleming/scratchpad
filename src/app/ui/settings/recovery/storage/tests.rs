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
fn storage_overview_is_aligned_and_technical_details_expand_in_both_themes() {
    for dark in [true, false] {
        let mut usage = sample_usage();
        usage.stored.archives = 6;
        usage.stored.recovery_points = 10;
        usage.stored.live_text = 58;
        let mut harness = Harness::builder()
            .with_size(egui::vec2(740.0, 1200.0))
            .with_context_setup(move |ctx| {
                setup(ctx);
                ctx.set_visuals(if dark { egui::Visuals::dark() } else { egui::Visuals::light() });
            })
            .build_ui_state(|ui, usage| {
                SettingsUi::apply_typography(ui);
                ui.set_width(684.0);
                render(ui, usage);
                location(ui, Path::new("/home/example/a/very/long/data/directory/that/should/not/widen/the/settings/card/scratchpad"));
            }, usage);
        harness.run();
        for label in [
            "Data files",
            "9.77 MiB",
            "Closed archives",
            "6",
            "Recovery points",
            "10",
            "Session database",
            "4.00 KiB",
            "Settings files",
            "1.92 KiB",
            "Allocated on disk",
        ] {
            harness.get_by_label(label);
        }
        let database = harness.get_by_label("4.00 KiB").rect();
        let settings = harness.get_by_label("1.92 KiB").rect();
        assert!((database.right() - settings.right()).abs() < 1.0);
        assert!(database.right() <= 700.0);
        assert!(harness.query_by_label("Undo history").is_none());
        harness.get_by_label("Stored recovery data").click();
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
            "58 B",
        ] {
            harness.get_by_label(label);
        }
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
                ui.set_width(684.0);
                render(ui, usage);
            },
            usage,
        );
    harness.run();
    harness.get_by_label("9.77 MiB");
    harness.get_by_label("Recovery details unavailable");
    harness.get_by_label("database is unavailable");
    assert_eq!(harness.get_all_by_label("—").count(), 2);
    assert!(harness.query_by_label("Stored recovery data").is_none());
    assert!(harness.query_by_label("Allocated on disk").is_none());
}
